//! 控制台使用作者 realm 和独立有界任务队列；挂起操作交回 Host，失败时丢弃任务。

use crate::{
    EcmaBinding, ScriptError, ScriptPending, binding::diagnostics::js_error, state_adapter,
};
use boa_engine::{
    Context, JsError, JsNativeError, JsResult, JsValue, Script, Source,
    builtins::promise::PromiseState,
    job::{Job, JobExecutor, SimpleJobExecutor},
    object::builtins::JsPromise,
};
use narrava_loom_core::state::State;
use narrava_loom_protocol::HostDebugValueDto;
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
};

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum ConsoleNavigation {
    Goto { target: String },
    Back,
    Forward,
    Restart,
}

pub(crate) enum ConsoleOutcome {
    Complete(HostDebugValueDto),
    Pending(ScriptPending),
}

#[derive(Default)]
pub(crate) struct ConsoleJobs {
    normal: Rc<SimpleJobExecutor>,
    console: RefCell<Rc<SimpleJobExecutor>>,
    active: Cell<bool>,
    count: Cell<usize>,
    overflow: Cell<bool>,
}

impl ConsoleJobs {
    fn reset(&self) {
        *self.console.borrow_mut() = Rc::new(SimpleJobExecutor::default());
        self.count.set(0);
        self.overflow.set(false);
        self.active.set(false);
    }
}

impl JobExecutor for ConsoleJobs {
    fn enqueue_job(self: Rc<Self>, job: Job, context: &mut Context) {
        if !self.active.get() {
            self.normal.clone().enqueue_job(job, context);
            return;
        }
        self.count.set(self.count.get().saturating_add(1));
        if self.count.get() > 1024 {
            self.overflow.set(true);
            return;
        }
        self.console.borrow().clone().enqueue_job(job, context);
    }
    fn run_jobs(self: Rc<Self>, context: &mut Context) -> JsResult<()> {
        if !self.active.get() {
            return self.normal.clone().run_jobs(context);
        }
        let queue: Rc<SimpleJobExecutor> = self.console.borrow().clone();
        queue.run_jobs(context)?;
        if self.overflow.get() {
            return Err(JsNativeError::range()
                .with_message("控制台任务超过 1024 次 / Console job budget exceeded")
                .into());
        }
        Ok(())
    }
}

impl EcmaBinding {
    pub(crate) fn configure_console_story(
        &self,
        story: serde_json::Value,
    ) -> Result<(), ScriptError> {
        let source: String = format!(
            "__narrava.configure({{story:{story}}}); Engine.started = {}",
            !story["current"].is_null()
        );
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(&source))
            .map(|_| ())
            .map_err(|error| ScriptError::new("console.story", error.to_string()))
    }

    pub(crate) fn take_console_navigation(&self) -> Result<Option<ConsoleNavigation>, ScriptError> {
        let mut runtime = self.runtime.borrow_mut();
        let value = runtime
            .context
            .eval(Source::from_bytes("JSON.stringify(__narrava.takeEngine())"))
            .map_err(|error| ScriptError::new("console.engine", error.to_string()))?;
        let json: String = crate::js_string(&value, &mut runtime.context)?;
        serde_json::from_str(&json)
            .map_err(|error| ScriptError::new("console.engine", error.to_string()))
    }

    pub(crate) fn console_completions(
        &self,
        path: &str,
        state: &mut State,
    ) -> Result<Vec<HostDebugValueDto>, ScriptError> {
        let source: String = format!(
            "JSON.stringify(__narrava.completeConsole({}))",
            serde_json::to_string(path).expect("字符串可序列化")
        );
        let mut runtime = self.runtime.borrow_mut();
        state_adapter::with_state(&mut runtime.context, state, |context| {
            let value: JsValue = context
                .eval(Source::from_bytes(&source))
                .map_err(|error| js_error(context, "console.complete", error, None))?;
            let json: String = crate::js_string(&value, context)?;
            serde_json::from_str(&json)
                .map_err(|error| ScriptError::new("console.complete", error.to_string()))
        })
    }

    /// 解析失败时才尝试 await 包装；绝不重复执行已产生副作用的脚本。
    pub(crate) fn evaluate_console(
        &self,
        source: &str,
        state: &mut State,
    ) -> Result<ConsoleOutcome, ScriptError> {
        if source.len() > 16_384 {
            return Err(ScriptError::new(
                "console.source_limit",
                "命令不能超过 16 KiB / Command exceeds 16 KiB",
            ));
        }
        self.runtime.borrow_mut().console_path = if source.len() <= 512
            && source
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "._$".contains(character))
        {
            source.to_owned()
        } else {
            String::new()
        };
        self.console_step(state, |context, _| {
            let parsed: JsResult<Script> = Script::parse(
                Source::from_bytes(source).with_path(Path::new("console.js")),
                None,
                context,
            );
            let script: Script = match parsed {
                Ok(script) => script,
                Err(_) if source.contains("await") => {
                    let expression: String = format!("(async () => (\n{source}\n))()");
                    Script::parse(
                        Source::from_bytes(&expression).with_path(Path::new("console.js")),
                        None,
                        context,
                    )
                    .or_else(|_| {
                        let body: String = format!("(async () => {{\n{source}\n}})()");
                        Script::parse(
                            Source::from_bytes(&body).with_path(Path::new("console.js")),
                            None,
                            context,
                        )
                    })
                    .map_err(|error| {
                        js_error(context, "console.parse", error, Some("console.js"))
                    })?
                }
                Err(error) => {
                    return Err(js_error(
                        context,
                        "console.parse",
                        error,
                        Some("console.js"),
                    ));
                }
            };
            script
                .evaluate(context)
                .map_err(|error| js_error(context, "console.execute", error, Some("console.js")))
        })
    }

    pub(crate) fn resume_console(
        &self,
        pending: ScriptPending,
        state: &mut State,
    ) -> Result<ConsoleOutcome, ScriptError> {
        self.console_step(state, |context, value| {
            context
                .eval(Source::from_bytes(&format!(
                    "__narrava.resolveHostOperation({})",
                    pending.id()
                )))
                .map_err(|error| js_error(context, "console.resume", error, Some("console.js")))?;
            value.ok_or_else(|| {
                ScriptError::new(
                    "console.resume",
                    "控制台没有挂起结果 / No pending console result",
                )
            })
        })
    }

    fn console_step(
        &self,
        state: &mut State,
        evaluate: impl FnOnce(&mut Context, Option<JsValue>) -> Result<JsValue, ScriptError>,
    ) -> Result<ConsoleOutcome, ScriptError> {
        let mut runtime = self.runtime.borrow_mut();
        let previous: Option<JsValue> = runtime.console_value.take();
        let path: String = runtime.console_path.clone();
        let context: &mut Context = &mut runtime.context;
        let jobs: Rc<ConsoleJobs> = context
            .downcast_job_executor::<ConsoleJobs>()
            .expect("Runtime 使用受控队列");
        if previous.is_none() {
            jobs.reset();
        }
        jobs.active.set(true);
        let result = state_adapter::with_state(context, state, |context| {
            let value: JsValue = evaluate(context, previous)?;
            context
                .run_jobs()
                .map_err(|error| js_error(context, "console.jobs", error, Some("console.js")))?;
            console_outcome(context, value, &path)
        });
        jobs.active.set(false);
        match result {
            Ok((outcome, value)) => {
                runtime.console_value = value;
                Ok(outcome)
            }
            Err(error) => {
                discard_console(&mut runtime.context);
                Err(error)
            }
        }
    }

    pub(crate) fn cancel_console(&self) {
        let mut runtime = self.runtime.borrow_mut();
        runtime.console_value = None;
        discard_console(&mut runtime.context);
    }
}

fn discard_console(context: &mut Context) {
    context
        .downcast_job_executor::<ConsoleJobs>()
        .expect("Runtime 使用受控队列")
        .reset();
    let _discarded: JsResult<JsValue> =
        context.eval(Source::from_bytes("__narrava.discardConsoleRequests()"));
}

fn console_outcome(
    context: &mut Context,
    mut value: JsValue,
    path: &str,
) -> Result<(ConsoleOutcome, Option<JsValue>), ScriptError> {
    if let Some(promise) = value
        .as_object()
        .and_then(|object| JsPromise::from_object(object.clone()).ok())
    {
        match promise.state() {
            PromiseState::Pending => {
                let operation: JsValue = context
                    .eval(Source::from_bytes(
                        "JSON.stringify(__narrava.takeHostOperation())",
                    ))
                    .map_err(|error| js_error(context, "console.pending", error, None))?;
                let operation: serde_json::Value =
                    serde_json::from_str(&crate::js_string(&operation, context)?)
                        .map_err(|error| ScriptError::new("console.pending", error.to_string()))?;
                let (Some(id), Some(milliseconds)) =
                    (operation["id"].as_u64(), operation["milliseconds"].as_u64())
                else {
                    return Err(ScriptError::new(
                        "console.pending",
                        "只能等待一个受管 Host 操作 / Await one managed Host operation",
                    ));
                };
                return Ok((
                    ConsoleOutcome::Pending(ScriptPending::delay_operation(id, milliseconds)),
                    Some(value),
                ));
            }
            PromiseState::Fulfilled(result) => value = result,
            PromiseState::Rejected(error) => {
                return Err(js_error(
                    context,
                    "console.rejected",
                    JsError::from_opaque(error),
                    Some("console.js"),
                ));
            }
        }
    }
    let unawaited: JsValue = context
        .eval(Source::from_bytes("__narrava.takeHostOperation() !== null"))
        .map_err(|error| js_error(context, "console.pending", error, None))?;
    if unawaited.as_boolean() == Some(true) {
        return Err(ScriptError::new(
            "console.pending",
            "请返回或 await Host 操作 / Return or await the Host operation",
        ));
    }
    let formatter: JsValue = context
        .eval(Source::from_bytes(
            "(value, path) => JSON.stringify(__narrava.inspectConsoleResult(value, path))",
        ))
        .map_err(|error| js_error(context, "console.format", error, None))?;
    let output: JsValue = formatter
        .as_callable()
        .expect("内置包装为函数")
        .call(
            &JsValue::undefined(),
            &[value, boa_engine::js_string!(path).into()],
            context,
        )
        .map_err(|error| js_error(context, "console.format", error, None))?;
    let json: String = crate::js_string(&output, context)?;
    let value: HostDebugValueDto = serde_json::from_str(&json)
        .map_err(|error| ScriptError::new("console.result", error.to_string()))?;
    Ok((ConsoleOutcome::Complete(value), None))
}

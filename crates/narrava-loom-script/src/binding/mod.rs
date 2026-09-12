//! 脚本 Macro 调用、挂起恢复与内建事件投递。

pub(crate) mod diagnostics;
mod reactions;
mod requests;

use crate::{
    EcmaBinding, EcmaRuntime, ScriptError, ScriptMacroOutcome, ScriptPending, js_string,
    json_to_value, script_error, state_adapter,
};
use boa_engine::{Context, JsError, Source};
use narrava_loom_core::{
    SourceList,
    expression::{
        evaluator::ScriptCallError,
        value::{ScriptCallable, Value},
    },
    i18n::I18nCatalog,
    resource::ResourceCatalog,
    script::ScriptCallDispatcher,
    state::State,
};
use std::{cell::RefCell, rc::Rc, time::Duration};

impl EcmaBinding {
    /// 装载脚本源码与桥接并返回绑定；返回 `Rc` 供 State 的脚本分发器共享。
    pub fn load(
        sources: &SourceList,
        resources: &ResourceCatalog,
        i18n: &I18nCatalog,
        default_locale: &str,
        state: &mut State,
    ) -> Result<Rc<Self>, ScriptError> {
        Ok(Rc::new(Self {
            runtime: RefCell::new(EcmaRuntime::load(
                sources,
                resources,
                i18n,
                default_locale,
                state,
            )?),
        }))
    }

    /// 查询脚本是否注册了指定 Macro。
    pub fn has_macro(&self, name: &str) -> Result<bool, ScriptError> {
        let expression = format!(
            "__narrava.hasMacro({})",
            serde_json::to_string(name).expect("字符串必须可序列化")
        );
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map(|value| value.as_boolean().unwrap_or(false))
            .map_err(|error| script_error("script.macro", error))
    }

    /// 调用脚本 Macro；handler 未决时返回 Pending 等待 Host 操作。
    pub fn call_macro(
        &self,
        name: &str,
        arguments: &str,
        state: &mut State,
    ) -> Result<ScriptMacroOutcome, ScriptError> {
        let call = serde_json::json!({ "name": name, "arguments": arguments });
        let expression = format!(
            "globalThis.__narravaMacroResult = undefined; Promise.resolve(__narrava.invokeMacro({}, {})).then(value => {{ globalThis.__narravaMacroResult = {{ ok: true, value: JSON.stringify(value) }} }}, error => {{ globalThis.__narravaMacroResult = {{ ok: false, error }} }});",
            serde_json::to_string(name).expect("字符串必须可序列化"),
            call,
        );
        let mut runtime = self.runtime.borrow_mut();
        state_adapter::with_state(&mut runtime.context, state, |context| {
            context
                .eval(Source::from_bytes(expression.as_bytes()))
                .map_err(|error| diagnostics::js_error(context, "script.macro", error, None))?;
            context.run_jobs().map_err(|error| {
                diagnostics::js_error(context, "script.macro_jobs", error, None)
            })?;
            macro_outcome(context)
        })
    }

    /// 解决挂起的 Host 操作（如 `Host.delay` 到期）并继续运行宏。
    pub fn resume_macro(
        &self,
        pending: ScriptPending,
        state: &mut State,
    ) -> Result<ScriptMacroOutcome, ScriptError> {
        let expression = format!("__narrava.resolveHostOperation({})", pending.id);
        let mut runtime = self.runtime.borrow_mut();
        state_adapter::with_state(&mut runtime.context, state, |context| {
            context
                .eval(Source::from_bytes(expression.as_bytes()))
                .map_err(|error| {
                    diagnostics::js_error(context, "script.host_operation", error, None)
                })?;
            context.run_jobs().map_err(|error| {
                diagnostics::js_error(context, "script.macro_jobs", error, None)
            })?;
            macro_outcome(context)
        })
    }

    /// 由 Host 把已经进入的 Core 生命周期事实投递给游戏脚本 Event。
    pub fn emit_builtin_event(
        &self,
        name: &str,
        payload: &serde_json::Value,
    ) -> Result<u64, ScriptError> {
        let expression = format!(
            "__narrava.emitBuiltin({}, {})",
            serde_json::to_string(name).expect("事件名必须可序列化"),
            payload
        );
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map_err(|error| script_error("script.event", error))?
            .as_number()
            .and_then(|sequence| {
                (sequence.is_finite() && sequence >= 0.0).then_some(sequence as u64)
            })
            .ok_or_else(|| ScriptError::new("script.event", "Event.emit 没有返回有效序号"))
    }
}

impl ScriptCallDispatcher for EcmaBinding {
    /// 供 Core 表达式求值调用脚本函数。
    fn call(
        &self,
        callable: &ScriptCallable,
        arguments: Vec<Value>,
        state: &mut State,
    ) -> Result<Value, ScriptCallError> {
        self.runtime.borrow_mut().call(callable, arguments, state)
    }
}

/// 读宏调用结果：已结算取返回值；未结算则把等待的 Host 操作转成 Pending。
fn macro_outcome(context: &mut Context) -> Result<ScriptMacroOutcome, ScriptError> {
    let settled = context
        .eval(Source::from_bytes(
            "globalThis.__narravaMacroResult !== undefined",
        ))
        .map_err(|error| script_error("script.macro", error))?;
    if settled.as_boolean() != Some(true) {
        let operation = context
            .eval(Source::from_bytes(
                "JSON.stringify(__narrava.takeHostOperation())",
            ))
            .map_err(|error| script_error("script.host_operation", error))?;
        if operation.is_undefined() {
            return Err(ScriptError::new(
                "script.macro_unmanaged_promise",
                "Macro 返回了未决 Promise，但没有等待 Host 操作",
            ));
        }
        let operation: serde_json::Value =
            serde_json::from_str(&js_string(&operation, context)?)
                .map_err(|error| ScriptError::new("script.host_operation", error.to_string()))?;
        if operation.is_null() {
            return Err(ScriptError::new(
                "script.macro_unmanaged_promise",
                "Macro 返回了未决 Promise，但没有等待 Host 操作",
            ));
        }
        if operation["kind"] == "invalid-count" {
            return Err(ScriptError::new(
                "script.host_operation_count",
                "一个 Macro 同时只能等待一个 Host 操作",
            ));
        }
        if operation["kind"] != "delay" {
            return Err(ScriptError::new(
                "script.host_operation_kind",
                "Host 返回了未知异步操作",
            ));
        }
        let id: u64 = operation["id"]
            .as_u64()
            .ok_or_else(|| ScriptError::new("script.host_operation", "Host 操作 ID 无效"))?;
        let milliseconds: u64 = operation["milliseconds"]
            .as_u64()
            .ok_or_else(|| ScriptError::new("script.host_operation", "Host.delay 毫秒数无效"))?;
        return Ok(ScriptMacroOutcome::Pending(ScriptPending {
            id,
            delay: Duration::from_millis(milliseconds),
        }));
    }

    let failed = context
        .eval(Source::from_bytes("__narravaMacroResult.ok === false"))
        .map_err(|error| diagnostics::js_error(context, "script.macro", error, None))?;
    if failed.as_boolean() == Some(true) {
        let error = context
            .eval(Source::from_bytes("__narravaMacroResult.error"))
            .map_err(|error| diagnostics::js_error(context, "script.macro", error, None))?;
        return Err(diagnostics::js_error(
            context,
            "script.macro_rejected",
            JsError::from_opaque(error),
            None,
        ));
    }
    let result = context
        .eval(Source::from_bytes("JSON.stringify(__narravaMacroResult)"))
        .map_err(|error| script_error("script.macro", error))?;
    let result: serde_json::Value = serde_json::from_str(&js_string(&result, context)?)
        .map_err(|error| ScriptError::new("script.macro", error.to_string()))?;
    let value: Value = match result["value"].as_str() {
        None => Value::Undefined,
        Some(json) => json_to_value(
            &serde_json::from_str(json)
                .map_err(|error| ScriptError::new("script.macro_value", error.to_string()))?,
        )?,
    };
    Ok(ScriptMacroOutcome::Complete(value))
}

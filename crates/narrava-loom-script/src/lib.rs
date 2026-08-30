//! Narrava 游戏的 ECMAScript 脚本执行。
//!
//! 宿主无关：本 crate 提供 `EcmaBinding`（Boa 引擎 + Oxc 转译）、State/Resource
//! 桥与 Macro/Save/Event 桥，Host（Tauri/TUI）只需提供脚本源码与资源目录。

use std::{cell::RefCell, error::Error, fmt, path::Path, rc::Rc, time::Duration};

use boa_engine::{Context, JsValue, Source};

/// 官方 Runtime 单个 ECMAScript 循环允许执行的最大迭代次数。
const DEFAULT_SCRIPT_LOOP_LIMIT: u64 = 1_000_000;

/// 建立带明确执行上限的 Boa Context，避免作者脚本永久占住 Runtime Worker。
fn runtime_context(loop_limit: u64) -> Context {
    let mut context = Context::default();
    context
        .runtime_limits_mut()
        .set_loop_iteration_limit(loop_limit);
    context
}

use narrava_loom_core::{
    SourceList,
    expression::{
        evaluator::ScriptCallError,
        value::{ScriptCallable, Value},
    },
    i18n::I18nCatalog,
    resource::ResourceCatalog,
    script::{ScriptBundle, ScriptCallDispatcher, ScriptFunctionHost},
    state::State,
};
use oxc::{
    allocator::Allocator,
    codegen::Codegen,
    parser::Parser,
    semantic::SemanticBuilder,
    span::SourceType,
    transformer::{TransformOptions, Transformer, TypeScriptOptions},
};

mod adapter;
pub mod dispatch;
pub mod protocol_adapter;
mod reaction_bridge;
mod resource_bridge;
mod session;
mod session_handle;
mod state_bridge;

pub use adapter::ScriptAdapter;
pub use session::{RuntimeServices, RuntimeSession};
pub use session_handle::{RuntimeSessionDriver, RuntimeSessionHandle};

/// ECMAScript 装载、桥接或执行失败。
///
/// Script crate 使用自己的稳定错误边界，不把 Tauri IPC DTO 泄漏给 TUI 或未来 Binding。
/// 具体 Host 在最外层决定如何把 `code` 与 `message` 编码到本平台的错误协议。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    /// 与具体 Host 无关的稳定错误码。
    pub code: String,
    /// 面向开发者的错误说明。
    pub message: String,
}

impl ScriptError {
    /// 构造一条脚本运行时错误。
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for ScriptError {}

impl From<narrava_loom_protocol::HostErrorDto> for ScriptError {
    fn from(error: narrava_loom_protocol::HostErrorDto) -> Self {
        Self {
            code: error.code,
            message: error.message,
        }
    }
}

/// 由 Bun 在开发期打包；Runtime 只编译期嵌入生成的 ECMAScript。
const BOOTSTRAP: &str = include_str!("bootstrap.generated.js");

fn bootstrap_source() -> &'static str {
    BOOTSTRAP
}

/// 持有 Boa 引擎上下文的脚本运行时（一次启动一个）。
pub struct EcmaRuntime {
    context: Context,
}

/// 对运行时上下文的借用访问边界：宏、保存、内置事件与脚本函数调用都经由它。
pub struct EcmaBinding {
    runtime: RefCell<EcmaRuntime>,
}

/// 脚本宏挂起等待 Host 操作的凭据（当前只有 `Host.delay`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptPending {
    id: u64,
    delay: Duration,
}

impl ScriptPending {
    /// 建立一个 delay 挂起凭据。Adapter 选择 ID，Runtime 只将其视为不透明值。
    pub fn delay_operation(id: u64, milliseconds: u64) -> Self {
        Self {
            id,
            delay: Duration::from_millis(milliseconds),
        }
    }

    /// Runtime 用来映射 resume/cancel 命令的不透明操作身份。
    pub fn id(&self) -> u64 {
        self.id
    }

    /// 需等待的时长。
    pub fn delay(&self) -> Duration {
        self.delay
    }

    /// Protocol 使用整数毫秒传输等待时长，避免泄漏 Rust `Duration`。
    pub fn milliseconds(&self) -> u64 {
        u64::try_from(self.delay.as_millis()).unwrap_or(u64::MAX)
    }
}

/// 脚本宏的一次调用结果：立即完成或挂起等待 Host 操作。
#[derive(Debug, PartialEq)]
pub enum ScriptMacroOutcome {
    /// 宏已完成并返回 Core 值。
    Complete(Value),
    /// 宏挂起，需在 `ScriptPending::delay` 后由 Host 恢复。
    Pending(ScriptPending),
}

impl EcmaBinding {
    /// 取出脚本登记的 Save 请求（operation, target）；无请求时返回 `None`。
    pub fn take_save(&self) -> Result<Option<(String, String)>, ScriptError> {
        let mut runtime = self.runtime.borrow_mut();
        let value = runtime
            .context
            .eval(Source::from_bytes("JSON.stringify(__narrava.takeSave())"))
            .map_err(|error| script_error("script.save_request", error))?;
        if value.is_undefined() {
            return Ok(None);
        }
        let json = js_string(&value, &mut runtime.context)?;
        let request: serde_json::Value = serde_json::from_str(&json)
            .map_err(|error| ScriptError::new("script.save_request", error.to_string()))?;
        if request.is_null() {
            return Ok(None);
        }
        let operation = request["operation"]
            .as_str()
            .ok_or_else(|| ScriptError::new("script.save_request", "Save operation 无效"))?;
        let target = request["target"]
            .as_str()
            .ok_or_else(|| ScriptError::new("script.save_request", "Save target 无效"))?;
        Ok(Some((operation.to_owned(), target.to_owned())))
    }

    /// 把存档结果回传给脚本的 `Save.after` 钩子。
    pub fn complete_save(
        &self,
        operation: &str,
        target: &str,
        outcome: Result<(), &str>,
    ) -> Result<(), ScriptError> {
        let completion = match outcome {
            Ok(()) => {
                serde_json::json!({ "operation": operation, "target": target, "succeeded": true })
            }
            Err(error) => {
                serde_json::json!({ "operation": operation, "target": target, "succeeded": false, "error": error })
            }
        };
        let expression = format!("__narrava.completeSave({completion})");
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map(|_| ())
            .map_err(|error| script_error("script.save_after", error))
    }

    /// 恢复存档后同步脚本变量视图。
    ///
    /// State API 直接读取活动 Rust State，恢复存档后不再维护或刷新 JS 镜像。
    pub fn sync_variables(&self, _state: &State) -> Result<(), ScriptError> {
        Ok(())
    }

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
            "globalThis.__narravaMacroResult = undefined; Promise.resolve(__narrava.invokeMacro({}, {})).then(value => {{ globalThis.__narravaMacroResult = {{ ok: true, value: JSON.stringify(value) }} }}, error => {{ globalThis.__narravaMacroResult = {{ ok: false, value: String(error) }} }});",
            serde_json::to_string(name).expect("字符串必须可序列化"),
            call,
        );
        let mut runtime = self.runtime.borrow_mut();
        state_bridge::with_state(&mut runtime.context, state, |context| {
            context
                .eval(Source::from_bytes(expression.as_bytes()))
                .map_err(|error| script_error("script.macro", error))?;
            context
                .run_jobs()
                .map_err(|error| script_error("script.macro_jobs", error))?;
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
        state_bridge::with_state(&mut runtime.context, state, |context| {
            context
                .eval(Source::from_bytes(expression.as_bytes()))
                .map_err(|error| script_error("script.host_operation", error))?;
            context
                .run_jobs()
                .map_err(|error| script_error("script.macro_jobs", error))?;
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

    /// 同步 Host 已确认的运行语言，使脚本侧 `I18n.locale` 与实际渲染语言一致。
    pub fn select_locale(&self, locale: &str) -> Result<(), ScriptError> {
        let configuration = serde_json::json!({ "locale": locale });
        let expression: String = format!("__narrava.configure({configuration})");
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map_err(|error| script_error("script.i18n_locale", error))?;
        Ok(())
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

impl EcmaRuntime {
    /// 安装 State/Resource 桥接、注入启动脚本并执行全部转译后的模块。
    pub fn load(
        sources: &SourceList,
        resources: &ResourceCatalog,
        i18n: &I18nCatalog,
        default_locale: &str,
        state: &mut State,
    ) -> Result<Self, ScriptError> {
        let mut context = runtime_context(DEFAULT_SCRIPT_LOOP_LIMIT);
        state_bridge::install(&mut context)
            .map_err(|error| script_error("script.state_bridge", error))?;
        resource_bridge::install(&mut context, resources.clone())
            .map_err(|error| script_error("script.resource_bridge", error))?;
        reaction_bridge::install(&mut context)
            .map_err(|error| script_error("script.reaction_bridge", error))?;
        let configuration = serde_json::json!({
            "defaultLocale": default_locale,
            "locale": default_locale,
            "i18nExport": serde_json::to_string_pretty(&i18n.template(default_locale))
                .map_err(|error| ScriptError::new("script.i18n_export", error.to_string()))?,
        });
        let configure = format!("__narrava.configure({configuration})");
        let modules = ScriptBundle::from_sources(sources)
            .modules()
            .iter()
            .map(|module| transpile(module.path(), module.source()))
            .collect::<Result<Vec<_>, _>>()?;
        state_bridge::with_state(&mut context, state, |context| {
            let bootstrap: &str = bootstrap_source();
            context
                .eval(Source::from_bytes(&bootstrap))
                .map_err(|error| script_error("script.bootstrap", error))?;
            context
                .eval(Source::from_bytes(configure.as_bytes()))
                .map_err(|error| script_error("script.configure", error))?;
            for javascript in &modules {
                context
                    .eval(Source::from_bytes(javascript))
                    .map_err(|error| script_error("script.execute", error))?;
                context
                    .run_jobs()
                    .map_err(|error| script_error("script.jobs", error))?;
            }
            Ok::<(), ScriptError>(())
        })?;
        Ok(Self { context })
    }
}

impl ScriptFunctionHost for EcmaRuntime {
    /// 供 Core 以 JSON 形式调用脚本函数并读回结果。
    fn call(
        &mut self,
        callable: &ScriptCallable,
        arguments: Vec<Value>,
        state: &mut State,
    ) -> Result<Value, ScriptCallError> {
        let arguments = arguments
            .iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ScriptCallError::Failed)?;
        let expression = format!(
            "JSON.stringify(__narrava.call({}, {}))",
            callable.id(),
            serde_json::to_string(&arguments).expect("JSON Value 必须可序列化")
        );
        state_bridge::with_state(&mut self.context, state, |context| {
            let result = context
                .eval(Source::from_bytes(expression.as_bytes()))
                .map_err(|_| ScriptCallError::Failed)?;
            context.run_jobs().map_err(|_| ScriptCallError::Failed)?;
            if result.is_undefined() {
                return Ok(Value::Undefined);
            }
            let json = result
                .to_string(context)
                .map_err(|_| ScriptCallError::Failed)?
                .to_std_string_escaped();
            let value: serde_json::Value =
                serde_json::from_str(&json).map_err(|_| ScriptCallError::Failed)?;
            json_to_value(&value).map_err(|_| ScriptCallError::Failed)
        })
    }
}

/// 构造脚本错误。
fn script_error(code: &str, error: impl std::fmt::Display) -> ScriptError {
    ScriptError::new(code, error.to_string())
}

/// JsValue → Rust 字符串。
fn js_string(value: &JsValue, context: &mut Context) -> Result<String, ScriptError> {
    value
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
        .map_err(|error| script_error("script.value", error))
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

    let result = context
        .eval(Source::from_bytes("JSON.stringify(__narravaMacroResult)"))
        .map_err(|error| script_error("script.macro", error))?;
    let result: serde_json::Value = serde_json::from_str(&js_string(&result, context)?)
        .map_err(|error| ScriptError::new("script.macro", error.to_string()))?;
    if result["ok"] != true {
        return Err(ScriptError::new(
            "script.macro_rejected",
            result["value"].as_str().unwrap_or("Promise 被拒绝"),
        ));
    }
    let value: Value = match result["value"].as_str() {
        None => Value::Undefined,
        Some(json) => json_to_value(
            &serde_json::from_str(json)
                .map_err(|error| ScriptError::new("script.macro_value", error.to_string()))?,
        )?,
    };
    Ok(ScriptMacroOutcome::Complete(value))
}

/// JSON → Core 值（供 Input 与宏返回值转换）。
pub fn json_to_value(value: &serde_json::Value) -> Result<Value, ScriptError> {
    match value {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(value) => Ok(Value::Boolean(*value)),
        serde_json::Value::Number(value) => value
            .as_f64()
            .map(Value::Number)
            .ok_or_else(|| ScriptError::new("script.value", "数值超出范围")),
        serde_json::Value::String(value) => Ok(Value::string(value.as_str())),
        serde_json::Value::Array(values) => values
            .iter()
            .map(json_to_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::array),
        serde_json::Value::Object(values) => values
            .iter()
            .map(|(name, value)| Ok((name.clone(), json_to_value(value)?)))
            .collect::<Result<Vec<_>, ScriptError>>()
            .map(Value::object),
    }
}

/// Core 值 → JSON；函数/命名空间不可序列化。
fn value_to_json(value: &Value) -> Result<serde_json::Value, ()> {
    match value {
        Value::Undefined | Value::Null => Ok(serde_json::Value::Null),
        Value::Boolean(value) => Ok(serde_json::Value::Bool(*value)),
        Value::Number(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .ok_or(()),
        Value::String(value) => value
            .to_unicode_string()
            .map(serde_json::Value::String)
            .ok_or(()),
        Value::Array(value) => value
            .snapshot()
            .iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        Value::Object(value) => value
            .snapshot()
            .into_iter()
            .map(|(name, value)| Ok((name, value_to_json(&value)?)))
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(serde_json::Value::Object),
        Value::Callable(_) | Value::ScriptCallable(_) | Value::Namespace(_) => Err(()),
    }
}

/// 按扩展名转译脚本：`.js` 原样返回，`.ts` 走 oxc 解析/语义/转换/代码生成。
pub fn transpile(path: &str, source: &str) -> Result<String, ScriptError> {
    if path.ends_with(".js") {
        return Ok(source.to_owned());
    }
    let source_type = SourceType::from_path(path)
        .map_err(|error| ScriptError::new("script.language", error.to_string()))?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(ScriptError::new(
            "script.parse",
            parsed.diagnostics[0].to_string(),
        ));
    }
    let mut program = parsed.program;
    let semantic = SemanticBuilder::new().build(&program);
    if !semantic.diagnostics.is_empty() {
        return Err(ScriptError::new(
            "script.semantic",
            semantic.diagnostics[0].to_string(),
        ));
    }
    let options = TransformOptions {
        typescript: TypeScriptOptions::default(),
        ..TransformOptions::default()
    };
    let transformed = Transformer::new(&allocator, Path::new(path), &options)
        .build_with_scoping(semantic.semantic.into_scoping(), &mut program);
    if !transformed.diagnostics.is_empty() {
        return Err(ScriptError::new(
            "script.transform",
            transformed.diagnostics[0].to_string(),
        ));
    }
    Ok(Codegen::new().build(&program).code)
}

#[cfg(test)]
mod tests;

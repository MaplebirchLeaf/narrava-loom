//! Narrava 游戏的 ECMAScript 绑定与 Host-neutral Runtime Session。

use std::{cell::RefCell, error::Error, fmt, rc::Rc, time::Duration};

use boa_engine::{Context, JsValue};
use narrava_loom_core::{
    expression::value::{ScriptCallable, Value},
    reaction::ReactionRegistry,
};

mod binding;
mod console;
pub mod dispatch;
mod ecma;
mod location_adapter;
mod logger_adapter;
pub mod protocol_adapter;
mod random_adapter;
mod reaction_adapter;
mod reaction_runtime;
mod refresh;
mod resource_adapter;
mod session;
mod state_adapter;
mod value;

pub use ecma::transpile;
pub use session::{RuntimeData, RuntimeSession};
pub use value::json_to_value;
use value::value_to_json;

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
    /// 原始诊断级别与源码归属，供宿主展示；未知位置保持为空。
    pub severity: Option<narrava_loom_protocol::DiagnosticSeverityDto>,
    pub location: Option<Box<narrava_loom_protocol::DiagnosticLocationDto>>,
}

impl ScriptError {
    /// 构造一条脚本运行时错误。
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            severity: None,
            location: None,
        }
    }

    /// 穿过脚本边界时保留诊断信息，不重新压成只有错误文本的值。
    pub fn into_host_error(self) -> narrava_loom_protocol::HostErrorDto {
        narrava_loom_protocol::HostErrorDto {
            code: self.code,
            message: self.message,
            severity: self.severity,
            location: self.location,
        }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for ScriptError {}

impl narrava_loom_core::host::HostDispatchError for ScriptError {
    fn into_diagnostic(
        self,
        _fallback_code: &str,
        _fallback_message: &str,
    ) -> narrava_loom_core::diagnostic::Diagnostic {
        protocol_adapter::core_diagnostic(&self.into_host_error())
    }
}

impl From<narrava_loom_protocol::HostErrorDto> for ScriptError {
    fn from(error: narrava_loom_protocol::HostErrorDto) -> Self {
        Self {
            code: error.code,
            message: error.message,
            severity: error.severity,
            location: error.location,
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
    console_value: Option<boa_engine::JsValue>,
    console_path: String,
    context: Context,
    reactions: Rc<RefCell<ReactionRegistry<ScriptCallable>>>,
}

/// 对运行时上下文的借用访问边界：宏、保存、内置事件与脚本函数调用都经由它。
pub struct EcmaBinding {
    runtime: RefCell<EcmaRuntime>,
}

/// 作者 `Event.emit` 进入 Runtime 安全队列的拥有型记录。
#[derive(Clone, Debug, PartialEq)]
pub struct QueuedAuthorEvent {
    pub name: String,
    pub payload: Value,
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

#[cfg(test)]
mod tests;

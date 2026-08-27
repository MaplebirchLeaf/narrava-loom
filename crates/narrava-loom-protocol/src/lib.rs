//! 跨 Host 的 Surface 传输协议。
//!
//! 语义类型（`Surface`、`SurfaceNode` 等）保留在 `narrava-loom-core::protocol`，
//! 由 Core 产生并被 Core 内部使用；本 crate 只承载 Host 与 Core 之间的稳定传输层：
//! IPC 错误 DTO、节点/更新 DTO 以及脚本 Surface builder 值的受验证转换。
//! Host 同时依赖 `narrava-loom-core`（执行）与本 crate（传输）。

use std::fmt;

pub mod protocol_bridge;
pub mod protocol_dto;

pub use protocol_dto::{HostNodeDto, HostReplaceTargetDto, HostUpdateDto, convert};

/// IPC 边界只暴露稳定代码与可显示消息，不泄漏 Rust 错误对象。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct HostErrorDto {
    /// 稳定的错误码（IPC 前端可据此分流）。
    pub code: String,
    /// 可显示的中文错误消息。
    pub message: String,
}

impl HostErrorDto {
    /// 构造带稳定错误码的 Host 错误。
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }

    /// 从 Core 诊断构造 Host 错误。
    pub fn diagnostic(diagnostic: narrava_loom_core::diagnostic::Diagnostic) -> Self {
        Self {
            code: diagnostic.code,
            message: diagnostic.message,
        }
    }
}

impl fmt::Display for HostErrorDto {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

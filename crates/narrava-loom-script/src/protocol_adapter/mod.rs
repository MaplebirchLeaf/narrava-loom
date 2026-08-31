//! Core、Script Surface builder 与纯数据 Protocol 之间的 Runtime 适配。
//!
//! 这些类型与转换依赖 Core，因此属于 Native Script Runtime 实现，
//! 不得放回零 Core 依赖的 `narrava-loom-protocol`。

mod conversion;
pub mod protocol_bridge;
mod protocol_dto;
mod surface;

pub use protocol_dto::encode_host_update;
pub use surface::*;

/// 把 Core 诊断降级为可跨 Runtime/Host 传输的错误。
pub fn diagnostic(
    diagnostic: narrava_loom_core::diagnostic::Diagnostic,
) -> narrava_loom_protocol::HostErrorDto {
    narrava_loom_protocol::HostErrorDto {
        code: diagnostic.code,
        message: diagnostic.message,
    }
}

#[cfg(test)]
mod tests;

//! Core、Script Surface builder 与纯数据 Protocol 之间的 Runtime 适配。
//!
//! 这些类型与转换依赖 Core，因此属于 Native Script Runtime 实现，
//! 不得放回零 Core 依赖的 `narrava-loom-protocol`。

mod host_update;
pub mod script_output;

pub use host_update::encode_host_update;

/// 保留 Core 的严重性与原始位置；仅有脚本来源时不补造字节范围。
pub fn diagnostic(
    diagnostic: narrava_loom_core::diagnostic::Diagnostic,
) -> narrava_loom_protocol::HostErrorDto {
    use narrava_loom_core::diagnostic::DiagnosticSeverity;
    use narrava_loom_protocol::{DiagnosticLocationDto, DiagnosticSeverityDto};

    let location: Option<DiagnosticLocationDto> = diagnostic
        .location
        .map(|location| DiagnosticLocationDto {
            source: location.source,
            start: Some(location.start),
            end: Some(location.end),
            line: Some(location.line),
            column: Some(location.column),
            generated: false,
        })
        .or_else(|| {
            diagnostic.source.map(|source| DiagnosticLocationDto {
                source: source.path,
                start: None,
                end: None,
                line: source.line,
                column: source.column,
                generated: source.generated,
            })
        });
    narrava_loom_protocol::HostErrorDto {
        code: diagnostic.code,
        message: diagnostic.message,
        severity: Some(match diagnostic.severity {
            DiagnosticSeverity::Error => DiagnosticSeverityDto::Error,
            DiagnosticSeverity::Warning => DiagnosticSeverityDto::Warning,
            DiagnosticSeverity::Note => DiagnosticSeverityDto::Note,
        }),
        location: location.map(Box::new),
    }
}

/// 脚本或 Host 错误进入 Core 日志时保留已经验证的定位信息。
pub fn core_diagnostic(
    error: &narrava_loom_protocol::HostErrorDto,
) -> narrava_loom_core::diagnostic::Diagnostic {
    use narrava_loom_core::diagnostic::{
        Diagnostic, DiagnosticLocation, DiagnosticSeverity, DiagnosticSource,
    };
    use narrava_loom_protocol::DiagnosticSeverityDto;

    let severity: DiagnosticSeverity = match error.severity {
        Some(DiagnosticSeverityDto::Warning) => DiagnosticSeverity::Warning,
        Some(DiagnosticSeverityDto::Note) => DiagnosticSeverity::Note,
        Some(DiagnosticSeverityDto::Error) | None => DiagnosticSeverity::Error,
    };
    let mut result: Diagnostic = Diagnostic::new(&error.code, severity, &error.message);
    if let Some(location) = &error.location {
        match (location.start, location.end, location.line, location.column) {
            (Some(start), Some(end), Some(line), Some(column)) if !location.generated => {
                result.location = Some(DiagnosticLocation {
                    source: location.source.clone(),
                    start,
                    end,
                    line,
                    column,
                });
            }
            _ => {
                result.source = Some(Box::new(DiagnosticSource {
                    path: location.source.clone(),
                    generated: location.generated,
                    line: location.line,
                    column: location.column,
                }))
            }
        }
    }
    result
}

#[cfg(test)]
mod tests;

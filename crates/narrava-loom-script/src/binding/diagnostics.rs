//! 保留 Boa 的异常内容和已知源码归属，不把生成位置当成 TypeScript 原位置。

use boa_engine::{Context, Finalize, JsData, JsError, Trace};
use narrava_loom_core::{
    SourceKind, SourceList,
    diagnostic::{Diagnostic, DiagnosticLocator, DiagnosticSeverity},
    expression::evaluator::ScriptCallError,
};
use narrava_loom_protocol::{DiagnosticLocationDto, DiagnosticSeverityDto};
use oxc::diagnostics::OxcDiagnostic;

use crate::{ScriptError, protocol_adapter};

#[derive(Trace, Finalize, JsData)]
struct ScriptSources {
    #[unsafe_ignore_trace]
    paths: Vec<(String, bool)>,
}

pub(crate) fn install(context: &mut Context, sources: &SourceList) {
    let paths: Vec<(String, bool)> = sources
        .items
        .iter()
        .filter_map(|source| match source.kind {
            SourceKind::Twee => None,
            SourceKind::TypeScript | SourceKind::JavaScript => Some((
                source.path.as_str().to_owned(),
                source.kind == SourceKind::TypeScript,
            )),
        })
        .collect();
    context.insert_data(ScriptSources { paths });
}

pub(crate) fn js_error(
    context: &mut Context,
    code: &str,
    error: JsError,
    fallback: Option<&str>,
) -> ScriptError {
    let trace: String = error
        .try_native(context)
        .map_or_else(|_| error.to_string(), |native| native.to_string());
    // Boa 的执行预算错误不能转换成 JavaScript Error 对象。
    let message: String = if error
        .as_native()
        .is_some_and(|error| error.is_runtime_limit())
    {
        trace.clone()
    } else {
        error
            .to_opaque(context)
            .to_string(context)
            .map(|value| value.to_std_string_escaped())
            .unwrap_or_else(|_| trace.clone())
    };
    let location: Option<DiagnosticLocationDto> = source_location(context, &trace)
        .or_else(|| source_location(context, &error.to_string()))
        .or_else(|| {
            fallback.map(|path| DiagnosticLocationDto {
                source: path.to_owned(),
                start: None,
                end: None,
                line: None,
                column: None,
                generated: path.ends_with(".ts"),
            })
        });
    ScriptError {
        code: code.to_owned(),
        message,
        severity: Some(DiagnosticSeverityDto::Error),
        location: location.map(Box::new),
    }
}

/// Boa 0.21 仅通过 Display 暴露异常保存的位置；只提取登记源码的明确坐标。
fn source_location(context: &Context, trace: &str) -> Option<DiagnosticLocationDto> {
    let sources: &ScriptSources = context.get_data::<ScriptSources>()?;
    for line in trace.lines() {
        for (path, generated) in &sources.paths {
            let marker: String = format!("({path}");
            let Some((_, tail)) = line.split_once(&marker) else {
                continue;
            };
            let suffix: &str = tail.split_once(')')?.0;
            if !suffix.is_empty() && !suffix.starts_with(':') {
                continue;
            }
            let mut coordinates = suffix.strip_prefix(':').unwrap_or("").split(':');
            let line: Option<usize> = coordinates
                .next()
                .and_then(|value| value.parse().ok())
                .filter(|value| *value > 0);
            let column: Option<usize> = coordinates
                .next()
                .and_then(|value| value.parse().ok())
                .filter(|value| *value > 0);
            return Some(DiagnosticLocationDto {
                source: path.clone(),
                start: None,
                end: None,
                line,
                column,
                generated: *generated,
            });
        }
    }
    None
}

pub(crate) fn oxc_error(
    code: &str,
    path: &str,
    source: &str,
    error: &OxcDiagnostic,
) -> ScriptError {
    let mut diagnostic: Diagnostic =
        Diagnostic::new(code, DiagnosticSeverity::Error, &error.to_string())
            .with_source(path, false);
    if let Some(label) = error.labels.first()
        && let (Ok(start), Ok(length)) = (
            usize::try_from(label.offset()),
            usize::try_from(label.len()),
        )
        && let Some(end) = start.checked_add(length)
        && let Ok(location) = DiagnosticLocator::new(path, source).locate(0, start, end)
    {
        diagnostic = diagnostic.with_location(location);
    }
    protocol_adapter::diagnostic(diagnostic).into()
}

pub(crate) fn call_error(error: ScriptError) -> ScriptCallError {
    ScriptCallError::Diagnostic(Box::new(protocol_adapter::core_diagnostic(
        &error.into_host_error(),
    )))
}

//! Expression 片段与 Twee Source 之间的 Diagnostic 位置映射。

use crate::{
    diagnostic::{Diagnostic, DiagnosticLocation, DiagnosticLocator, DiagnosticSeverity},
    expression::{Expression, Span as ExpressionSpan, parse},
    source::SourcePath,
    twee,
};

use super::super::HirError;

/// 把宏参数 Expression 解析错误映射到 Twee Source 的公共 Diagnostic。
pub(super) fn map_macro_expression_error(
    passage: &twee::Passage<'_>,
    macro_node: &twee::MacroNode<'_>,
    error: crate::expression::ParseError,
) -> Diagnostic {
    map_macro_expression_fragment_error(passage, macro_node, 0, error)
}

/// 解析参数片段，保留片段相对宏参数的起点偏移用于错误定位。
pub(super) fn parse_for_expression<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
    source: &'source str,
    fragment_start: usize,
) -> Result<Expression<'source>, HirError> {
    parse(source).map_err(|error| HirError {
        diagnostic: map_macro_expression_fragment_error(passage, macro_node, fragment_start, error),
    })
}

/// 把带偏移的片段解析错误映射到 Twee Source 位置；映射失败回退到宏参数 Span。
pub(super) fn map_macro_expression_fragment_error(
    passage: &twee::Passage<'_>,
    macro_node: &twee::MacroNode<'_>,
    fragment_start: usize,
    error: crate::expression::ParseError,
) -> Diagnostic {
    let local_span: ExpressionSpan = error.span().unwrap_or(ExpressionSpan {
        start: 0,
        end: macro_node.arguments.len().saturating_sub(fragment_start),
    });
    let locator: DiagnosticLocator<'_> =
        DiagnosticLocator::new(passage.source.as_str(), passage.content);
    let absolute_fragment_start: usize = macro_node.arguments_span.start + fragment_start;

    match locator.locate(absolute_fragment_start, local_span.start, local_span.end) {
        Ok(location) => error.diagnostic().with_location(location),
        Err(_) => Diagnostic::new(
            "hir.invalid_source_span",
            DiagnosticSeverity::Error,
            "Macro 参数位置无法映射到 Twee Source",
        )
        .with_location(node_location(passage, macro_node.arguments_span)),
    }
}

/// 构造宏参数内片段在 Twee Source 中的 DiagnosticLocation。
pub(super) fn for_argument_location(
    passage: &twee::Passage<'_>,
    macro_node: &twee::MacroNode<'_>,
    start: usize,
    end: usize,
) -> DiagnosticLocation {
    node_location(passage, macro_argument_span(macro_node, start, end))
}

/// 把宏参数内的相对范围换算为 Twee Span。
pub(super) fn macro_argument_span(
    macro_node: &twee::MacroNode<'_>,
    start: usize,
    end: usize,
) -> twee::Span {
    twee::Span {
        start: macro_node.arguments_span.start + start,
        end: macro_node.arguments_span.start + end,
        line: macro_node.arguments_span.line,
        column: macro_node.arguments_span.column + macro_node.arguments[..start].chars().count(),
    }
}

/// 把 Passage 内的 Twee Span 转换为带 Source 的 DiagnosticLocation。
pub(super) fn node_location(passage: &twee::Passage<'_>, span: twee::Span) -> DiagnosticLocation {
    span_location(passage.source, span)
}

/// 把 Twee Span 与 Source 组合为 DiagnosticLocation。
pub(super) fn span_location(source: &SourcePath, span: twee::Span) -> DiagnosticLocation {
    DiagnosticLocation {
        source: source.as_str().to_owned(),
        start: span.start,
        end: span.end,
        line: span.line,
        column: span.column,
    }
}

//! Expression 片段与 Twee Source 之间的 Diagnostic 位置映射。

use crate::{
    diagnostic::{Diagnostic, DiagnosticLocation, DiagnosticLocator, DiagnosticSeverity},
    expression::{Expression, Span as ExpressionSpan, parse},
    source::SourcePath,
    twee,
};

use super::super::HirError;

pub(super) fn map_macro_expression_error(
    passage: &twee::Passage<'_>,
    macro_node: &twee::MacroNode<'_>,
    error: crate::expression::ParseError,
) -> Diagnostic {
    map_macro_expression_fragment_error(passage, macro_node, 0, error)
}

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

pub(super) fn for_argument_location(
    passage: &twee::Passage<'_>,
    macro_node: &twee::MacroNode<'_>,
    start: usize,
    end: usize,
) -> DiagnosticLocation {
    node_location(passage, macro_argument_span(macro_node, start, end))
}

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

pub(super) fn node_location(passage: &twee::Passage<'_>, span: twee::Span) -> DiagnosticLocation {
    span_location(passage.source, span)
}

pub(super) fn span_location(source: &SourcePath, span: twee::Span) -> DiagnosticLocation {
    DiagnosticLocation {
        source: source.as_str().to_owned(),
        start: span.start,
        end: span.end,
        line: span.line,
        column: span.column,
    }
}

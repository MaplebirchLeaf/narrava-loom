//! `set`、`unset` 与单个动作 Expression 的 lowering。

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    expression::{AssignmentOperator, Expression, ExpressionKind, Span as ExpressionSpan, parse},
    twee,
};

use super::super::HirError;
use super::{
    for_argument_location, map_macro_expression_error, node_location, parse_for_expression,
    syntax::{find_top_level_assignment, find_top_level_keyword, trimmed_slice},
};

/// 校验 unset 目标可删除，返回目标 Expression。
pub(super) fn lower_unset<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
) -> Result<Expression<'source>, HirError> {
    let target: Expression<'source> = lower_action_expression(passage, macro_node)?;
    if !target.is_assignable_target() {
        return Err(HirError {
            diagnostic: Diagnostic::new(
                "hir.invalid_unset_target",
                DiagnosticSeverity::Error,
                "unset 目标必须是可删除的变量、成员或索引",
            )
            .with_location(node_location(passage, macro_node.arguments_span)),
        });
    }
    Ok(target)
}

/// 把动作类 Macro（run/include/goto/set 值）的参数解析为单个 Expression。
pub(super) fn lower_action_expression<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
) -> Result<Expression<'source>, HirError> {
    parse(macro_node.arguments).map_err(|error| HirError {
        diagnostic: map_macro_expression_error(passage, macro_node, error),
    })
}

/// 解析 `target = value` 或 `target to value` 参数并构造赋值 Expression。
pub(super) fn lower_set<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
) -> Result<Expression<'source>, HirError> {
    let arguments: &'source str = macro_node.arguments;
    let separator: Option<(usize, usize)> = find_top_level_assignment(arguments)
        .map(|index: usize| (index, index + 1))
        .or_else(|| {
            find_top_level_keyword(arguments, "to").map(|index: usize| (index, index + "to".len()))
        });
    let (separator_start, separator_end): (usize, usize) =
        separator.ok_or_else(|| invalid_set_arguments(passage, macro_node))?;
    let (target_source, target_start, target_end): (&str, usize, usize) =
        trimmed_slice(arguments, 0, separator_start)
            .ok_or_else(|| invalid_set_arguments(passage, macro_node))?;
    let (value_source, value_start, value_end): (&str, usize, usize) =
        trimmed_slice(arguments, separator_end, arguments.len())
            .ok_or_else(|| invalid_set_arguments(passage, macro_node))?;
    let target: Expression<'source> =
        parse_for_expression(passage, macro_node, target_source, target_start)?;
    if !target.is_assignable_target() {
        return Err(HirError {
            diagnostic: Diagnostic::new(
                "hir.invalid_set_target",
                DiagnosticSeverity::Error,
                "set 目标必须是可写位置",
            )
            .with_location(for_argument_location(
                passage,
                macro_node,
                target_start,
                target_end,
            )),
        });
    }
    let value: Expression<'source> =
        parse_for_expression(passage, macro_node, value_source, value_start)?;

    Ok(Expression {
        kind: ExpressionKind::Assignment {
            operator: AssignmentOperator::Assign,
            target: Box::new(target),
            value: Box::new(value),
        },
        span: ExpressionSpan {
            start: target_start,
            end: value_end,
        },
    })
}

fn invalid_set_arguments(
    passage: &twee::Passage<'_>,
    macro_node: &twee::MacroNode<'_>,
) -> HirError {
    HirError {
        diagnostic: Diagnostic::new(
            "hir.invalid_set_arguments",
            DiagnosticSeverity::Error,
            "set 参数应使用 `target = value` 或 `target to value`",
        )
        .with_location(node_location(passage, macro_node.arguments_span)),
    }
}

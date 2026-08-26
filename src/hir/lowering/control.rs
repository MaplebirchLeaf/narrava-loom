//! `if`、`switch` 与 `for` 的结构化 lowering。

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    expression::{Expression, ExpressionKind},
    twee::{self, BodyNodeKind},
};

use super::super::{
    HirBodyNode, HirError, HirFor, HirForKind, HirForTarget, HirIf, HirIfBranch, HirMacroArguments,
    HirSwitch, HirSwitchCase,
};
use super::{
    LoweringContext,
    assignment::lower_action_expression,
    lower_body_node, lower_body_nodes, lower_macro_arguments, lower_required_expression,
    source_map::{for_argument_location, macro_argument_span, node_location, parse_for_expression},
    syntax::{find_top_level_keyword, first_word, trim_start_index, trimmed_slice},
};

/// 把 switch 正文降低为按源码顺序的 case 列表与可选 default。
pub(super) fn lower_switch<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
    context: LoweringContext,
) -> Result<HirSwitch<'source>, HirError> {
    let value: Expression<'source> = lower_action_expression(passage, macro_node)?;
    let mut cases: Vec<HirSwitchCase<'source>> = Vec::new();
    let mut default: Option<Vec<HirBodyNode<'source>>> = None;

    for child in &macro_node.body {
        let BodyNodeKind::Macro(clause) = &child.kind else {
            return Err(invalid_switch_clause(
                passage,
                child,
                "首个 `case` 前不能有正文",
            ));
        };
        match clause.name {
            "case" if default.is_none() => cases.push(HirSwitchCase {
                value: lower_action_expression(passage, clause)?,
                body: lower_body_nodes(passage, &clause.body, context)?,
            }),
            "default" if default.is_none() && clause.arguments.is_empty() => {
                default = Some(lower_body_nodes(passage, &clause.body, context)?);
            }
            "default" if !clause.arguments.is_empty() => {
                return Err(HirError {
                    diagnostic: Diagnostic::new(
                        "hir.unexpected_macro_arguments",
                        DiagnosticSeverity::Error,
                        "Macro `default` 不接受参数",
                    )
                    .with_location(node_location(passage, clause.arguments_span)),
                });
            }
            "case" | "default" => {
                return Err(invalid_switch_clause(
                    passage,
                    child,
                    "`default` 之后不能再出现 `case` 或 `default`",
                ));
            }
            _ => {
                return Err(invalid_switch_clause(
                    passage,
                    child,
                    "switch 正文只能包含 `case` 或 `default` 子句",
                ));
            }
        }
    }

    Ok(HirSwitch {
        value,
        cases,
        default,
    })
}

/// 解析 for 目标与迭代模式，降低正文并进入循环上下文。
pub(super) fn lower_for<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
    context: LoweringContext,
) -> Result<HirFor<'source>, HirError> {
    let arguments: &'source str = macro_node.arguments;
    let (target_source, target_start, target_end): (&str, usize, usize) =
        first_word(arguments).ok_or_else(|| invalid_for_arguments(passage, macro_node))?;
    let (mode_source, _mode_start, mode_end): (&str, usize, usize) =
        first_word(&arguments[target_end..])
            .map(|(value, start, end)| (value, target_end + start, target_end + end))
            .ok_or_else(|| invalid_for_arguments(passage, macro_node))?;
    let target: Expression<'source> =
        parse_for_expression(passage, macro_node, target_source, target_start)?;
    if !matches!(
        target.kind,
        ExpressionKind::Global(_) | ExpressionKind::Variable { .. }
    ) {
        return Err(HirError {
            diagnostic: Diagnostic::new(
                "hir.invalid_for_target",
                DiagnosticSeverity::Error,
                "for 目标必须是可写变量",
            )
            .with_location(for_argument_location(
                passage,
                macro_node,
                target_start,
                target_end,
            )),
        });
    }

    let remainder_start: usize = trim_start_index(arguments, mode_end, arguments.len());
    let kind: HirForKind<'source> = match mode_source {
        "in" | "of" => {
            let (collection_source, collection_start, collection_end): (&str, usize, usize) =
                trimmed_slice(arguments, remainder_start, arguments.len())
                    .ok_or_else(|| invalid_for_arguments(passage, macro_node))?;
            let collection: Expression<'source> =
                parse_for_expression(passage, macro_node, collection_source, collection_start)?;
            let span: twee::Span =
                macro_argument_span(macro_node, collection_start, collection_end);
            if mode_source == "in" {
                HirForKind::In { collection, span }
            } else {
                HirForKind::Of { collection, span }
            }
        }
        "range" => lower_for_range(passage, macro_node, remainder_start)?,
        _ => return Err(invalid_for_arguments(passage, macro_node)),
    };

    Ok(HirFor {
        target: HirForTarget {
            value: target,
            span: macro_argument_span(macro_node, target_start, target_end),
        },
        kind,
        body: lower_body_nodes(passage, &macro_node.body, context.enter_loop())?,
    })
}

/// 把 if/elseif/else 子句归组为条件分支与 fallback。
pub(super) fn lower_if<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
    context: LoweringContext,
) -> Result<HirIf<'source>, HirError> {
    let condition: Expression<'source> = lower_required_expression(passage, macro_node)?;
    let mut main_body: Vec<HirBodyNode<'source>> = Vec::new();
    let mut branches: Vec<HirIfBranch<'source>> = Vec::new();
    let mut fallback: Option<Vec<HirBodyNode<'source>>> = None;

    for child in &macro_node.body {
        let BodyNodeKind::Macro(clause) = &child.kind else {
            main_body.push(lower_body_node(passage, child, context)?);
            continue;
        };
        match clause.name {
            "elseif" if fallback.is_none() => branches.push(HirIfBranch {
                condition: lower_required_expression(passage, clause)?,
                body: lower_body_nodes(passage, &clause.body, context)?,
            }),
            "else" if fallback.is_none() => {
                let _: HirMacroArguments<'source> = lower_macro_arguments(passage, clause)?;
                fallback = Some(lower_body_nodes(passage, &clause.body, context)?);
            }
            "elseif" | "else" => {
                return Err(HirError {
                    diagnostic: Diagnostic::new(
                        "hir.invalid_if_clause_order",
                        DiagnosticSeverity::Error,
                        "`else` 之后不能再出现 `elseif` 或 `else`",
                    )
                    .with_location(node_location(passage, child.span)),
                });
            }
            _ => main_body.push(lower_body_node(passage, child, context)?),
        }
    }

    branches.insert(
        0,
        HirIfBranch {
            condition,
            body: main_body,
        },
    );
    Ok(HirIf { branches, fallback })
}

/// 解析 `range start to end [step n]` 的边界、步长及其源码位置。
fn lower_for_range<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
    range_start: usize,
) -> Result<HirForKind<'source>, HirError> {
    let arguments: &'source str = macro_node.arguments;
    let range_source: &str = &arguments[range_start..];
    let to_relative: usize = find_top_level_keyword(range_source, "to")
        .ok_or_else(|| invalid_for_arguments(passage, macro_node))?;
    let to_start: usize = range_start + to_relative;
    let to_end: usize = to_start + "to".len();
    let (start_source, start_start, start_end): (&str, usize, usize) =
        trimmed_slice(arguments, range_start, to_start)
            .ok_or_else(|| invalid_for_arguments(passage, macro_node))?;
    let after_to: usize = trim_start_index(arguments, to_end, arguments.len());
    let tail: &str = &arguments[after_to..];
    let step_relative: Option<usize> = find_top_level_keyword(tail, "step");
    let end_limit: usize = step_relative.map_or(arguments.len(), |index| after_to + index);
    let (end_source, end_start, end_end): (&str, usize, usize) =
        trimmed_slice(arguments, after_to, end_limit)
            .ok_or_else(|| invalid_for_arguments(passage, macro_node))?;
    let start: Expression<'source> =
        parse_for_expression(passage, macro_node, start_source, start_start)?;
    let end: Expression<'source> =
        parse_for_expression(passage, macro_node, end_source, end_start)?;
    let (step, step_span): (Option<Expression<'source>>, Option<twee::Span>) =
        if let Some(step_relative) = step_relative {
            let step_keyword_end: usize = after_to + step_relative + "step".len();
            let (step_source, step_start, step_end): (&str, usize, usize) =
                trimmed_slice(arguments, step_keyword_end, arguments.len())
                    .ok_or_else(|| invalid_for_arguments(passage, macro_node))?;
            (
                Some(parse_for_expression(
                    passage,
                    macro_node,
                    step_source,
                    step_start,
                )?),
                Some(macro_argument_span(macro_node, step_start, step_end)),
            )
        } else {
            (None, None)
        };

    Ok(HirForKind::Range {
        start,
        start_span: macro_argument_span(macro_node, start_start, start_end),
        end,
        end_span: macro_argument_span(macro_node, end_start, end_end),
        step,
        step_span,
    })
}

fn invalid_switch_clause(
    passage: &twee::Passage<'_>,
    node: &twee::BodyNode<'_>,
    message: &str,
) -> HirError {
    HirError {
        diagnostic: Diagnostic::new(
            "hir.invalid_switch_clause",
            DiagnosticSeverity::Error,
            message,
        )
        .with_location(node_location(passage, node.span)),
    }
}

fn invalid_for_arguments(
    passage: &twee::Passage<'_>,
    macro_node: &twee::MacroNode<'_>,
) -> HirError {
    HirError {
        diagnostic: Diagnostic::new(
            "hir.invalid_for_arguments",
            DiagnosticSeverity::Error,
            "for 参数应使用 `target in expression`、`target of expression` 或 `target range start to end [step amount]`",
        )
        .with_location(node_location(passage, macro_node.arguments_span)),
    }
}

//! Widget Definition lowering 与 Story 级结构校验。

use std::collections::HashSet;

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    twee,
};

use super::super::{
    HirBodyKind, HirBodyNode, HirError, HirIfBranch, HirPassage, HirSwitchCase, HirWidget,
};
use super::{
    LoweringContext, lower_body_nodes, macro_argument_error, node_location, span_location,
    syntax::first_word,
};

/// `[widget]` Passage 是纯定义容器，不在启动时执行其他逻辑。
pub(super) fn validate_widget_passage_content(passages: &[HirPassage<'_>]) -> Result<(), HirError> {
    for passage in passages
        .iter()
        .filter(|passage: &&HirPassage<'_>| passage.has_tag("widget"))
    {
        if let Some(node) = passage
            .body
            .iter()
            .find(|node: &&HirBodyNode<'_>| match node.kind {
                HirBodyKind::Widget(_) => false,
                HirBodyKind::Text(text) => !text.trim().is_empty(),
                _ => true,
            })
        {
            return Err(HirError {
                diagnostic: Diagnostic::new(
                    "hir.invalid_widget_content",
                    DiagnosticSeverity::Error,
                    "`[widget]` Passage 只能包含顶层 Widget Definition",
                )
                .with_location(span_location(passage.source, node.span)),
            });
        }
    }

    Ok(())
}

/// 确保 Widget Definition 只出现在 `[widget]` Passage 顶层。
pub(super) fn validate_top_level_widgets(passages: &[HirPassage<'_>]) -> Result<(), HirError> {
    for passage in passages {
        for node in &passage.body {
            let nested: Option<&HirBodyNode<'_>> = match &node.kind {
                HirBodyKind::Widget(widget) => find_widget(&widget.body),
                _ => find_nested_widget(node),
            };
            if let Some(widget) = nested {
                return Err(HirError {
                    diagnostic: Diagnostic::new(
                        "hir.nested_widget",
                        DiagnosticSeverity::Error,
                        "Widget Definition 只能位于 `[widget]` Passage 顶层",
                    )
                    .with_location(span_location(passage.source, widget.span)),
                });
            }
        }
    }

    Ok(())
}

/// Twee 静态 Widget 名称全局唯一；scripts 可通过 Macro API 主动替换定义。
pub(super) fn validate_unique_widgets(passages: &[HirPassage<'_>]) -> Result<(), HirError> {
    let mut names: HashSet<&str> = HashSet::new();

    for passage in passages {
        for node in &passage.body {
            let HirBodyKind::Widget(widget) = &node.kind else {
                continue;
            };
            if !names.insert(widget.name) {
                return Err(HirError {
                    diagnostic: Diagnostic::new(
                        "hir.duplicate_widget",
                        DiagnosticSeverity::Error,
                        &format!("Twee Widget `{}` 已经定义", widget.name),
                    )
                    .with_location(span_location(passage.source, node.span)),
                });
            }
        }
    }

    Ok(())
}

/// Widget 的声明正文是 Handler；调用处始终是 Inline Macro，不能再携带调用正文。
pub(super) fn validate_inline_widget_calls(passages: &[HirPassage<'_>]) -> Result<(), HirError> {
    let names: HashSet<&str> = passages
        .iter()
        .flat_map(|passage: &HirPassage<'_>| passage.body.iter())
        .filter_map(|node: &HirBodyNode<'_>| match &node.kind {
            HirBodyKind::Widget(widget) => Some(widget.name),
            _ => None,
        })
        .collect();

    for passage in passages {
        if let Some(node) = find_container_widget_call(&passage.body, &names) {
            let HirBodyKind::Macro(call) = &node.kind else {
                unreachable!("Widget 调用检查只返回通用 Macro")
            };
            return Err(HirError {
                diagnostic: Diagnostic::new(
                    "hir.widget_call_must_be_inline",
                    DiagnosticSeverity::Error,
                    &format!(
                        "Widget `{}` 的调用必须写成 `<<{} ...>>`，不能使用调用侧闭合标签",
                        call.name, call.name
                    ),
                )
                .with_location(span_location(passage.source, node.span)),
            });
        }
    }
    Ok(())
}

fn find_container_widget_call<'node, 'source>(
    nodes: &'node [HirBodyNode<'source>],
    names: &HashSet<&str>,
) -> Option<&'node HirBodyNode<'source>> {
    nodes
        .iter()
        .find_map(|node: &HirBodyNode<'source>| match &node.kind {
            HirBodyKind::Macro(call)
                if names.contains(call.name)
                    && call.syntax_kind == twee::MacroSyntaxKind::Container =>
            {
                Some(node)
            }
            _ => find_nested_widget_call(node, names),
        })
}

fn find_nested_widget_call<'node, 'source>(
    node: &'node HirBodyNode<'source>,
    names: &HashSet<&str>,
) -> Option<&'node HirBodyNode<'source>> {
    match &node.kind {
        HirBodyKind::If(conditional) => conditional
            .branches
            .iter()
            .find_map(|branch: &HirIfBranch<'source>| {
                find_container_widget_call(&branch.body, names)
            })
            .or_else(|| {
                conditional
                    .fallback
                    .as_deref()
                    .and_then(|body| find_container_widget_call(body, names))
            }),
        HirBodyKind::Switch(switch) => switch
            .cases
            .iter()
            .find_map(|case: &HirSwitchCase<'source>| find_container_widget_call(&case.body, names))
            .or_else(|| {
                switch
                    .default
                    .as_deref()
                    .and_then(|body| find_container_widget_call(body, names))
            }),
        HirBodyKind::For(loop_node) => find_container_widget_call(&loop_node.body, names),
        HirBodyKind::While(loop_node) => find_container_widget_call(&loop_node.body, names),
        HirBodyKind::Silently(body) => find_container_widget_call(body, names),
        HirBodyKind::Widget(widget) => find_container_widget_call(&widget.body, names),
        HirBodyKind::Capture(capture) => find_container_widget_call(&capture.body, names),
        HirBodyKind::Macro(call) => find_container_widget_call(&call.body, names),
        _ => None,
    }
}

/// 校验 Widget 名称与声明参数，并降低声明正文（进入独立调用边界）。
pub(super) fn lower_widget<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
    context: LoweringContext,
) -> Result<HirWidget<'source>, HirError> {
    let Some((name, name_start, name_end, declaration_end)) = widget_name(macro_node.arguments)
    else {
        return Err(HirError {
            diagnostic: Diagnostic::new(
                "hir.invalid_widget_name",
                DiagnosticSeverity::Error,
                "Widget 必须声明有效名称",
            )
            .with_location(node_location(passage, macro_node.arguments_span)),
        });
    };
    if !is_widget_name(name) {
        return Err(macro_argument_error(
            passage,
            macro_node,
            name_start,
            name_end,
            "hir.invalid_widget_name",
            "Widget 名称必须是有效标识符",
        ));
    }

    if let Some((_, relative_start, relative_end)) =
        first_word(&macro_node.arguments[declaration_end..])
    {
        let start: usize = declaration_end + relative_start;
        let end: usize = declaration_end + relative_end;
        return Err(macro_argument_error(
            passage,
            macro_node,
            start,
            end,
            "hir.unexpected_widget_arguments",
            "Widget 只声明名称；调用实参通过 `@args` 读取",
        ));
    }

    Ok(HirWidget {
        name,
        body: lower_body_nodes(passage, &macro_node.body, context.enter_widget())?,
    })
}

/// Widget 名是标识符；作者可使用引号让“定义名”在编辑器中保持字符串外观。
/// 返回名称、名称范围和整个声明的结束偏移。
fn widget_name(arguments: &str) -> Option<(&str, usize, usize, usize)> {
    let leading = arguments.len() - arguments.trim_start().len();
    let rest = &arguments[leading..];
    let quote = rest.as_bytes().first().copied();
    if matches!(quote, Some(b'\'') | Some(b'"')) {
        let quote = quote.expect("已确认引号");
        let relative_end = rest[1..]
            .bytes()
            .position(|byte| byte == quote)
            .map(|index| index + 1)?;
        let start = leading + 1;
        let end = leading + relative_end;
        return Some((&arguments[start..end], start, end, end + 1));
    }
    first_word(arguments).map(|(name, start, end)| (name, start, end, end))
}

fn find_widget<'node, 'source>(
    nodes: &'node [HirBodyNode<'source>],
) -> Option<&'node HirBodyNode<'source>> {
    nodes.iter().find_map(|node: &HirBodyNode<'source>| {
        if matches!(node.kind, HirBodyKind::Widget(_)) {
            Some(node)
        } else {
            find_nested_widget(node)
        }
    })
}

fn find_nested_widget<'node, 'source>(
    node: &'node HirBodyNode<'source>,
) -> Option<&'node HirBodyNode<'source>> {
    match &node.kind {
        HirBodyKind::If(conditional) => conditional
            .branches
            .iter()
            .find_map(|branch: &HirIfBranch<'source>| find_widget(&branch.body))
            .or_else(|| conditional.fallback.as_deref().and_then(find_widget)),
        HirBodyKind::Switch(switch) => switch
            .cases
            .iter()
            .find_map(|case: &HirSwitchCase<'source>| find_widget(&case.body))
            .or_else(|| switch.default.as_deref().and_then(find_widget)),
        HirBodyKind::For(loop_node) => find_widget(&loop_node.body),
        HirBodyKind::While(loop_node) => find_widget(&loop_node.body),
        HirBodyKind::Silently(body) => find_widget(body),
        HirBodyKind::Widget(widget) => find_widget(&widget.body),
        HirBodyKind::Capture(capture) => find_widget(&capture.body),
        HirBodyKind::Macro(macro_node) => find_widget(&macro_node.body),
        _ => None,
    }
}

/// 判断名称是否为合法标识符：字母/下划线开头，其余为字母数字下划线。
fn is_widget_name(name: &str) -> bool {
    let mut bytes: std::slice::Iter<'_, u8> = name.as_bytes().iter();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || *first == b'_')
        && bytes.all(|byte: &u8| byte.is_ascii_alphanumeric() || *byte == b'_')
}

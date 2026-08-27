//! Twee AST → HIR 的 lowering 与校验。
//!
//! 只负责把已通过 Twee 编译边界的 Passage 转换到 HIR，并执行 Widget 等
//! 结构化约束校验；不涉及 State、Macro Definitions 或 Surface。

mod assignment;
mod control;
mod fragment;
mod source_map;
mod syntax;
mod widget;

pub use fragment::lower_fragment;

use assignment::{lower_action_expression, lower_set, lower_unset};
use control::{lower_for, lower_if, lower_switch};
use fragment::{PrintLoweringError, lower_print_argument, print_argument_required_diagnostic};
use source_map::{
    for_argument_location, macro_argument_span, map_macro_expression_error,
    map_macro_expression_fragment_error, node_location, parse_for_expression, span_location,
};
use syntax::first_word;
use widget::{
    lower_widget, validate_inline_widget_calls, validate_top_level_widgets,
    validate_unique_widgets, validate_widget_passage_content,
};

use crate::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::expression::{Expression, ExpressionKind, VariableScope, parse as parse_expression};
use crate::twee::{self, BodyNodeKind};

use super::{
    HirBodyKind, HirBodyNode, HirCapture, HirError, HirMacro, HirMacroArguments, HirPassage,
    HirPrint, HirStory, HirWhile, HirWidget,
};

/// lowering 过程状态：循环嵌套深度用于校验 break/continue 的合法位置。
#[derive(Clone, Copy, Debug, Default)]
struct LoweringContext {
    loop_depth: usize,
}

impl LoweringContext {
    fn enter_loop(self) -> Self {
        Self {
            loop_depth: self.loop_depth + 1,
        }
    }

    fn enter_widget(self) -> Self {
        Self {
            // Widget 正文是独立调用边界，不能跳出声明位置的循环。
            loop_depth: 0,
        }
    }
}

impl<'source> HirStory<'source> {
    /// 把通过 Twee 语义检查的 Story 降低为 HIR，并执行 Widget 结构校验。
    pub fn lower(story: &twee::Story<'source>) -> Result<Self, HirError> {
        let passages: Result<Vec<HirPassage<'source>>, HirError> =
            story.passages.iter().map(lower_passage).collect();
        let passages: Vec<HirPassage<'source>> = passages?;
        validate_top_level_widgets(&passages)?;
        validate_widget_passage_content(&passages)?;
        validate_unique_widgets(&passages)?;
        validate_inline_widget_calls(&passages)?;

        Ok(Self { passages })
    }
}

/// 降低单个 Twee Passage 的正文并保留源码身份。
fn lower_passage<'source>(
    passage: &twee::Passage<'source>,
) -> Result<HirPassage<'source>, HirError> {
    let body: Vec<HirBodyNode<'source>> =
        lower_body_nodes(passage, &passage.body, LoweringContext::default())?;

    Ok(HirPassage {
        source: passage.source,
        name: passage.name,
        tags: passage.tags.clone(),
        body,
    })
}

/// 按 Macro 名称分派，把单个 Twee 正文节点降低为 HIR 节点。
fn lower_body_node<'source>(
    passage: &twee::Passage<'source>,
    node: &twee::BodyNode<'source>,
    context: LoweringContext,
) -> Result<HirBodyNode<'source>, HirError> {
    let kind: HirBodyKind<'source> = match &node.kind {
        BodyNodeKind::Text(text) => HirBodyKind::Text(text),
        BodyNodeKind::Macro(macro_node) if macro_node.name == "if" => {
            HirBodyKind::If(lower_if(passage, macro_node, context)?)
        }
        BodyNodeKind::Macro(macro_node) if macro_node.name == "switch" => {
            HirBodyKind::Switch(Box::new(lower_switch(passage, macro_node, context)?))
        }
        BodyNodeKind::Macro(macro_node) if macro_node.name == "for" => {
            HirBodyKind::For(Box::new(lower_for(passage, macro_node, context)?))
        }
        BodyNodeKind::Macro(macro_node) if macro_node.name == "while" => {
            HirBodyKind::While(Box::new(HirWhile {
                condition: lower_required_expression(passage, macro_node)?,
                body: lower_body_nodes(passage, &macro_node.body, context.enter_loop())?,
            }))
        }
        BodyNodeKind::Macro(macro_node) if macro_node.name == "widget" => {
            let widget: HirWidget<'source> = lower_widget(passage, macro_node, context)?;
            if !passage.tags.contains(&"widget") {
                return Err(HirError {
                    diagnostic: Diagnostic::new(
                        "hir.widget_tag_required",
                        DiagnosticSeverity::Error,
                        "定义 widget 的 Passage 必须带有 `[widget]` Tag",
                    )
                    .with_location(node_location(passage, node.span)),
                });
            }
            HirBodyKind::Widget(widget)
        }
        BodyNodeKind::Macro(macro_node) if macro_node.name == "return" => {
            let value: Option<Box<Expression<'source>>> = if macro_node.arguments.is_empty() {
                None
            } else {
                Some(Box::new(lower_action_expression(passage, macro_node)?))
            };
            HirBodyKind::Return(value)
        }
        BodyNodeKind::Macro(macro_node) if macro_node.name == "capture" => {
            HirBodyKind::Capture(lower_capture(passage, macro_node, context)?)
        }
        BodyNodeKind::Macro(macro_node) if macro_node.name == "exit" => {
            if !macro_node.arguments.is_empty() {
                return Err(HirError {
                    diagnostic: Diagnostic::new(
                        "hir.unexpected_macro_arguments",
                        DiagnosticSeverity::Error,
                        "Macro `exit` 不接受参数",
                    )
                    .with_location(node_location(passage, macro_node.arguments_span)),
                });
            }
            HirBodyKind::Exit
        }
        BodyNodeKind::Macro(macro_node) if macro_node.name == "set" => {
            HirBodyKind::Set(Box::new(lower_set(passage, macro_node)?))
        }
        BodyNodeKind::Macro(macro_node) if macro_node.name == "unset" => {
            HirBodyKind::Unset(Box::new(lower_unset(passage, macro_node)?))
        }
        BodyNodeKind::Macro(macro_node) if macro_node.name == "print" => {
            if has_print_options(macro_node.arguments) {
                // 带 color/styles/delay/heading 选项时走动态宏路径（与脚本/Widget 宏同链路），
                // 由 Host 求值参数并产生 StyledText；单参数仍是编译器固有 Print（纯 Text）。
                HirBodyKind::Macro(HirMacro {
                    name: macro_node.name,
                    arguments: lower_macro_arguments(passage, macro_node)?,
                    syntax_kind: macro_node.syntax_kind,
                    body: Vec::new(),
                })
            } else {
                HirBodyKind::Print(lower_print(passage, macro_node)?)
            }
        }
        BodyNodeKind::Macro(macro_node) if macro_node.name == "silently" => {
            if !macro_node.arguments.is_empty() {
                return Err(HirError {
                    diagnostic: Diagnostic::new(
                        "hir.unexpected_macro_arguments",
                        DiagnosticSeverity::Error,
                        "Macro `silently` 不接受参数",
                    )
                    .with_location(node_location(passage, macro_node.arguments_span)),
                });
            }
            HirBodyKind::Silently(lower_body_nodes(passage, &macro_node.body, context)?)
        }
        BodyNodeKind::Macro(macro_node)
            if matches!(macro_node.name, "run" | "include" | "goto") =>
        {
            let expression: Box<Expression<'source>> =
                Box::new(lower_action_expression(passage, macro_node)?);
            match macro_node.name {
                "run" => HirBodyKind::Run(expression),
                "include" => HirBodyKind::Include(expression),
                "goto" => HirBodyKind::Goto(expression),
                _ => unreachable!("匹配守卫已经限制动作名称"),
            }
        }
        BodyNodeKind::Macro(macro_node) if matches!(macro_node.name, "break" | "continue") => {
            if !macro_node.arguments.is_empty() {
                return Err(HirError {
                    diagnostic: Diagnostic::new(
                        "hir.unexpected_macro_arguments",
                        DiagnosticSeverity::Error,
                        &format!("Macro `{}` 不接受参数", macro_node.name),
                    )
                    .with_location(node_location(passage, macro_node.arguments_span)),
                });
            }
            if context.loop_depth == 0 {
                return Err(HirError {
                    diagnostic: Diagnostic::new(
                        "hir.loop_control_outside_loop",
                        DiagnosticSeverity::Error,
                        &format!("`{}` 只能出现在 `for` 或 `while` 内部", macro_node.name),
                    )
                    .with_location(node_location(passage, node.span)),
                });
            }
            if macro_node.name == "break" {
                HirBodyKind::Break
            } else {
                HirBodyKind::Continue
            }
        }
        BodyNodeKind::Macro(macro_node)
            if matches!(macro_node.name, "elseif" | "else" | "case" | "default") =>
        {
            return Err(HirError {
                diagnostic: Diagnostic::new(
                    "hir.orphan_clause",
                    DiagnosticSeverity::Error,
                    &format!("`{}` 不能独立出现在此处", macro_node.name),
                )
                .with_location(node_location(passage, node.span)),
            });
        }
        BodyNodeKind::Macro(macro_node) => HirBodyKind::Macro(HirMacro {
            name: macro_node.name,
            arguments: lower_macro_arguments(passage, macro_node)?,
            syntax_kind: macro_node.syntax_kind,
            body: lower_body_nodes(passage, &macro_node.body, context)?,
        }),
    };

    Ok(HirBodyNode {
        kind,
        span: node.span,
    })
}

/// `print` 的参数是否超过一个（出现 color/styles/delay/heading 选项）。
/// 顶层空白分割参数，忽略引号内与括号/中括号/大括号内部的空白。
pub(super) fn has_print_options(arguments: &str) -> bool {
    split_print_arguments(arguments).len() > 1
}

fn split_print_arguments(source: &str) -> Vec<&str> {
    let mut arguments: Vec<&str> = Vec::new();
    let mut depth: i32 = 0;
    let mut quote: Option<char> = None;
    let mut escaped: bool = false;
    let mut start: usize = 0;
    for (offset, ch) in source.char_indices() {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if ch.is_whitespace() && depth == 0 => {
                if offset > start {
                    arguments.push(source[start..offset].trim());
                }
                start = offset + ch.len_utf8();
            }
            _ => {}
        }
    }
    if start < source.len() {
        let tail: &str = source[start..].trim();
        if !tail.is_empty() {
            arguments.push(tail);
        }
    }
    arguments
}

fn lower_print<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
) -> Result<HirPrint<'source>, HirError> {
    lower_print_argument(macro_node.arguments).map_err(|error| HirError {
        diagnostic: match error {
            PrintLoweringError::Required => print_argument_required_diagnostic()
                .with_location(node_location(passage, macro_node.arguments_span)),
            PrintLoweringError::Expression { offset, error } => {
                map_macro_expression_fragment_error(passage, macro_node, offset, error)
            }
        },
    })
}

fn lower_capture<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
    context: LoweringContext,
) -> Result<HirCapture<'source>, HirError> {
    let mut locals: Vec<&'source str> = Vec::new();
    let mut cursor: usize = 0;

    while let Some((word, relative_start, relative_end)) =
        first_word(&macro_node.arguments[cursor..])
    {
        let start: usize = cursor + relative_start;
        let end: usize = cursor + relative_end;
        let expression: Expression<'source> =
            parse_for_expression(passage, macro_node, word, start)?;
        let ExpressionKind::Variable {
            scope: VariableScope::Local,
            name,
        } = expression.kind
        else {
            return Err(macro_argument_error(
                passage,
                macro_node,
                start,
                end,
                "hir.invalid_capture_variable",
                "capture 参数必须是 `@` 局部变量",
            ));
        };
        if locals.contains(&name) {
            return Err(macro_argument_error(
                passage,
                macro_node,
                start,
                end,
                "hir.duplicate_capture_variable",
                &format!("capture 重复列出了 `@{name}`"),
            ));
        }
        locals.push(name);
        cursor = end;
    }

    if locals.is_empty() {
        return Err(HirError {
            diagnostic: Diagnostic::new(
                "hir.invalid_capture_arguments",
                DiagnosticSeverity::Error,
                "capture 至少需要一个 `@` 局部变量",
            )
            .with_location(node_location(passage, macro_node.arguments_span)),
        });
    }

    Ok(HirCapture {
        locals,
        body: lower_body_nodes(passage, &macro_node.body, context)?,
    })
}

fn macro_argument_error(
    passage: &twee::Passage<'_>,
    macro_node: &twee::MacroNode<'_>,
    start: usize,
    end: usize,
    code: &'static str,
    message: &str,
) -> HirError {
    HirError {
        diagnostic: Diagnostic::new(code, DiagnosticSeverity::Error, message).with_location(
            node_location(passage, macro_argument_span(macro_node, start, end)),
        ),
    }
}

fn lower_body_nodes<'source>(
    passage: &twee::Passage<'source>,
    nodes: &[twee::BodyNode<'source>],
    context: LoweringContext,
) -> Result<Vec<HirBodyNode<'source>>, HirError> {
    let mut lowered: Vec<HirBodyNode<'source>> = Vec::new();
    for node in nodes {
        if let BodyNodeKind::Text(text) = node.kind {
            lowered.extend(lower_text_nodes(text, node.span));
        } else {
            lowered.push(lower_body_node(passage, node, context)?);
        }
    }
    Ok(lowered)
}

/// `<br>` 是 Narrava 正文结构，进入 HIR 后不再作为可见字符串存在。
fn lower_text_nodes<'source>(text: &'source str, span: twee::Span) -> Vec<HirBodyNode<'source>> {
    let mut nodes: Vec<HirBodyNode<'source>> = Vec::new();
    let mut start: usize = 0;
    while let Some(relative) = text[start..].find("<br>") {
        let end: usize = start + relative;
        if end > start {
            nodes.push(HirBodyNode {
                kind: HirBodyKind::Text(&text[start..end]),
                span: twee::Span {
                    start: span.start + start,
                    end: span.start + end,
                    ..span
                },
            });
        }
        nodes.push(HirBodyNode {
            kind: HirBodyKind::HardBreak,
            span: twee::Span {
                start: span.start + end,
                end: span.start + end + 4,
                ..span
            },
        });
        start = end + 4;
    }
    if start < text.len() {
        nodes.push(HirBodyNode {
            kind: HirBodyKind::Text(&text[start..]),
            span: twee::Span {
                start: span.start + start,
                end: span.end,
                ..span
            },
        });
    }
    nodes
}

fn lower_required_expression<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
) -> Result<Expression<'source>, HirError> {
    match lower_macro_arguments(passage, macro_node)? {
        HirMacroArguments::Expression(expression) => Ok(expression),
        HirMacroArguments::None | HirMacroArguments::Raw(_) => {
            unreachable!("调用方只会为已声明 Expression 参数的 Macro 请求条件")
        }
    }
}

fn lower_macro_arguments<'source>(
    passage: &twee::Passage<'source>,
    macro_node: &twee::MacroNode<'source>,
) -> Result<HirMacroArguments<'source>, HirError> {
    match macro_node.name {
        "if" | "elseif" | "while" | "run" => {
            let expression: Expression<'source> =
                parse_expression(macro_node.arguments).map_err(|error| HirError {
                    diagnostic: map_macro_expression_error(passage, macro_node, error),
                })?;
            Ok(HirMacroArguments::Expression(expression))
        }
        "else" if macro_node.arguments.is_empty() => Ok(HirMacroArguments::None),
        "else" => Err(HirError {
            diagnostic: Diagnostic::new(
                "hir.unexpected_macro_arguments",
                DiagnosticSeverity::Error,
                "Macro `else` 不接受参数",
            )
            .with_location(node_location(passage, macro_node.arguments_span)),
        }),
        _ => Ok(HirMacroArguments::Raw(macro_node.arguments)),
    }
}

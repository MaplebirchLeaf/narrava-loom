//! HIR 可见正文到稳定翻译消息目录的提取。
//!
//! 提取器把相邻静态文本和动态表达式组成一条译者消息，并以 HIR 结构路径生成
//! placeholder 身份。模板、校验和运行时解析只消费提取结果，不重复遍历 HIR。

use super::*;

/// 递归收集一段正文中的可见文本，把相邻静态文本与动态表达式合并为一条消息。
pub(super) fn collect_body(
    source: &str,
    passage: &str,
    path: &str,
    body: &[HirBodyNode<'_>],
    messages: &mut Vec<I18nMessage>,
) {
    let mut visible: Option<VisibleMessage> = None;

    for (index, node) in body.iter().enumerate() {
        let node_path: String = format!("{path}.{index}");
        match &node.kind {
            HirBodyKind::Text(text) => visible
                .get_or_insert_with(|| VisibleMessage::new(index, node.span))
                .push_text(text, node.span),
            HirBodyKind::Print(HirPrint::Literal(text)) => visible
                .get_or_insert_with(|| VisibleMessage::new(index, node.span))
                .push_text(text, node.span),
            HirBodyKind::Print(HirPrint::Expression(expression)) => visible
                .get_or_insert_with(|| VisibleMessage::new(index, node.span))
                .push_expression(expression, node_path, node.span),
            // silently 的正文不会进入 Presentation，因此不属于翻译目录。
            HirBodyKind::Silently(_) => {
                flush_visible(source, passage, path, &mut visible, messages)
            }
            HirBodyKind::If(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                for (branch_index, branch) in value.branches.iter().enumerate() {
                    collect_body(
                        source,
                        passage,
                        &format!("{node_path}.branch.{branch_index}"),
                        &branch.body,
                        messages,
                    );
                }
                if let Some(fallback) = &value.fallback {
                    collect_body(
                        source,
                        passage,
                        &format!("{node_path}.fallback"),
                        fallback,
                        messages,
                    );
                }
            }
            HirBodyKind::Switch(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                for (case_index, case) in value.cases.iter().enumerate() {
                    collect_body(
                        source,
                        passage,
                        &format!("{node_path}.case.{case_index}"),
                        &case.body,
                        messages,
                    );
                }
                if let Some(default) = &value.default {
                    collect_body(
                        source,
                        passage,
                        &format!("{node_path}.default"),
                        default,
                        messages,
                    );
                }
            }
            HirBodyKind::For(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                collect_body(
                    source,
                    passage,
                    &format!("{node_path}.body"),
                    &value.body,
                    messages,
                );
            }
            HirBodyKind::While(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                collect_body(
                    source,
                    passage,
                    &format!("{node_path}.body"),
                    &value.body,
                    messages,
                );
            }
            HirBodyKind::Widget(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                collect_body(
                    source,
                    passage,
                    &format!("{node_path}.body"),
                    &value.body,
                    messages,
                );
            }
            HirBodyKind::Capture(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                collect_body(
                    source,
                    passage,
                    &format!("{node_path}.body"),
                    &value.body,
                    messages,
                );
            }
            HirBodyKind::Macro(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                collect_body(
                    source,
                    passage,
                    &format!("{node_path}.body"),
                    &value.body,
                    messages,
                );
            }
            HirBodyKind::Break
            | HirBodyKind::Continue
            | HirBodyKind::Exit
            | HirBodyKind::Set(_)
            | HirBodyKind::Unset(_)
            | HirBodyKind::Run(_)
            | HirBodyKind::Include(_)
            | HirBodyKind::Goto(_)
            | HirBodyKind::Return(_) => {
                flush_visible(source, passage, path, &mut visible, messages);
            }
        }
    }

    flush_visible(source, passage, path, &mut visible, messages);
}

/// 正在累积的一条可见消息：静态文本与按出现顺序排列的 placeholder。
struct VisibleMessage {
    start_index: usize,
    text: String,
    placeholders: Vec<I18nPlaceholder>,
    span: Span,
    has_static_text: bool,
}

impl VisibleMessage {
    /// 以节点序号开始累积一条消息。
    fn new(start_index: usize, span: Span) -> Self {
        Self {
            start_index,
            text: String::new(),
            placeholders: Vec::new(),
            span,
            has_static_text: false,
        }
    }

    /// 追加静态文本；源码花括号按模板语法转义为双花括号。
    fn push_text(&mut self, text: &str, span: Span) {
        // 模板使用单花括号标记 placeholder，源码花括号必须先转义。
        for character in text.chars() {
            match character {
                '{' => self.text.push_str("{{"),
                '}' => self.text.push_str("}}"),
                _ => self.text.push(character),
            }
        }
        self.span.end = span.end;
        self.has_static_text |= !text.trim().is_empty();
    }

    /// 追加一个动态表达式占位符，并记录其 HIR 来源路径。
    fn push_expression(&mut self, expression: &Expression<'_>, node_path: String, span: Span) {
        let ordinal: usize = self.placeholders.len() + 1;
        let name: String = placeholder_name(expression, ordinal);
        self.text.push('{');
        self.text.push_str(&name);
        self.text.push('}');
        self.placeholders.push(I18nPlaceholder { name, node_path });
        self.span.end = span.end;
    }
}

/// 结束当前消息并写入目录；只有动态值、没有可翻译文字时不生成条目。
fn flush_visible(
    source: &str,
    passage: &str,
    path: &str,
    visible: &mut Option<VisibleMessage>,
    messages: &mut Vec<I18nMessage>,
) {
    let Some(visible) = visible.take() else {
        return;
    };
    // 只有动态值而没有可翻译文字时，不生成空洞的翻译条目。
    if !visible.has_static_text {
        return;
    }
    let node_path: String = format!("{path}.{}", visible.start_index);
    messages.push(I18nMessage {
        id: I18nTextId(format!("p{}:{passage}:{node_path}", passage.len())),
        source: source.to_owned(),
        passage: passage.to_owned(),
        text: visible.text,
        placeholders: visible.placeholders,
        span: visible.span,
    });
}

/// 为占位符取名：优先使用可静态确定的读取链，否则回退到序号。
fn placeholder_name(expression: &Expression<'_>, ordinal: usize) -> String {
    placeholder_path(expression).unwrap_or_else(|| format!("value_{ordinal}"))
}

/// 为静态可确定的读取链保留作者可识别的名称；动态索引和计算表达式必须回退到结构序号。
fn placeholder_path(expression: &Expression<'_>) -> Option<String> {
    match &expression.kind {
        ExpressionKind::Variable { scope, name } => {
            let prefix: char = match scope {
                VariableScope::Variables => '$',
                VariableScope::Temporary => '_',
                VariableScope::Local => '@',
            };
            Some(format!("{prefix}{name}"))
        }
        ExpressionKind::Global(name) => Some((*name).to_owned()),
        ExpressionKind::Setup => Some(String::from("setup")),
        ExpressionKind::Group(inner) => placeholder_path(inner),
        ExpressionKind::Member {
            target, property, ..
        }
        | ExpressionKind::OptionalMember {
            target, property, ..
        } => placeholder_path(target).map(|mut path: String| {
            path.push('.');
            path.push_str(property);
            path
        }),
        _ => None,
    }
}

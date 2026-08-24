//! 产生宿主无关 Presentation 语义的 Core Macro。

use std::fmt::Write;

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    expression::{
        evaluator::value_to_text,
        value::{TextValue, Value},
    },
    hir::HirBodyNode,
    macro_runtime::{
        CapturedMacroLocals, MacroInteraction, MacroInteractionError, MacroInteractions,
    },
    presentation::{
        InputGroupId, InteractionId, NavigationRole, PresentationInputBinding,
        PresentationInputKind, PresentationNode, PresentationOutput, PresentationRegion,
        PresentationTarget, PresentationValue, TextStyle, TextTone,
    },
    runtime::{BodyControl, BodyExecution, RuntimeExecutionIdentity},
};

/// 从 Twee `<<text value options?>>` 产生带语义样式的文字。
pub fn text(arguments: &[Value]) -> Result<BodyExecution, Diagnostic> {
    let Some(value) = arguments.first() else {
        return Err(text_error("`text` 至少需要一个文字参数"));
    };
    let text: TextValue =
        value_to_text(value).ok_or_else(|| text_error("`text` 的第一个参数必须能转换为文字"))?;
    let (styles, tone): (Vec<TextStyle>, TextTone) = match arguments.get(1) {
        None | Some(Value::Undefined | Value::Null) => (Vec::new(), TextTone::Default),
        Some(Value::Object(options)) => text_options(&options.snapshot())?,
        Some(value) => {
            let tone: TextTone = text_tone(value)?;
            let styles: Vec<TextStyle> = arguments[2..]
                .iter()
                .map(text_style)
                .collect::<Result<Vec<_>, _>>()?;
            (styles, tone)
        }
    };
    Ok(BodyExecution {
        control: BodyControl::Continue,
        output: PresentationOutput::from_nodes(vec![PresentationNode::StyledText {
            text,
            styles,
            tone,
        }]),
    })
}

fn text_options(properties: &[(String, Value)]) -> Result<(Vec<TextStyle>, TextTone), Diagnostic> {
    let mut styles: Vec<TextStyle> = Vec::new();
    let mut tone: TextTone = TextTone::Default;
    for (name, value) in properties {
        match name.as_str() {
            "styles" => {
                let Value::Array(values) = value else {
                    return Err(text_error("`text` options.styles 必须是 Array"));
                };
                styles = values
                    .snapshot()
                    .iter()
                    .map(text_style)
                    .collect::<Result<Vec<_>, _>>()?;
            }
            "tone" => tone = text_tone(value)?,
            name => return Err(text_error(&format!("`text` 不认识 options.{name}"))),
        }
    }
    Ok((styles, tone))
}

fn text_style(value: &Value) -> Result<TextStyle, Diagnostic> {
    match text_name(value)?.as_str() {
        "emphasis" => Ok(TextStyle::Emphasis),
        "strong" => Ok(TextStyle::Strong),
        "code" => Ok(TextStyle::Code),
        "deleted" => Ok(TextStyle::Deleted),
        "inserted" => Ok(TextStyle::Inserted),
        "marked" => Ok(TextStyle::Marked),
        "small" => Ok(TextStyle::Small),
        "subscript" => Ok(TextStyle::Subscript),
        "superscript" => Ok(TextStyle::Superscript),
        "quote" => Ok(TextStyle::Quote),
        "heading1" => Ok(TextStyle::Heading1),
        "heading2" => Ok(TextStyle::Heading2),
        "heading3" => Ok(TextStyle::Heading3),
        "heading4" => Ok(TextStyle::Heading4),
        "heading5" => Ok(TextStyle::Heading5),
        "heading6" => Ok(TextStyle::Heading6),
        name => Err(text_error(&format!("未知 TextStyle：{name}"))),
    }
}

fn text_tone(value: &Value) -> Result<TextTone, Diagnostic> {
    match text_name(value)?.as_str() {
        "default" => Ok(TextTone::Default),
        "muted" => Ok(TextTone::Muted),
        "accent" => Ok(TextTone::Accent),
        "informational" => Ok(TextTone::Informational),
        "positive" => Ok(TextTone::Positive),
        "warning" => Ok(TextTone::Warning),
        "negative" => Ok(TextTone::Negative),
        "critical" => Ok(TextTone::Critical),
        name => Err(text_error(&format!("未知 TextTone：{name}"))),
    }
}

fn text_name(value: &Value) -> Result<String, Diagnostic> {
    let Value::String(value) = value else {
        return Err(text_error("TextStyle 与 TextTone 必须是文字"));
    };
    value
        .to_unicode_string()
        .ok_or_else(|| text_error("TextStyle 与 TextTone 必须是有效 Unicode"))
}

fn text_error(message: &str) -> Diagnostic {
    Diagnostic::new(
        "macro.text.invalid_arguments",
        DiagnosticSeverity::Error,
        message,
    )
}

struct PreparedLink {
    id: InteractionId,
    label: TextValue,
    target: String,
}

/// 把准备完成的单个 `[[label|target]]` 参数转换为导航 Interaction。
///
/// 容器正文的延迟执行不在此函数中处理；该函数只建立玩家可选择的动作语义。
pub fn link(
    arguments: &[Value],
    identity: RuntimeExecutionIdentity,
) -> Result<BodyExecution, Diagnostic> {
    let prepared: PreparedLink = prepare_link(arguments, identity)?;
    Ok(link_output(prepared, NavigationRole::Link))
}

/// 建立导航语义，并保存玩家激活后才执行的容器正文。
///
/// 登记和输出是一个原子步骤：ID 冲突时保留已有动作，也不产生新导航。
pub fn link_with_body<'hir, 'source>(
    arguments: &[Value],
    identity: RuntimeExecutionIdentity,
    body: &'hir [HirBodyNode<'source>],
    captures: CapturedMacroLocals<Value>,
    interactions: &mut MacroInteractions<'hir, 'source>,
) -> Result<BodyExecution, Diagnostic> {
    let prepared: PreparedLink = prepare_link(arguments, identity)?;
    let action: MacroInteraction<'hir, 'source> =
        MacroInteraction::new(&prepared.target, body, captures);
    interactions
        .add(prepared.id.clone(), action)
        .map_err(|error: MacroInteractionError| link_interaction_error(error))?;
    Ok(link_output(prepared, NavigationRole::Link))
}

/// 与 link 共用延迟正文事务，但要求 Host 呈现为按钮控件。
pub fn button_with_body<'hir, 'source>(
    arguments: &[Value],
    identity: RuntimeExecutionIdentity,
    body: &'hir [HirBodyNode<'source>],
    captures: CapturedMacroLocals<Value>,
    interactions: &mut MacroInteractions<'hir, 'source>,
) -> Result<BodyExecution, Diagnostic> {
    let prepared: PreparedLink = prepare_link(arguments, identity)?;
    let action: MacroInteraction<'hir, 'source> =
        MacroInteraction::new(&prepared.target, body, captures);
    interactions
        .add(prepared.id.clone(), action)
        .map_err(link_interaction_error)?;
    Ok(link_output(prepared, NavigationRole::Button))
}

fn prepare_link(
    arguments: &[Value],
    identity: RuntimeExecutionIdentity,
) -> Result<PreparedLink, Diagnostic> {
    let [Value::Object(interaction)] = arguments else {
        return Err(link_error("`link` 必须只接收一个 Interaction Target 参数"));
    };
    let properties: Vec<(String, Value)> = interaction.snapshot();
    let label: TextValue = interaction_text(&properties, "label")?;
    let target_text: TextValue = interaction_text(&properties, "target")?;
    let target: String = target_text
        .to_unicode_string()
        .ok_or_else(|| link_error("`link` 的 Passage 目标必须是有效 Unicode 文本"))?;
    let id: InteractionId = link_identity(identity, &label, &target);

    Ok(PreparedLink { id, label, target })
}

fn link_output(prepared: PreparedLink, role: NavigationRole) -> BodyExecution {
    BodyExecution {
        control: BodyControl::Continue,
        output: PresentationOutput::from_nodes(vec![PresentationNode::Navigation {
            id: prepared.id,
            label: prepared.label,
            target: prepared.target,
            role,
        }]),
    }
}

fn interaction_text(properties: &[(String, Value)], name: &str) -> Result<TextValue, Diagnostic> {
    properties
        .iter()
        .find_map(|(key, value): &(String, Value)| {
            if key == name {
                match value {
                    Value::String(text) => Some(text.clone()),
                    _ => None,
                }
            } else {
                None
            }
        })
        .ok_or_else(|| link_error(&format!("`link` 的 `{name}` 必须是文本")))
}

fn link_identity(
    identity: RuntimeExecutionIdentity,
    label: &TextValue,
    target: &str,
) -> InteractionId {
    let mut key: String = format!("link:{}:{}:", identity.story, identity.chain);
    for unit in label.as_units() {
        write!(&mut key, "{unit:04x}").expect("写入 String 不会失败");
    }
    key.push(':');
    key.push_str(target);
    InteractionId::from_key(key)
}

fn link_error(message: &str) -> Diagnostic {
    Diagnostic::new(
        "macro.link.invalid_arguments",
        DiagnosticSeverity::Error,
        message,
    )
}

fn link_interaction_error(error: MacroInteractionError) -> Diagnostic {
    let message: &str = match error {
        MacroInteractionError::Duplicate => "`link` 生成了重复的 Interaction ID",
        MacroInteractionError::Missing => "`link` 无法登记 Interaction",
    };
    Diagnostic::new(
        "macro.link.interaction_registration_failed",
        DiagnosticSeverity::Error,
        message,
    )
}

/// 产生 `<<checkbox receiver unchecked checked>>` 的受验证输入语义。
pub fn checkbox(
    receiver: &str,
    unchecked: &Value,
    checked: &Value,
    current: &Value,
    identity: RuntimeExecutionIdentity,
    occurrence: usize,
) -> Result<BodyExecution, Diagnostic> {
    let unchecked: PresentationValue = input_value(unchecked)?;
    let checked: PresentationValue = input_value(checked)?;
    let selected: bool = input_value(current)? == checked;
    input_output(
        receiver,
        "checkbox",
        identity,
        occurrence,
        PresentationInputKind::Checkbox {
            unchecked,
            checked,
            selected,
        },
    )
}

/// 产生 `<<radiobutton receiver value>>` 的受验证输入语义。
pub fn radiobutton(
    receiver: &str,
    value: &Value,
    current: &Value,
    identity: RuntimeExecutionIdentity,
    occurrence: usize,
) -> Result<BodyExecution, Diagnostic> {
    let value: PresentationValue = input_value(value)?;
    let selected: bool = input_value(current)? == value;
    let group: InputGroupId = InputGroupId::from_key(format!(
        "radio-group:{}:{}:{receiver}",
        identity.story, identity.chain
    ));
    input_output(
        receiver,
        "radiobutton",
        identity,
        occurrence,
        PresentationInputKind::Radio {
            group,
            value,
            selected,
        },
    )
}

/// 产生 `<<textbox receiver default>>` 的受验证文字输入语义。
pub fn textbox(
    receiver: &str,
    current: &Value,
    identity: RuntimeExecutionIdentity,
    occurrence: usize,
) -> Result<BodyExecution, Diagnostic> {
    let text: TextValue =
        value_to_text(current).ok_or_else(|| input_error("`textbox` 当前值必须能转换为文字"))?;
    input_output(
        receiver,
        "textbox",
        identity,
        occurrence,
        PresentationInputKind::Text { value: text },
    )
}

fn input_output(
    receiver: &str,
    control: &str,
    identity: RuntimeExecutionIdentity,
    occurrence: usize,
    kind: PresentationInputKind,
) -> Result<BodyExecution, Diagnostic> {
    if receiver.starts_with('@') {
        return Err(input_error(
            "状态绑定输入暂不支持已经结束调用帧的 `@` receiver",
        ));
    }
    if !(receiver.starts_with('$') || receiver.starts_with('_')) {
        return Err(input_error("输入 receiver 必须以 `$` 或 `_` 开头"));
    }
    let id: InteractionId = InteractionId::from_key(format!(
        "input:{}:{}:{occurrence}:{control}:{receiver}",
        identity.story, identity.chain
    ));
    Ok(BodyExecution {
        control: BodyControl::Continue,
        output: PresentationOutput::from_nodes(vec![PresentationNode::Input {
            id,
            binding: PresentationInputBinding {
                receiver: receiver.to_owned(),
                kind,
            },
        }]),
    })
}

fn input_value(value: &Value) -> Result<PresentationValue, Diagnostic> {
    match value {
        Value::Undefined | Value::Null => Ok(PresentationValue::Null),
        Value::Boolean(value) => Ok(PresentationValue::Boolean(*value)),
        Value::Number(value) if value.is_finite() => Ok(PresentationValue::Number(*value)),
        Value::Number(_) => Err(input_error("输入值必须是有限数值")),
        Value::String(value) => value
            .to_unicode_string()
            .map(PresentationValue::Text)
            .ok_or_else(|| input_error("输入值必须是有效 Unicode")),
        Value::Array(values) => values
            .snapshot()
            .iter()
            .map(input_value)
            .collect::<Result<Vec<_>, _>>()
            .map(PresentationValue::List),
        Value::Object(values) => values
            .snapshot()
            .iter()
            .map(|(name, value)| Ok((name.clone(), input_value(value)?)))
            .collect::<Result<std::collections::BTreeMap<_, _>, Diagnostic>>()
            .map(PresentationValue::Map),
        Value::Callable(_) | Value::ScriptCallable(_) | Value::Namespace(_) => {
            Err(input_error("输入值不能包含函数或命名空间"))
        }
    }
}

fn input_error(message: &str) -> Diagnostic {
    Diagnostic::new(
        "macro.input.invalid_arguments",
        DiagnosticSeverity::Error,
        message,
    )
}

/// 把容器正文包装为跨 Host 的区域或稳定 key 替换语义。
pub fn replace(target: &str, content: PresentationOutput) -> Result<BodyExecution, Diagnostic> {
    let target: &str = target.trim();
    if target.is_empty() {
        return Err(replace_error("`replace` 目标不能为空"));
    }
    let target: PresentationTarget = match target {
        "header" => PresentationTarget::Region(PresentationRegion::Header),
        "main" => PresentationTarget::Region(PresentationRegion::Main),
        "footer" => PresentationTarget::Region(PresentationRegion::Footer),
        "bar" => PresentationTarget::Region(PresentationRegion::Bar),
        "bar-stowed" => PresentationTarget::Region(PresentationRegion::BarStowed),
        "dialog" => PresentationTarget::Region(PresentationRegion::Dialog),
        key => PresentationTarget::Key(
            crate::presentation::PresentationKey::parse(key)
                .map_err(|_| replace_error("`replace` key 无效"))?,
        ),
    };
    Ok(BodyExecution {
        control: BodyControl::Continue,
        output: PresentationOutput::from_nodes(vec![PresentationNode::Replace { target, content }]),
    })
}

/// 建立一个由稳定 key 标识的普通内容槽，供后续 `replace` 跨 Host 定位。
pub fn slot(key: &str, content: PresentationOutput) -> Result<BodyExecution, Diagnostic> {
    let key: &str = key.trim();
    if key.is_empty() {
        return Err(slot_error("`slot` key 不能为空"));
    }
    if matches!(key, "header" | "main" | "footer" | "bar" | "dialog") {
        return Err(slot_error("`slot` key 不能使用保留的 Region 名称"));
    }
    let key = crate::presentation::PresentationKey::parse(key)
        .map_err(|_| slot_error("`slot` key 无效"))?;
    let mut output = PresentationOutput::default();
    output
        .push_keyed(key, PresentationNode::Container { content })
        .map_err(|error| slot_error(&error.to_string()))?;
    Ok(BodyExecution {
        control: BodyControl::Continue,
        output,
    })
}

fn slot_error(message: &str) -> Diagnostic {
    Diagnostic::new("macro.slot.invalid_key", DiagnosticSeverity::Error, message)
}

fn replace_error(message: &str) -> Diagnostic {
    Diagnostic::new(
        "macro.replace.invalid_target",
        DiagnosticSeverity::Error,
        message,
    )
}

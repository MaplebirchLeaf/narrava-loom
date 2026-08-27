//! 产生宿主无关 Surface 语义的 Core Macro。

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
    protocol::{
        HeadingLevel, InputGroupId, InteractionId, NavigationRole, RegionId, Surface,
        SurfaceInputBinding, SurfaceInputKind, SurfaceNode, SurfaceTarget, SurfaceValue, TextColor,
        TextStyle,
    },
    runtime::{BodyControl, BodyExecution, RuntimeExecutionIdentity},
};

/// 从 Twee `<<print value options?>>` 求值并入 Passage 输出；带选项时产生 StyledText。
///
/// 单参数（无样式选项）输出纯 Text，与编译器固有 `<<print expression>>` 一致；
/// 带选项时由 `print` 直接构造 StyledText：
/// - `value`：内容，可来自变量或其他 Twee Expression；
/// - `options?`：可为 color 字符串（随后可跟多个 style 字符串），或对象
///   `{ color, styles, delay, heading }`；
/// - `color`：0..=63 的标准调色板索引；`styles`：8 个语义字形之一；
/// - `delay`：毫秒，渲染器在此之前不呈现文本，不约定到期后的动画；
/// - `heading`：1 或 2 的结构性标题级别，用于页面划分（如弹窗页签标题），不是字形样式。
pub fn print(arguments: &[Value]) -> Result<BodyExecution, Diagnostic> {
    let Some(value) = arguments.first() else {
        return Err(print_error("`print` 至少需要一个文字参数"));
    };
    let text: TextValue =
        value_to_text(value).ok_or_else(|| print_error("`print` 的第一个参数必须能转换为文字"))?;
    let options: PrintOptions = match arguments.get(1) {
        None | Some(Value::Undefined | Value::Null) => PrintOptions::default(),
        Some(Value::Object(options)) => print_options(&options.snapshot())?,
        Some(value) => {
            let color: TextColor = print_color(value)?;
            let styles: Vec<TextStyle> = arguments[2..]
                .iter()
                .map(print_style)
                .collect::<Result<Vec<_>, _>>()?;
            PrintOptions {
                styles,
                color,
                delay: None,
                heading: None,
            }
        }
    };
    let node: SurfaceNode = if options.styles.is_empty()
        && options.color == TextColor::DEFAULT
        && options.delay.is_none()
        && options.heading.is_none()
    {
        SurfaceNode::Text(text)
    } else {
        SurfaceNode::StyledText {
            text,
            styles: options.styles,
            color: options.color,
            delay: options.delay,
            heading: options.heading,
        }
    };
    Ok(BodyExecution {
        control: BodyControl::Continue,
        output: Surface::from_nodes(vec![node]),
    })
}

/// 解析 `print` 的对象 options；未知字段名直接报错。
#[derive(Default)]
struct PrintOptions {
    styles: Vec<TextStyle>,
    color: TextColor,
    delay: Option<u64>,
    heading: Option<HeadingLevel>,
}

fn print_options(properties: &[(String, Value)]) -> Result<PrintOptions, Diagnostic> {
    let mut options: PrintOptions = PrintOptions::default();
    for (name, value) in properties {
        match name.as_str() {
            "styles" => {
                let Value::Array(values) = value else {
                    return Err(print_error("`print` options.styles 必须是 Array"));
                };
                options.styles = values
                    .snapshot()
                    .iter()
                    .map(print_style)
                    .collect::<Result<Vec<_>, _>>()?;
            }
            "color" => options.color = print_color(value)?,
            "delay" => options.delay = Some(print_delay(value)?),
            "heading" => options.heading = Some(print_heading(value)?),
            name => return Err(print_error(&format!("`print` 不认识 options.{name}"))),
        }
    }
    Ok(options)
}

/// 解析 `heading`（结构性标题级别）：必须为 1 或 2 的整数。
fn print_heading(value: &Value) -> Result<HeadingLevel, Diagnostic> {
    let Value::Number(level) = value else {
        return Err(print_error("`print` options.heading 必须是 1 或 2 的整数"));
    };
    let level: f64 = *level;
    if level.fract() != 0.0 || !(1.0..=2.0).contains(&level) {
        return Err(print_error("`print` options.heading 必须是 1 或 2 的整数"));
    }
    HeadingLevel::from_u8(level as u8)
        .ok_or_else(|| print_error("`print` options.heading 必须是 1 或 2 的整数"))
}

/// 解析 `delay`（毫秒）：非负整数，上限与 `Host.delay` 一致。
fn print_delay(value: &Value) -> Result<u64, Diagnostic> {
    let Value::Number(milliseconds) = value else {
        return Err(print_error("`print` options.delay 必须是数值毫秒"));
    };
    let milliseconds: f64 = *milliseconds;
    if !milliseconds.is_finite() || !(0.0..=86_400_000.0).contains(&milliseconds) {
        return Err(print_error(
            "`print` options.delay 必须在 0 到 86400000 毫秒之间",
        ));
    }
    if milliseconds.fract() != 0.0 {
        return Err(print_error("`print` options.delay 必须是整数毫秒"));
    }
    Ok(milliseconds as u64)
}

fn print_style(value: &Value) -> Result<TextStyle, Diagnostic> {
    match text_name(value)?.as_str() {
        "emphasis" => Ok(TextStyle::Emphasis),
        "strong" => Ok(TextStyle::Strong),
        "code" => Ok(TextStyle::Code),
        "marked" => Ok(TextStyle::Marked),
        "small" => Ok(TextStyle::Small),
        "inserted" => Ok(TextStyle::Inserted),
        "deleted" => Ok(TextStyle::Deleted),
        "quote" => Ok(TextStyle::Quote),
        name => Err(print_error(&format!("未知 TextStyle：{name}"))),
    }
}

/// 解析 `color`：0..=63 的整数标准色号；不接收颜色或语义字符串名。
fn print_color(value: &Value) -> Result<TextColor, Diagnostic> {
    let Value::Number(index) = value else {
        return Err(print_error("`print` color 必须是 0 到 63 的整数"));
    };
    let index: f64 = *index;
    if !index.is_finite() || !(0.0..=63.0).contains(&index) || index.fract() != 0.0 {
        return Err(print_error("`print` color 必须是 0 到 63 的整数"));
    }
    TextColor::from_index(index as u8)
        .ok_or_else(|| print_error("`print` color 必须是 0 到 63 的整数"))
}

fn text_name(value: &Value) -> Result<String, Diagnostic> {
    let Value::String(value) = value else {
        return Err(print_error("TextStyle 与 TextColor 必须是文字"));
    };
    value
        .to_unicode_string()
        .ok_or_else(|| print_error("TextStyle 与 TextColor 必须是有效 Unicode"))
}

/// 构造 `print` 参数错误的统一稳定 Diagnostic。
fn print_error(message: &str) -> Diagnostic {
    Diagnostic::new(
        "macro.print.invalid_arguments",
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

/// 校验单个 Interaction Target 参数并计算稳定 Interaction ID。
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

/// 把准备好的链接组装为导航语义输出。
fn link_output(prepared: PreparedLink, role: NavigationRole) -> BodyExecution {
    BodyExecution {
        control: BodyControl::Continue,
        output: Surface::from_nodes(vec![SurfaceNode::Navigation {
            id: prepared.id,
            label: prepared.label,
            target: prepared.target,
            role,
        }]),
    }
}

/// 从 Interaction 参数对象中读取指定字段并强制为文本。
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

/// 由执行身份、显示文本与目标生成稳定的链接 ID。
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

/// 构造 `link` 参数错误的统一稳定 Diagnostic。
fn link_error(message: &str) -> Diagnostic {
    Diagnostic::new(
        "macro.link.invalid_arguments",
        DiagnosticSeverity::Error,
        message,
    )
}

/// 把 Interaction 登记失败映射为稳定 Diagnostic。
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
    let unchecked: SurfaceValue = input_value(unchecked)?;
    let checked: SurfaceValue = input_value(checked)?;
    let selected: bool = input_value(current)? == checked;
    input_output(
        receiver,
        "checkbox",
        identity,
        occurrence,
        SurfaceInputKind::Checkbox {
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
    let value: SurfaceValue = input_value(value)?;
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
        SurfaceInputKind::Radio {
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
        SurfaceInputKind::Text { value: text },
    )
}

/// 校验输入 receiver 并组装输入语义输出。
fn input_output(
    receiver: &str,
    control: &str,
    identity: RuntimeExecutionIdentity,
    occurrence: usize,
    kind: SurfaceInputKind,
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
        output: Surface::from_nodes(vec![SurfaceNode::Input {
            id,
            binding: SurfaceInputBinding {
                receiver: receiver.to_owned(),
                kind,
            },
        }]),
    })
}

/// 把运行时值递归转换为可呈现的输入值；函数与命名空间被拒绝。
fn input_value(value: &Value) -> Result<SurfaceValue, Diagnostic> {
    match value {
        Value::Undefined | Value::Null => Ok(SurfaceValue::Null),
        Value::Boolean(value) => Ok(SurfaceValue::Boolean(*value)),
        Value::Number(value) if value.is_finite() => Ok(SurfaceValue::Number(*value)),
        Value::Number(_) => Err(input_error("输入值必须是有限数值")),
        Value::String(value) => value
            .to_unicode_string()
            .map(SurfaceValue::Text)
            .ok_or_else(|| input_error("输入值必须是有效 Unicode")),
        Value::Array(values) => values
            .snapshot()
            .iter()
            .map(input_value)
            .collect::<Result<Vec<_>, _>>()
            .map(SurfaceValue::List),
        Value::Object(values) => values
            .snapshot()
            .iter()
            .map(|(name, value)| Ok((name.clone(), input_value(value)?)))
            .collect::<Result<std::collections::BTreeMap<_, _>, Diagnostic>>()
            .map(SurfaceValue::Map),
        Value::Callable(_) | Value::ScriptCallable(_) | Value::Namespace(_) => {
            Err(input_error("输入值不能包含函数或命名空间"))
        }
    }
}

/// 构造输入 Macro 参数错误的统一稳定 Diagnostic。
fn input_error(message: &str) -> Diagnostic {
    Diagnostic::new(
        "macro.input.invalid_arguments",
        DiagnosticSeverity::Error,
        message,
    )
}

/// 把容器正文包装为跨 Host 的区域或稳定 key 替换语义。
pub fn replace(target: &str, content: Surface) -> Result<BodyExecution, Diagnostic> {
    let target: &str = target.trim();
    if target.is_empty() {
        return Err(replace_error("`replace` 目标不能为空"));
    }
    let target: SurfaceTarget = match target {
        "header" => SurfaceTarget::Region(RegionId::header()),
        "main" => SurfaceTarget::Region(RegionId::main()),
        "footer" => SurfaceTarget::Region(RegionId::footer()),
        "bar" => SurfaceTarget::Region(RegionId::bar()),
        "bar-stowed" => SurfaceTarget::Region(RegionId::bar_stowed()),
        "dialog" => SurfaceTarget::Region(RegionId::dialog()),
        key => SurfaceTarget::Key(
            crate::protocol::SurfaceKey::parse(key)
                .map_err(|_| replace_error("`replace` key 无效"))?,
        ),
    };
    Ok(BodyExecution {
        control: BodyControl::Continue,
        output: Surface::from_nodes(vec![SurfaceNode::Replace { target, content }]),
    })
}

/// 建立一个由稳定 key 标识的普通内容槽，供后续 `replace` 跨 Host 定位。
pub fn slot(key: &str, content: Surface) -> Result<BodyExecution, Diagnostic> {
    let key: &str = key.trim();
    if key.is_empty() {
        return Err(slot_error("`slot` key 不能为空"));
    }
    if matches!(key, "header" | "main" | "footer" | "bar" | "dialog") {
        return Err(slot_error("`slot` key 不能使用保留的 Region 名称"));
    }
    let key = crate::protocol::SurfaceKey::parse(key).map_err(|_| slot_error("`slot` key 无效"))?;
    let mut output = Surface::default();
    output
        .push_keyed(key, SurfaceNode::Container { content })
        .map_err(|error| slot_error(&error.to_string()))?;
    Ok(BodyExecution {
        control: BodyControl::Continue,
        output,
    })
}

/// 构造 `slot` key 错误的统一稳定 Diagnostic。
fn slot_error(message: &str) -> Diagnostic {
    Diagnostic::new("macro.slot.invalid_key", DiagnosticSeverity::Error, message)
}

/// 构造 `replace` 目标错误的统一稳定 Diagnostic。
fn replace_error(message: &str) -> Diagnostic {
    Diagnostic::new(
        "macro.replace.invalid_target",
        DiagnosticSeverity::Error,
        message,
    )
}

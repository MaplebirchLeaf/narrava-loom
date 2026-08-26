//! ECMAScript Presentation builder 值到 Core 语义输出的受验证转换。

use std::collections::BTreeMap;

use narrava_loom_core::{
    expression::value::{ObjectValue, TextValue, Value},
    presentation::{
        ActionRole, ComponentCapability, HeadingLevel, PresentationAction, PresentationKey,
        PresentationNode, PresentationOutput, PresentationRegion, PresentationValue, TextStyle,
        TextTone,
    },
    resource::ResourcePath,
};

use crate::HostErrorDto;

const MARKER: &str = "__narravaPresentation";
const MAX_DEPTH: usize = 32;

/// 若脚本返回值带 Presentation 标记则解析为语义输出，否则返回 `None`。
pub(super) fn output(value: &Value) -> Result<Option<PresentationOutput>, HostErrorDto> {
    let Value::Object(object) = value else {
        return Ok(None);
    };
    if property(object, MARKER).is_none() {
        return Ok(None);
    }
    parse_output(value, 0).map(Some)
}

/// 解析标记对象：`fragment` 只作输出/Region 子内容，其余按单节点处理。
fn parse_output(value: &Value, depth: usize) -> Result<PresentationOutput, HostErrorDto> {
    if depth > MAX_DEPTH {
        return Err(invalid("Presentation 嵌套超过 32 层"));
    }
    let object = object(value)?;
    match string_property(object, MARKER)?.as_str() {
        "fragment" => parse_children(object, depth + 1),
        _ => {
            let mut output = PresentationOutput::default();
            push_node(&mut output, object, depth)?;
            Ok(output)
        }
    }
}

/// 校验并推送单个节点；带 `key` 时按 key 入树，否则追加。
fn push_node(
    output: &mut PresentationOutput,
    object: &ObjectValue,
    depth: usize,
) -> Result<(), HostErrorDto> {
    let kind = string_property(object, MARKER)?;
    let node = match kind.as_str() {
        "text" => PresentationNode::StyledText {
            text: TextValue::from(string_property(object, "text")?),
            styles: styles(object)?,
            tone: tone(object)?,
            delay: delay(object)?,
            heading: heading(object)?,
        },
        "image" => {
            let resource = string_property(object, "resource")?;
            ResourcePath::parse(&resource).map_err(|error| invalid(error.to_string()))?;
            PresentationNode::Image {
                resource,
                alt: TextValue::from(optional_string(object, "alt")?.unwrap_or_default()),
                caption: optional_string(object, "caption")?.map(TextValue::from),
            }
        }
        "region" => PresentationNode::Region {
            region: region(object)?,
            content: parse_children(object, depth + 1)?,
        },
        "component" => {
            let version = number_property(object, "version")?;
            if version.fract() != 0.0 || !(1.0..=f64::from(u16::MAX)).contains(&version) {
                return Err(invalid(
                    "Presentation component version 必须是 1..65535 的整数",
                ));
            }
            PresentationNode::Component {
                capability: ComponentCapability::parse(string_property(object, "capability")?)
                    .map_err(|error| invalid(error.to_string()))?,
                version: version as u16,
                properties: properties(object)?,
                fallback: parse_children(object, depth + 1)?,
            }
        }
        "action" => PresentationNode::Action {
            label: TextValue::from(string_property(object, "label")?),
            action: match string_property(object, "action")?.as_str() {
                "dismiss" => PresentationAction::Dismiss,
                action => return Err(invalid(format!("未知 Presentation action：{action}"))),
            },
            role: match optional_string(object, "role")?
                .as_deref()
                .unwrap_or("default")
            {
                "default" => ActionRole::Default,
                "primary" => ActionRole::Primary,
                "secondary" => ActionRole::Secondary,
                "danger" => ActionRole::Danger,
                role => return Err(invalid(format!("未知 Presentation action role：{role}"))),
            },
        },
        "fragment" => return Err(invalid("Fragment 只能作为输出或 Region 子内容")),
        _ => return Err(invalid(format!("未知 Presentation 节点：{kind}"))),
    };
    match optional_string(object, "key")? {
        Some(key) => output
            .push_keyed(
                PresentationKey::parse(key).map_err(|error| invalid(error.to_string()))?,
                node,
            )
            .map_err(|error| invalid(error.to_string())),
        None => {
            output.push(node);
            Ok(())
        }
    }
}

/// 解析 `children` 数组：字符串直接成文本，对象递归为节点。
fn parse_children(object: &ObjectValue, depth: usize) -> Result<PresentationOutput, HostErrorDto> {
    let Value::Array(children) = required_property(object, "children")? else {
        return Err(invalid("Presentation children 必须是数组"));
    };
    let mut output = PresentationOutput::default();
    for child in children.snapshot() {
        match child {
            Value::String(text) => output.push(PresentationNode::Text(text)),
            Value::Object(object) => push_node(&mut output, &object, depth)?,
            _ => {
                return Err(invalid(
                    "Presentation children 只能包含文本或 Presentation 节点",
                ));
            }
        }
    }
    Ok(output)
}

/// 解析 8 个语义字形名（emphasis/strong/code/quote/marked/small/inserted/deleted）。
fn styles(object: &ObjectValue) -> Result<Vec<TextStyle>, HostErrorDto> {
    let Some(value) = property(object, "styles") else {
        return Ok(Vec::new());
    };
    let Value::Array(values) = value else {
        return Err(invalid("Presentation styles 必须是数组"));
    };
    values
        .snapshot()
        .into_iter()
        .map(|value| match unicode(&value)?.as_str() {
            "emphasis" => Ok(TextStyle::Emphasis),
            "strong" => Ok(TextStyle::Strong),
            "code" => Ok(TextStyle::Code),
            "marked" => Ok(TextStyle::Marked),
            "small" => Ok(TextStyle::Small),
            "inserted" => Ok(TextStyle::Inserted),
            "deleted" => Ok(TextStyle::Deleted),
            "quote" => Ok(TextStyle::Quote),
            style => Err(invalid(format!("未知 Presentation text style：{style}"))),
        })
        .collect()
}

/// 解析 0..=63 的整数 tone；缺省或 null 用 `TextTone::DEFAULT`。
fn tone(object: &ObjectValue) -> Result<TextTone, HostErrorDto> {
    match property(object, "tone") {
        None | Some(Value::Undefined | Value::Null) => Ok(TextTone::DEFAULT),
        Some(Value::Number(index)) => {
            let index: f64 = index;
            if !index.is_finite() || !(0.0..=63.0).contains(&index) || index.fract() != 0.0 {
                return Err(invalid("Presentation text tone 必须是 0 到 63 的整数"));
            }
            TextTone::from_index(index as u8)
                .ok_or_else(|| invalid("Presentation text tone 必须是 0 到 63 的整数"))
        }
        Some(_) => Err(invalid("Presentation text tone 必须是 0 到 63 的整数")),
    }
}

/// 解析区域名（header/main/footer/bar/bar-stowed/dialog）。
fn region(object: &ObjectValue) -> Result<PresentationRegion, HostErrorDto> {
    match string_property(object, "region")?.as_str() {
        "header" => Ok(PresentationRegion::Header),
        "main" => Ok(PresentationRegion::Main),
        "footer" => Ok(PresentationRegion::Footer),
        "bar" => Ok(PresentationRegion::Bar),
        "bar-stowed" => Ok(PresentationRegion::BarStowed),
        "dialog" => Ok(PresentationRegion::Dialog),
        region => Err(invalid(format!("未知 Presentation region：{region}"))),
    }
}

/// 解析组件属性为纯数据 Presentation 值。
fn properties(object: &ObjectValue) -> Result<BTreeMap<String, PresentationValue>, HostErrorDto> {
    let Value::Object(properties) = required_property(object, "properties")? else {
        return Err(invalid("Presentation component properties 必须是对象"));
    };
    properties
        .snapshot()
        .into_iter()
        .map(|(name, value)| Ok((name, presentation_value(&value)?)))
        .collect()
}

/// 任意 Core 值 → Presentation 值（函数/命名空间拒绝）。
fn presentation_value(value: &Value) -> Result<PresentationValue, HostErrorDto> {
    match value {
        Value::Undefined | Value::Null => Ok(PresentationValue::Null),
        Value::Boolean(value) => Ok(PresentationValue::Boolean(*value)),
        Value::Number(value) if value.is_finite() => Ok(PresentationValue::Number(*value)),
        Value::String(value) => value
            .to_unicode_string()
            .map(PresentationValue::Text)
            .ok_or_else(|| invalid("Component 文本必须是有效 Unicode")),
        Value::Array(values) => values
            .snapshot()
            .iter()
            .map(presentation_value)
            .collect::<Result<Vec<_>, _>>()
            .map(PresentationValue::List),
        Value::Object(values) => values
            .snapshot()
            .iter()
            .map(|(name, value)| Ok((name.clone(), presentation_value(value)?)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(PresentationValue::Map),
        _ => Err(invalid("Component properties 只能包含有限纯数据")),
    }
}

/// 取值对象，非对象报错。
fn object(value: &Value) -> Result<&ObjectValue, HostErrorDto> {
    match value {
        Value::Object(object) => Ok(object),
        _ => Err(invalid("Presentation 值必须是对象")),
    }
}

/// 取可选属性（未出现视为缺失）。
fn property(object: &ObjectValue, name: &str) -> Option<Value> {
    object
        .snapshot()
        .into_iter()
        .find_map(|(key, value)| (key == name).then_some(value))
}

/// 取必填属性，缺失报错。
fn required_property(object: &ObjectValue, name: &str) -> Result<Value, HostErrorDto> {
    property(object, name).ok_or_else(|| invalid(format!("Presentation 缺少 `{name}`")))
}

/// 取必填 Unicode 字符串属性。
fn string_property(object: &ObjectValue, name: &str) -> Result<String, HostErrorDto> {
    unicode(&required_property(object, name)?)
}

/// 取必填数字属性。
fn number_property(object: &ObjectValue, name: &str) -> Result<f64, HostErrorDto> {
    match required_property(object, name)? {
        Value::Number(value) => Ok(value),
        _ => Err(invalid(format!("Presentation `{name}` 必须是数字"))),
    }
}

/// 取可选字符串属性；缺失或 null/undefined 视为无。
fn optional_string(object: &ObjectValue, name: &str) -> Result<Option<String>, HostErrorDto> {
    match property(object, name) {
        None | Some(Value::Undefined | Value::Null) => Ok(None),
        Some(value) => unicode(&value).map(Some),
    }
}

/// 读取可选 `delay`（毫秒）：0..=86_400_000 的非负整数，上限与 `Host.delay` 一致。
fn delay(object: &ObjectValue) -> Result<Option<u64>, HostErrorDto> {
    match property(object, "delay") {
        None | Some(Value::Undefined | Value::Null) => Ok(None),
        Some(Value::Number(milliseconds)) => {
            if !milliseconds.is_finite()
                || !(0.0..=86_400_000.0).contains(&milliseconds)
                || milliseconds.fract() != 0.0
            {
                return Err(invalid(
                    "Presentation text delay 必须是 0 到 86400000 的整数毫秒",
                ));
            }
            Ok(Some(milliseconds as u64))
        }
        Some(_) => Err(invalid("Presentation text delay 必须是数值毫秒")),
    }
}

/// 读取可选 `heading`（结构性标题级别）：必须为 1 或 2 的整数。
fn heading(object: &ObjectValue) -> Result<Option<HeadingLevel>, HostErrorDto> {
    match property(object, "heading") {
        None | Some(Value::Undefined | Value::Null) => Ok(None),
        Some(Value::Number(level)) => {
            let level: f64 = level;
            if !level.is_finite() || level.fract() != 0.0 || !(1.0..=2.0).contains(&level) {
                return Err(invalid("Presentation text heading 必须是 1 或 2 的整数"));
            }
            HeadingLevel::from_u8(level as u8)
                .map(Some)
                .ok_or_else(|| invalid("Presentation text heading 必须是 1 或 2 的整数"))
        }
        Some(_) => Err(invalid("Presentation text heading 必须是 1 或 2 的整数")),
    }
}

/// 取 Unicode 字符串，非字符串或非 Unicode 报错。
fn unicode(value: &Value) -> Result<String, HostErrorDto> {
    match value {
        Value::String(text) => text
            .to_unicode_string()
            .ok_or_else(|| invalid("Presentation 文本必须是有效 Unicode")),
        _ => Err(invalid("Presentation 字段必须是文本")),
    }
}

/// 统一 Presentation 校验错误。
fn invalid(message: impl Into<String>) -> HostErrorDto {
    HostErrorDto::new("tauri_host.presentation", message)
}

#[cfg(test)]
mod tests {
    use narrava_loom_core::{
        expression::value::{TextValue, Value},
        presentation::{PresentationNode, PresentationRegion, TextStyle, TextTone},
    };

    use super::output;

    fn text(value: &str) -> Value {
        Value::String(TextValue::from(value))
    }

    /// 脚本 builder 值被解析为带 key 的语义节点树。
    #[test]
    fn builder_values_become_keyed_semantic_text_image_and_regions() {
        let value = Value::object(vec![
            ("__narravaPresentation".into(), text("region")),
            ("region".into(), text("bar")),
            ("key".into(), text("status")),
            (
                "children".into(),
                Value::array(vec![Value::object(vec![
                    ("__narravaPresentation".into(), text("text")),
                    ("text".into(), text("危险")),
                    ("styles".into(), Value::array(vec![text("strong")])),
                    ("tone".into(), Value::Number(8.0)),
                ])]),
            ),
        ]);

        let output = output(&value).unwrap().unwrap();

        assert_eq!(output.key(0).unwrap().as_str(), "status");
        let [PresentationNode::Region { region, content }] = output.nodes() else {
            panic!("应转换 Region");
        };
        assert_eq!(*region, PresentationRegion::Bar);
        assert!(matches!(
            content.nodes(),
            [PresentationNode::StyledText { styles, tone: TextTone::RED, .. }]
                if styles == &[TextStyle::Strong]
        ));
    }

    /// 未知样式等 DOM 风格值被拒绝，错误码统一为 `tauri_host.presentation`。
    #[test]
    fn builder_values_reject_dom_names_and_unknown_semantics() {
        let value = Value::object(vec![
            ("__narravaPresentation".into(), text("text")),
            ("text".into(), text("危险")),
            ("styles".into(), Value::array(vec![text("red")])),
        ]);

        assert_eq!(output(&value).unwrap_err().code, "tauri_host.presentation");
    }
}

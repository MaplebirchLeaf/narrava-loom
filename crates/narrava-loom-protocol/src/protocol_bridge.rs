//! ECMAScript Surface builder 值到 Core 语义输出的受验证转换。

use std::collections::BTreeMap;

use narrava_loom_core::{
    expression::value::{ObjectValue, TextValue, Value},
    resource::ResourcePath,
    semantic::{ActionRole, ComponentCapability, HeadingLevel, RegionId, TextColor, TextStyle},
};

use crate::{
    HostErrorDto,
    surface::{Surface, SurfaceAction, SurfaceKey, SurfaceNode, SurfaceValue},
};

const MARKER: &str = "__narravaSurface";
const MAX_DEPTH: usize = 32;

/// 若脚本返回值带 Surface 标记则解析为语义输出，否则返回 `None`。
pub fn output(value: &Value) -> Result<Option<Surface>, HostErrorDto> {
    let Value::Object(object) = value else {
        return Ok(None);
    };
    if property(object, MARKER).is_none() {
        return Ok(None);
    }
    parse_output(value, 0).map(Some)
}

/// 解析标记对象：`fragment` 只作输出/Region 子内容，其余按单节点处理。
fn parse_output(value: &Value, depth: usize) -> Result<Surface, HostErrorDto> {
    if depth > MAX_DEPTH {
        return Err(invalid("Surface 嵌套超过 32 层"));
    }
    let object = object(value)?;
    match string_property(object, MARKER)?.as_str() {
        "fragment" => parse_children(object, depth + 1),
        _ => {
            let mut output = Surface::default();
            push_node(&mut output, object, depth)?;
            Ok(output)
        }
    }
}

/// 校验并推送单个节点；带 `key` 时按 key 入树，否则追加。
fn push_node(output: &mut Surface, object: &ObjectValue, depth: usize) -> Result<(), HostErrorDto> {
    let kind = string_property(object, MARKER)?;
    let node = match kind.as_str() {
        "text" => SurfaceNode::StyledText {
            text: visible_text(string_property(object, "text")?)?,
            styles: styles(object)?,
            color: color(object)?,
            delay: delay(object)?,
            heading: heading(object)?,
        },
        "hard-break" => SurfaceNode::HardBreak,
        "image" => {
            let resource = string_property(object, "resource")?;
            ResourcePath::parse(&resource).map_err(|error| invalid(error.to_string()))?;
            SurfaceNode::Image {
                resource,
                alt: visible_text(optional_string(object, "alt")?.unwrap_or_default())?,
                caption: optional_string(object, "caption")?
                    .map(visible_text)
                    .transpose()?,
            }
        }
        "region" => SurfaceNode::Region {
            region: region(object)?,
            content: parse_children(object, depth + 1)?,
        },
        "component" => {
            let version = number_property(object, "version")?;
            if version.fract() != 0.0 || !(1.0..=f64::from(u16::MAX)).contains(&version) {
                return Err(invalid("Surface component version 必须是 1..65535 的整数"));
            }
            SurfaceNode::Component {
                capability: ComponentCapability::parse(string_property(object, "capability")?)
                    .map_err(|error| invalid(error.to_string()))?,
                version: version as u16,
                properties: properties(object)?,
                fallback: parse_children(object, depth + 1)?,
            }
        }
        "action" => SurfaceNode::Action {
            label: visible_text(string_property(object, "label")?)?,
            action: match string_property(object, "action")?.as_str() {
                "dismiss" => SurfaceAction::Dismiss,
                action => return Err(invalid(format!("未知 Surface action：{action}"))),
            },
            role: match optional_string(object, "role")?
                .as_deref()
                .unwrap_or("default")
            {
                "default" => ActionRole::Default,
                "primary" => ActionRole::Primary,
                "secondary" => ActionRole::Secondary,
                "danger" => ActionRole::Danger,
                role => return Err(invalid(format!("未知 Surface action role：{role}"))),
            },
        },
        "fragment" => return Err(invalid("Fragment 只能作为输出或 Region 子内容")),
        _ => return Err(invalid(format!("未知 Surface 节点：{kind}"))),
    };
    match optional_string(object, "key")? {
        Some(key) => output
            .push_keyed(
                SurfaceKey::parse(key).map_err(|error| invalid(error.to_string()))?,
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
fn parse_children(object: &ObjectValue, depth: usize) -> Result<Surface, HostErrorDto> {
    let Value::Array(children) = required_property(object, "children")? else {
        return Err(invalid("Surface children 必须是数组"));
    };
    let mut output = Surface::default();
    for child in children.snapshot() {
        match child {
            Value::String(text) => {
                let text: String = text
                    .to_unicode_string()
                    .ok_or_else(|| invalid("Surface 文本必须是有效 Unicode"))?;
                output.push(SurfaceNode::Text(visible_text(text)?));
            }
            Value::Object(object) => push_node(&mut output, &object, depth)?,
            _ => {
                return Err(invalid("Surface children 只能包含文本或 Surface 节点"));
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
        return Err(invalid("Surface styles 必须是数组"));
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
            style => Err(invalid(format!("未知 Surface text style：{style}"))),
        })
        .collect()
}

/// 解析 0..=63 的整数 color；缺省或 null 用 `TextColor::DEFAULT`。
fn color(object: &ObjectValue) -> Result<TextColor, HostErrorDto> {
    match property(object, "color") {
        None | Some(Value::Undefined | Value::Null) => Ok(TextColor::DEFAULT),
        Some(Value::Number(index)) => {
            let index: f64 = index;
            if !index.is_finite() || !(0.0..=63.0).contains(&index) || index.fract() != 0.0 {
                return Err(invalid("Surface text color 必须是 0 到 63 的整数"));
            }
            TextColor::from_index(index as u8)
                .ok_or_else(|| invalid("Surface text color 必须是 0 到 63 的整数"))
        }
        Some(_) => Err(invalid("Surface text color 必须是 0 到 63 的整数")),
    }
}

/// 解析开放逻辑区域；标准名称与自定义名称使用同一验证边界。
fn region(object: &ObjectValue) -> Result<RegionId, HostErrorDto> {
    RegionId::parse(string_property(object, "region")?).map_err(|error| invalid(error.to_string()))
}

/// 解析组件属性为纯数据 Surface 值。
fn properties(object: &ObjectValue) -> Result<BTreeMap<String, SurfaceValue>, HostErrorDto> {
    let Value::Object(properties) = required_property(object, "properties")? else {
        return Err(invalid("Surface component properties 必须是对象"));
    };
    properties
        .snapshot()
        .into_iter()
        .map(|(name, value)| Ok((name, surface_value(&value)?)))
        .collect()
}

/// 任意 Core 值 → Surface 值（函数/命名空间拒绝）。
fn surface_value(value: &Value) -> Result<SurfaceValue, HostErrorDto> {
    match value {
        Value::Undefined | Value::Null => Ok(SurfaceValue::Null),
        Value::Boolean(value) => Ok(SurfaceValue::Boolean(*value)),
        Value::Number(value) if value.is_finite() => Ok(SurfaceValue::Number(*value)),
        Value::String(value) => value
            .to_unicode_string()
            .map(SurfaceValue::Text)
            .ok_or_else(|| invalid("Component 文本必须是有效 Unicode")),
        Value::Array(values) => values
            .snapshot()
            .iter()
            .map(surface_value)
            .collect::<Result<Vec<_>, _>>()
            .map(SurfaceValue::List),
        Value::Object(values) => values
            .snapshot()
            .iter()
            .map(|(name, value)| Ok((name.clone(), surface_value(value)?)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(SurfaceValue::Map),
        _ => Err(invalid("Component properties 只能包含有限纯数据")),
    }
}

/// 取值对象，非对象报错。
fn object(value: &Value) -> Result<&ObjectValue, HostErrorDto> {
    match value {
        Value::Object(object) => Ok(object),
        _ => Err(invalid("Surface 值必须是对象")),
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
    property(object, name).ok_or_else(|| invalid(format!("Surface 缺少 `{name}`")))
}

/// 取必填 Unicode 字符串属性。
fn string_property(object: &ObjectValue, name: &str) -> Result<String, HostErrorDto> {
    unicode(&required_property(object, name)?)
}

/// 取必填数字属性。
fn number_property(object: &ObjectValue, name: &str) -> Result<f64, HostErrorDto> {
    match required_property(object, name)? {
        Value::Number(value) => Ok(value),
        _ => Err(invalid(format!("Surface `{name}` 必须是数字"))),
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
                    "Surface text delay 必须是 0 到 86400000 的整数毫秒",
                ));
            }
            Ok(Some(milliseconds as u64))
        }
        Some(_) => Err(invalid("Surface text delay 必须是数值毫秒")),
    }
}

/// 读取可选 `heading`（结构性标题级别）：必须为 1 或 2 的整数。
fn heading(object: &ObjectValue) -> Result<Option<HeadingLevel>, HostErrorDto> {
    match property(object, "heading") {
        None | Some(Value::Undefined | Value::Null) => Ok(None),
        Some(Value::Number(level)) => {
            let level: f64 = level;
            if !level.is_finite() || level.fract() != 0.0 || !(1.0..=2.0).contains(&level) {
                return Err(invalid("Surface text heading 必须是 1 或 2 的整数"));
            }
            HeadingLevel::from_u8(level as u8)
                .map(Some)
                .ok_or_else(|| invalid("Surface text heading 必须是 1 或 2 的整数"))
        }
        Some(_) => Err(invalid("Surface text heading 必须是 1 或 2 的整数")),
    }
}

/// 取 Unicode 字符串，非字符串或非 Unicode 报错。
fn unicode(value: &Value) -> Result<String, HostErrorDto> {
    match value {
        Value::String(text) => text
            .to_unicode_string()
            .ok_or_else(|| invalid("Surface 文本必须是有效 Unicode")),
        _ => Err(invalid("Surface 字段必须是文本")),
    }
}

/// Script 输出不重新解析 Twee 标记；换行必须显式使用 `Surface.hardBreak()`。
fn visible_text(text: String) -> Result<TextValue, HostErrorDto> {
    if text.contains("<br>") {
        return Err(invalid(
            "Surface 文本不能包含 `<br>`；请改用 Surface.hardBreak()",
        ));
    }
    Ok(TextValue::from(text))
}

/// 统一 Surface 校验错误。
fn invalid(message: impl Into<String>) -> HostErrorDto {
    HostErrorDto::new("tauri_host.surface", message)
}

#[cfg(test)]
mod tests {
    use crate::surface::SurfaceNode;
    use narrava_loom_core::{
        expression::value::{TextValue, Value},
        semantic::{RegionId, TextColor, TextStyle},
    };

    use super::output;

    fn text(value: &str) -> Value {
        Value::String(TextValue::from(value))
    }

    /// 脚本 builder 值被解析为带 key 的语义节点树。
    #[test]
    fn builder_values_become_keyed_semantic_text_image_and_regions() {
        let value = Value::object(vec![
            ("__narravaSurface".into(), text("region")),
            ("region".into(), text("bar")),
            ("key".into(), text("status")),
            (
                "children".into(),
                Value::array(vec![Value::object(vec![
                    ("__narravaSurface".into(), text("text")),
                    ("text".into(), text("危险")),
                    ("styles".into(), Value::array(vec![text("strong")])),
                    ("color".into(), Value::Number(8.0)),
                ])]),
            ),
        ]);

        let output = output(&value).unwrap().unwrap();

        assert_eq!(output.key(0).unwrap().as_str(), "status");
        let [SurfaceNode::Region { region, content }] = output.nodes() else {
            panic!("应转换 Region");
        };
        assert_eq!(*region, RegionId::bar());
        assert!(matches!(
            content.nodes(),
            [SurfaceNode::StyledText { styles, color: TextColor::RED, .. }]
                if styles == &[TextStyle::Strong]
        ));
    }

    /// 未知样式等 DOM 风格值被拒绝，错误码统一为 `tauri_host.surface`。
    #[test]
    fn builder_values_reject_dom_names_and_unknown_semantics() {
        let value = Value::object(vec![
            ("__narravaSurface".into(), text("text")),
            ("text".into(), text("危险")),
            ("styles".into(), Value::array(vec![text("red")])),
        ]);

        assert_eq!(output(&value).unwrap_err().code, "tauri_host.surface");
    }

    #[test]
    fn builder_accepts_hard_break_and_custom_region() {
        let value = Value::object(vec![
            ("__narravaSurface".into(), text("region")),
            ("region".into(), text("hud")),
            (
                "children".into(),
                Value::array(vec![Value::object(vec![(
                    "__narravaSurface".into(),
                    text("hard-break"),
                )])]),
            ),
        ]);

        let output = output(&value).unwrap().unwrap();
        assert!(matches!(
            output.nodes(),
            [SurfaceNode::Region { region, content }]
                if region.as_str() == "hud"
                    && matches!(content.nodes(), [SurfaceNode::HardBreak])
        ));
    }

    #[test]
    fn builder_rejects_br_markup_instead_of_rescanning_script_text() {
        let value = Value::object(vec![
            ("__narravaSurface".into(), text("text")),
            ("text".into(), text("上一行<br>下一行")),
        ]);

        let error = output(&value).unwrap_err();
        assert_eq!(error.code, "tauri_host.surface");
        assert!(error.message.contains("Surface.hardBreak()"));
    }
}

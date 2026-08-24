//! ECMAScript Presentation builder 值到 Core 语义输出的受验证转换。

use std::collections::BTreeMap;

use narrava_loom_core::{
    expression::value::{ObjectValue, TextValue, Value},
    presentation::{
        ActionRole, ComponentCapability, PresentationAction, PresentationKey, PresentationNode,
        PresentationOutput, PresentationRegion, PresentationValue, TextStyle, TextTone,
    },
    resource::ResourcePath,
};

use crate::HostErrorDto;

const MARKER: &str = "__narravaPresentation";
const MAX_DEPTH: usize = 32;

pub(super) fn output(value: &Value) -> Result<Option<PresentationOutput>, HostErrorDto> {
    let Value::Object(object) = value else {
        return Ok(None);
    };
    if property(object, MARKER).is_none() {
        return Ok(None);
    }
    parse_output(value, 0).map(Some)
}

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
            style => Err(invalid(format!("未知 Presentation text style：{style}"))),
        })
        .collect()
}

fn tone(object: &ObjectValue) -> Result<TextTone, HostErrorDto> {
    match optional_string(object, "tone")?
        .as_deref()
        .unwrap_or("default")
    {
        "default" => Ok(TextTone::Default),
        "muted" => Ok(TextTone::Muted),
        "accent" => Ok(TextTone::Accent),
        "informational" => Ok(TextTone::Informational),
        "positive" => Ok(TextTone::Positive),
        "warning" => Ok(TextTone::Warning),
        "negative" => Ok(TextTone::Negative),
        "critical" => Ok(TextTone::Critical),
        tone => Err(invalid(format!("未知 Presentation text tone：{tone}"))),
    }
}

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

fn object(value: &Value) -> Result<&ObjectValue, HostErrorDto> {
    match value {
        Value::Object(object) => Ok(object),
        _ => Err(invalid("Presentation 值必须是对象")),
    }
}

fn property(object: &ObjectValue, name: &str) -> Option<Value> {
    object
        .snapshot()
        .into_iter()
        .find_map(|(key, value)| (key == name).then_some(value))
}

fn required_property(object: &ObjectValue, name: &str) -> Result<Value, HostErrorDto> {
    property(object, name).ok_or_else(|| invalid(format!("Presentation 缺少 `{name}`")))
}

fn string_property(object: &ObjectValue, name: &str) -> Result<String, HostErrorDto> {
    unicode(&required_property(object, name)?)
}

fn number_property(object: &ObjectValue, name: &str) -> Result<f64, HostErrorDto> {
    match required_property(object, name)? {
        Value::Number(value) => Ok(value),
        _ => Err(invalid(format!("Presentation `{name}` 必须是数字"))),
    }
}

fn optional_string(object: &ObjectValue, name: &str) -> Result<Option<String>, HostErrorDto> {
    match property(object, name) {
        None | Some(Value::Undefined | Value::Null) => Ok(None),
        Some(value) => unicode(&value).map(Some),
    }
}

fn unicode(value: &Value) -> Result<String, HostErrorDto> {
    match value {
        Value::String(text) => text
            .to_unicode_string()
            .ok_or_else(|| invalid("Presentation 文本必须是有效 Unicode")),
        _ => Err(invalid("Presentation 字段必须是文本")),
    }
}

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
                    ("tone".into(), text("critical")),
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
            [PresentationNode::StyledText { styles, tone: TextTone::Critical, .. }]
                if styles == &[TextStyle::Strong]
        ));
    }

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

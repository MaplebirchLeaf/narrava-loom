//! Core Presentation 到 WebView IPC DTO 的纯转换。

use narrava_loom_core::{
    expression::value::TextValue,
    host::HostUpdate,
    presentation::{
        ActionRole, NavigationRole, PresentationAction, PresentationInputKind, PresentationNode,
        PresentationOutput, PresentationRegion, PresentationTarget, PresentationValue, TextStyle,
        TextTone,
    },
};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HostNodeDto {
    Text {
        key: String,
        text: String,
    },
    StyledText {
        key: String,
        text: String,
        styles: Vec<&'static str>,
        tone: &'static str,
    },
    Image {
        key: String,
        resource: String,
        alt: String,
        caption: Option<String>,
    },
    Region {
        key: String,
        region: &'static str,
        nodes: Vec<HostNodeDto>,
    },
    Container {
        key: String,
        nodes: Vec<HostNodeDto>,
    },
    Component {
        key: String,
        capability: String,
        version: u16,
        properties: serde_json::Value,
        fallback: Vec<HostNodeDto>,
    },
    Replace {
        key: String,
        target: HostReplaceTargetDto,
        nodes: Vec<HostNodeDto>,
    },
    Action {
        key: String,
        label: String,
        action: &'static str,
        role: &'static str,
    },
    Checkbox {
        key: String,
        id: String,
        unchecked: serde_json::Value,
        checked: serde_json::Value,
        selected: bool,
    },
    Radiobutton {
        key: String,
        id: String,
        group: String,
        value: serde_json::Value,
        selected: bool,
    },
    Textbox {
        key: String,
        id: String,
        value: String,
    },
    Navigation {
        key: String,
        id: String,
        label: String,
        target: String,
    },
    Button {
        key: String,
        id: String,
        label: String,
        target: String,
    },
    SafeReturn {
        key: String,
        id: String,
        target: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum HostReplaceTargetDto {
    Region(&'static str),
    Key(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HostUpdateDto {
    pub current: String,
    pub nodes: Vec<HostNodeDto>,
}

pub(super) fn convert(update: &HostUpdate) -> HostUpdateDto {
    HostUpdateDto {
        current: update.current().to_owned(),
        nodes: convert_output(
            update.presentation(),
            &format!("passage:{}", update.current()),
        ),
    }
}

fn convert_output(output: &PresentationOutput, scope: &str) -> Vec<HostNodeDto> {
    output
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let key: String = output.key(index).map_or_else(
                || format!("{scope}:{index}:{}", node_kind(node)),
                |key| key.as_str().to_owned(),
            );
            match node {
                PresentationNode::Text(text) => HostNodeDto::Text {
                    key,
                    text: unicode(text),
                },
                PresentationNode::StyledText { text, styles, tone } => HostNodeDto::StyledText {
                    key,
                    text: unicode(text),
                    styles: styles.iter().copied().map(text_style).collect(),
                    tone: text_tone(*tone),
                },
                PresentationNode::Image {
                    resource,
                    alt,
                    caption,
                } => HostNodeDto::Image {
                    key,
                    resource: resource.clone(),
                    alt: unicode(alt),
                    caption: caption.as_ref().map(unicode),
                },
                PresentationNode::Region { region, content } => HostNodeDto::Region {
                    nodes: convert_output(content, &key),
                    key,
                    region: presentation_region(*region),
                },
                PresentationNode::Container { content } => HostNodeDto::Container {
                    nodes: convert_output(content, &key),
                    key,
                },
                PresentationNode::Component {
                    capability,
                    version,
                    properties,
                    fallback,
                } => HostNodeDto::Component {
                    fallback: convert_output(fallback, &key),
                    key,
                    capability: capability.as_str().to_owned(),
                    version: *version,
                    properties: serde_json::Value::Object(
                        properties
                            .iter()
                            .map(|(name, value)| (name.clone(), presentation_value(value)))
                            .collect(),
                    ),
                },
                PresentationNode::Replace { target, content } => HostNodeDto::Replace {
                    key: key.clone(),
                    target: match target {
                        PresentationTarget::Region(region) => {
                            HostReplaceTargetDto::Region(presentation_region(*region))
                        }
                        PresentationTarget::Key(target) => {
                            HostReplaceTargetDto::Key(target.as_str().to_owned())
                        }
                    },
                    nodes: convert_output(content, &key),
                },
                PresentationNode::Action {
                    label,
                    action,
                    role,
                } => HostNodeDto::Action {
                    key,
                    label: unicode(label),
                    action: presentation_action(*action),
                    role: action_role(*role),
                },
                PresentationNode::Input { id, binding } => match &binding.kind {
                    PresentationInputKind::Checkbox {
                        unchecked,
                        checked,
                        selected,
                    } => HostNodeDto::Checkbox {
                        key,
                        id: id.as_str().to_owned(),
                        unchecked: presentation_value(unchecked),
                        checked: presentation_value(checked),
                        selected: *selected,
                    },
                    PresentationInputKind::Radio {
                        group,
                        value,
                        selected,
                    } => HostNodeDto::Radiobutton {
                        key,
                        id: id.as_str().to_owned(),
                        group: group.as_str().to_owned(),
                        value: presentation_value(value),
                        selected: *selected,
                    },
                    PresentationInputKind::Text { value } => HostNodeDto::Textbox {
                        key,
                        id: id.as_str().to_owned(),
                        value: unicode(value),
                    },
                },
                PresentationNode::Navigation {
                    id,
                    label,
                    target,
                    role,
                } => match role {
                    NavigationRole::Link => HostNodeDto::Navigation {
                        key,
                        id: id.as_str().to_owned(),
                        label: unicode(label),
                        target: target.clone(),
                    },
                    NavigationRole::Button => HostNodeDto::Button {
                        key,
                        id: id.as_str().to_owned(),
                        label: unicode(label),
                        target: target.clone(),
                    },
                },
                PresentationNode::SafeReturn { id, target } => HostNodeDto::SafeReturn {
                    key,
                    id: id.as_str().to_owned(),
                    target: target.clone(),
                },
            }
        })
        .collect()
}

fn node_kind(node: &PresentationNode) -> &'static str {
    match node {
        PresentationNode::Text(_) => "text",
        PresentationNode::StyledText { .. } => "styled-text",
        PresentationNode::Image { .. } => "image",
        PresentationNode::Region { .. } => "region",
        PresentationNode::Container { .. } => "container",
        PresentationNode::Component { .. } => "component",
        PresentationNode::Replace { .. } => "replace",
        PresentationNode::Action { .. } => "action",
        PresentationNode::Input { binding, .. } => match binding.kind {
            PresentationInputKind::Checkbox { .. } => "checkbox",
            PresentationInputKind::Radio { .. } => "radiobutton",
            PresentationInputKind::Text { .. } => "textbox",
        },
        PresentationNode::Navigation { .. } => "navigation",
        PresentationNode::SafeReturn { .. } => "safe-return",
    }
}

fn presentation_action(action: PresentationAction) -> &'static str {
    match action {
        PresentationAction::Dismiss => "dismiss",
    }
}

fn action_role(role: ActionRole) -> &'static str {
    match role {
        ActionRole::Default => "default",
        ActionRole::Primary => "primary",
        ActionRole::Secondary => "secondary",
        ActionRole::Danger => "danger",
    }
}

fn presentation_value(value: &PresentationValue) -> serde_json::Value {
    match value {
        PresentationValue::Null => serde_json::Value::Null,
        PresentationValue::Boolean(value) => serde_json::Value::Bool(*value),
        PresentationValue::Number(value) => serde_json::Number::from_f64(*value)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        PresentationValue::Text(value) => serde_json::Value::String(value.clone()),
        PresentationValue::List(values) => {
            serde_json::Value::Array(values.iter().map(presentation_value).collect())
        }
        PresentationValue::Map(values) => serde_json::Value::Object(
            values
                .iter()
                .map(|(name, value)| (name.clone(), presentation_value(value)))
                .collect(),
        ),
    }
}

fn presentation_region(region: PresentationRegion) -> &'static str {
    match region {
        PresentationRegion::Header => "header",
        PresentationRegion::Main => "main",
        PresentationRegion::Footer => "footer",
        PresentationRegion::Bar => "bar",
        PresentationRegion::BarStowed => "bar-stowed",
        PresentationRegion::Dialog => "dialog",
    }
}

fn text_style(style: TextStyle) -> &'static str {
    match style {
        TextStyle::Emphasis => "emphasis",
        TextStyle::Strong => "strong",
        TextStyle::Code => "code",
        TextStyle::Deleted => "deleted",
        TextStyle::Inserted => "inserted",
        TextStyle::Marked => "marked",
        TextStyle::Small => "small",
        TextStyle::Subscript => "subscript",
        TextStyle::Superscript => "superscript",
        TextStyle::Quote => "quote",
        TextStyle::Heading1 => "heading1",
        TextStyle::Heading2 => "heading2",
        TextStyle::Heading3 => "heading3",
        TextStyle::Heading4 => "heading4",
        TextStyle::Heading5 => "heading5",
        TextStyle::Heading6 => "heading6",
    }
}

fn text_tone(tone: TextTone) -> &'static str {
    match tone {
        TextTone::Default => "default",
        TextTone::Muted => "muted",
        TextTone::Accent => "accent",
        TextTone::Informational => "informational",
        TextTone::Positive => "positive",
        TextTone::Warning => "warning",
        TextTone::Negative => "negative",
        TextTone::Critical => "critical",
    }
}

fn unicode(text: &TextValue) -> String {
    text.to_unicode_string()
        .unwrap_or_else(|| String::from("<非 Unicode 文本>"))
}

//! Core Surface 到 WebView IPC DTO 的纯转换。
//!
//! 语义与视觉分离：DTO 只携带语义字段（color 为 0..=63 数字、styles 为 8 个语义
//! 字形名），颜色/字形外观完全由 WebView 按语义自行决定。

use narrava_loom_core::{
    expression::value::TextValue,
    host::HostUpdate,
    semantic::{
        ActionRole, HeadingLevel, NavigationRole, SemanticAction, SemanticInputKind, SemanticNode,
        SemanticOutput, SemanticTarget, SemanticValue, TextStyle,
    },
};
use narrava_loom_protocol::{ContainerFlowDto, ContainerPresentationDto};
pub use narrava_loom_protocol::{HostNodeDto, HostReplaceTargetDto, HostUpdateDto};

/// 把 Core 的 HostUpdate 转换为 IPC DTO。
pub fn encode_host_update(update: &HostUpdate, can_back: bool, can_forward: bool) -> HostUpdateDto {
    HostUpdateDto {
        current: update.current().to_owned(),
        nodes: convert_output(update.surface(), &format!("passage:{}", update.current())),
        can_back,
        can_forward,
    }
}

/// 递归转换输出树；无 key 的节点用 `scope:index:kind` 生成稳定 key。
fn convert_output(output: &SemanticOutput, scope: &str) -> Vec<HostNodeDto> {
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
                SemanticNode::Text(text) => HostNodeDto::Text {
                    key,
                    text: unicode(text),
                },
                SemanticNode::HardBreak => HostNodeDto::HardBreak { key },
                SemanticNode::StyledText {
                    text,
                    styles,
                    color,
                    delay,
                    heading,
                } => HostNodeDto::StyledText {
                    key,
                    text: unicode(text),
                    styles: styles
                        .iter()
                        .copied()
                        .map(text_style)
                        .map(str::to_owned)
                        .collect(),
                    color: color.index(),
                    delay: *delay,
                    heading: heading.map(HeadingLevel::level),
                },
                SemanticNode::Image {
                    resource,
                    alt,
                    caption,
                } => HostNodeDto::Image {
                    key,
                    resource: resource.clone(),
                    alt: unicode(alt),
                    caption: caption.as_ref().map(unicode),
                },
                SemanticNode::Region { region, content } => HostNodeDto::Region {
                    nodes: convert_output(content, &key),
                    key,
                    region: region.as_str().to_owned(),
                },
                SemanticNode::Container {
                    presentation,
                    flow,
                    content,
                } => HostNodeDto::Container {
                    nodes: convert_output(content, &key),
                    key,
                    presentation: match presentation {
                        narrava_loom_core::semantic::ContainerPresentation::Plain => {
                            ContainerPresentationDto::Plain
                        }
                        narrava_loom_core::semantic::ContainerPresentation::Panel => {
                            ContainerPresentationDto::Panel
                        }
                    },
                    flow: match flow {
                        narrava_loom_core::semantic::ContainerFlow::Stack => {
                            ContainerFlowDto::Stack
                        }
                        narrava_loom_core::semantic::ContainerFlow::Row => ContainerFlowDto::Row,
                    },
                },
                SemanticNode::Component {
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
                            .map(|(name, value)| (name.clone(), surface_value(value)))
                            .collect(),
                    ),
                },
                SemanticNode::Replace { target, content } => HostNodeDto::Replace {
                    key: key.clone(),
                    target: match target {
                        SemanticTarget::Region(region) => {
                            HostReplaceTargetDto::Region(region.as_str().to_owned())
                        }
                        SemanticTarget::Key(target) => {
                            HostReplaceTargetDto::Key(target.as_str().to_owned())
                        }
                    },
                    nodes: convert_output(content, &key),
                },
                SemanticNode::Action {
                    label,
                    action,
                    role,
                } => HostNodeDto::Action {
                    key,
                    label: unicode(label),
                    action: surface_action(*action).to_owned(),
                    role: action_role(*role).to_owned(),
                },
                SemanticNode::Input { id, binding } => match &binding.kind {
                    SemanticInputKind::Checkbox {
                        unchecked,
                        checked,
                        selected,
                    } => HostNodeDto::Checkbox {
                        key,
                        id: id.as_str().to_owned(),
                        unchecked: surface_value(unchecked),
                        checked: surface_value(checked),
                        selected: *selected,
                    },
                    SemanticInputKind::Radio {
                        group,
                        value,
                        selected,
                    } => HostNodeDto::Radiobutton {
                        key,
                        id: id.as_str().to_owned(),
                        group: group.as_str().to_owned(),
                        value: surface_value(value),
                        selected: *selected,
                    },
                    SemanticInputKind::Text { value } => HostNodeDto::Textbox {
                        key,
                        id: id.as_str().to_owned(),
                        value: unicode(value),
                    },
                },
                SemanticNode::Navigation {
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
                SemanticNode::SafeReturn { id, target } => HostNodeDto::SafeReturn {
                    key,
                    id: id.as_str().to_owned(),
                    target: target.clone(),
                },
            }
        })
        .collect()
}

/// 节点种类名（用于生成匿名节点 key）。
fn node_kind(node: &SemanticNode) -> &'static str {
    match node {
        SemanticNode::Text(_) => "text",
        SemanticNode::HardBreak => "hard-break",
        SemanticNode::StyledText { .. } => "styled-text",
        SemanticNode::Image { .. } => "image",
        SemanticNode::Region { .. } => "region",
        SemanticNode::Container { .. } => "container",
        SemanticNode::Component { .. } => "component",
        SemanticNode::Replace { .. } => "replace",
        SemanticNode::Action { .. } => "action",
        SemanticNode::Input { binding, .. } => match binding.kind {
            SemanticInputKind::Checkbox { .. } => "checkbox",
            SemanticInputKind::Radio { .. } => "radiobutton",
            SemanticInputKind::Text { .. } => "textbox",
        },
        SemanticNode::Navigation { .. } => "navigation",
        SemanticNode::SafeReturn { .. } => "safe-return",
    }
}

/// 动作枚举 → IPC 名。
fn surface_action(action: SemanticAction) -> &'static str {
    match action {
        SemanticAction::Dismiss => "dismiss",
    }
}

/// 动作角色枚举 → IPC 名。
fn action_role(role: ActionRole) -> &'static str {
    match role {
        ActionRole::Default => "default",
        ActionRole::Primary => "primary",
        ActionRole::Secondary => "secondary",
        ActionRole::Danger => "danger",
    }
}

/// Surface 值 → JSON（用于组件属性与控件取值）。
fn surface_value(value: &SemanticValue) -> serde_json::Value {
    match value {
        SemanticValue::Null => serde_json::Value::Null,
        SemanticValue::Boolean(value) => serde_json::Value::Bool(*value),
        SemanticValue::Number(value) => serde_json::Number::from_f64(*value)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        SemanticValue::Text(value) => serde_json::Value::String(value.clone()),
        SemanticValue::List(values) => {
            serde_json::Value::Array(values.iter().map(surface_value).collect())
        }
        SemanticValue::Map(values) => serde_json::Value::Object(
            values
                .iter()
                .map(|(name, value)| (name.clone(), surface_value(value)))
                .collect(),
        ),
    }
}

/// 语义字形枚举 → IPC 字形名（emphasis/strong/code/quote/marked/small/inserted/deleted）。
fn text_style(style: TextStyle) -> &'static str {
    match style {
        TextStyle::Emphasis => "emphasis",
        TextStyle::Strong => "strong",
        TextStyle::Code => "code",
        TextStyle::Marked => "marked",
        TextStyle::Small => "small",
        TextStyle::Inserted => "inserted",
        TextStyle::Deleted => "deleted",
        TextStyle::Quote => "quote",
    }
}

/// TextValue 转 Unicode 字符串；非 Unicode 文本给占位。
fn unicode(text: &TextValue) -> String {
    text.to_unicode_string()
        .unwrap_or_else(|| String::from("<非 Unicode 文本>"))
}

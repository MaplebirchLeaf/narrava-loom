//! Core Surface 到 WebView IPC DTO 的纯转换。
//!
//! 语义与视觉分离：DTO 只携带语义字段（color 为 0..=63 数字、styles 为 8 个语义
//! 字形名），颜色/字形外观完全由 WebView 按语义自行决定。

use narrava_loom_core::{
    expression::value::TextValue,
    host::HostUpdate,
    semantic::{ActionRole, HeadingLevel, NavigationRole, TextStyle},
};
pub use narrava_loom_protocol::{HostNodeDto, HostReplaceTargetDto, HostUpdateDto};

use super::surface::{
    Surface, SurfaceAction, SurfaceInputKind, SurfaceNode, SurfaceTarget, SurfaceValue,
};

/// 把 Core 的 HostUpdate 转换为 IPC DTO。
pub fn convert(update: &HostUpdate) -> HostUpdateDto {
    // Core 执行输出（semantic）经同构转换成为 Host 消费的 Surface 协议表示。
    let surface = Surface::from(update.surface());
    HostUpdateDto {
        current: update.current().to_owned(),
        nodes: convert_output(&surface, &format!("passage:{}", update.current())),
    }
}

/// 递归转换输出树；无 key 的节点用 `scope:index:kind` 生成稳定 key。
fn convert_output(output: &Surface, scope: &str) -> Vec<HostNodeDto> {
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
                SurfaceNode::Text(text) => HostNodeDto::Text {
                    key,
                    text: unicode(text),
                },
                SurfaceNode::HardBreak => HostNodeDto::HardBreak { key },
                SurfaceNode::StyledText {
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
                SurfaceNode::Image {
                    resource,
                    alt,
                    caption,
                } => HostNodeDto::Image {
                    key,
                    resource: resource.clone(),
                    alt: unicode(alt),
                    caption: caption.as_ref().map(unicode),
                },
                SurfaceNode::Region { region, content } => HostNodeDto::Region {
                    nodes: convert_output(content, &key),
                    key,
                    region: region.as_str().to_owned(),
                },
                SurfaceNode::Container { content } => HostNodeDto::Container {
                    nodes: convert_output(content, &key),
                    key,
                },
                SurfaceNode::Component {
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
                SurfaceNode::Replace { target, content } => HostNodeDto::Replace {
                    key: key.clone(),
                    target: match target {
                        SurfaceTarget::Region(region) => {
                            HostReplaceTargetDto::Region(region.as_str().to_owned())
                        }
                        SurfaceTarget::Key(target) => {
                            HostReplaceTargetDto::Key(target.as_str().to_owned())
                        }
                    },
                    nodes: convert_output(content, &key),
                },
                SurfaceNode::Action {
                    label,
                    action,
                    role,
                } => HostNodeDto::Action {
                    key,
                    label: unicode(label),
                    action: surface_action(*action).to_owned(),
                    role: action_role(*role).to_owned(),
                },
                SurfaceNode::Input { id, binding } => match &binding.kind {
                    SurfaceInputKind::Checkbox {
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
                    SurfaceInputKind::Radio {
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
                    SurfaceInputKind::Text { value } => HostNodeDto::Textbox {
                        key,
                        id: id.as_str().to_owned(),
                        value: unicode(value),
                    },
                },
                SurfaceNode::Navigation {
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
                SurfaceNode::SafeReturn { id, target } => HostNodeDto::SafeReturn {
                    key,
                    id: id.as_str().to_owned(),
                    target: target.clone(),
                },
            }
        })
        .collect()
}

/// 节点种类名（用于生成匿名节点 key）。
fn node_kind(node: &SurfaceNode) -> &'static str {
    match node {
        SurfaceNode::Text(_) => "text",
        SurfaceNode::HardBreak => "hard-break",
        SurfaceNode::StyledText { .. } => "styled-text",
        SurfaceNode::Image { .. } => "image",
        SurfaceNode::Region { .. } => "region",
        SurfaceNode::Container { .. } => "container",
        SurfaceNode::Component { .. } => "component",
        SurfaceNode::Replace { .. } => "replace",
        SurfaceNode::Action { .. } => "action",
        SurfaceNode::Input { binding, .. } => match binding.kind {
            SurfaceInputKind::Checkbox { .. } => "checkbox",
            SurfaceInputKind::Radio { .. } => "radiobutton",
            SurfaceInputKind::Text { .. } => "textbox",
        },
        SurfaceNode::Navigation { .. } => "navigation",
        SurfaceNode::SafeReturn { .. } => "safe-return",
    }
}

/// 动作枚举 → IPC 名。
fn surface_action(action: SurfaceAction) -> &'static str {
    match action {
        SurfaceAction::Dismiss => "dismiss",
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
fn surface_value(value: &SurfaceValue) -> serde_json::Value {
    match value {
        SurfaceValue::Null => serde_json::Value::Null,
        SurfaceValue::Boolean(value) => serde_json::Value::Bool(*value),
        SurfaceValue::Number(value) => serde_json::Number::from_f64(*value)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        SurfaceValue::Text(value) => serde_json::Value::String(value.clone()),
        SurfaceValue::List(values) => {
            serde_json::Value::Array(values.iter().map(surface_value).collect())
        }
        SurfaceValue::Map(values) => serde_json::Value::Object(
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

//! Core Surface 到 WebView IPC DTO 的纯转换。
//!
//! 语义与视觉分离：DTO 只携带语义字段（color 为 0..=63 数字、styles 为 8 个语义
//! 字形名），颜色/字形外观完全由 WebView 按语义自行决定。

use narrava_loom_core::{
    expression::value::TextValue,
    host::HostUpdate,
    protocol::{
        ActionRole, HeadingLevel, NavigationRole, Surface, SurfaceAction, SurfaceInputKind,
        SurfaceNode, SurfaceTarget, SurfaceValue, TextStyle,
    },
};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HostNodeDto {
    /// 纯文本。
    Text { key: String, text: String },
    /// 作者显式硬换行。
    HardBreak { key: String },
    /// 带语义样式与标准 color 的文本（`delay` 只规定到期前不可见，`heading` 为结构性标题级别）。
    StyledText {
        key: String,
        text: String,
        styles: Vec<&'static str>,
        color: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        delay: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        heading: Option<u8>,
    },
    /// 图像（仅逻辑路径与替代文本，字节走 Resource 协议）。
    Image {
        key: String,
        resource: String,
        alt: String,
        caption: Option<String>,
    },
    /// 具名区域（header/main/footer/bar/bar-stowed/dialog）。
    Region {
        key: String,
        region: String,
        nodes: Vec<HostNodeDto>,
    },
    /// 按 key 可被 Replace 定位的容器。
    Container {
        key: String,
        nodes: Vec<HostNodeDto>,
    },
    /// 由宿主能力渲染的组件（properties 为纯数据，fallback 供不支持时降级）。
    Component {
        key: String,
        capability: String,
        version: u16,
        properties: serde_json::Value,
        fallback: Vec<HostNodeDto>,
    },
    /// 用新内容替换既有容器/区域。
    Replace {
        key: String,
        target: HostReplaceTargetDto,
        nodes: Vec<HostNodeDto>,
    },
    /// 纯客户端动作（如 dismiss）。
    Action {
        key: String,
        label: String,
        action: &'static str,
        role: &'static str,
    },
    /// 复选框输入控件。
    Checkbox {
        key: String,
        id: String,
        unchecked: serde_json::Value,
        checked: serde_json::Value,
        selected: bool,
    },
    /// 单选输入控件（同组互斥）。
    Radiobutton {
        key: String,
        id: String,
        group: String,
        value: serde_json::Value,
        selected: bool,
    },
    /// 文本框输入控件。
    Textbox {
        key: String,
        id: String,
        value: String,
    },
    /// 链接式导航（link 角色）。
    Navigation {
        key: String,
        id: String,
        label: String,
        target: String,
    },
    /// 按钮式导航（button 角色）。
    Button {
        key: String,
        id: String,
        label: String,
        target: String,
    },
    /// 安全返回上一语境。
    SafeReturn {
        key: String,
        id: String,
        target: String,
    },
}

/// Replace 的目标：整个区域或具名容器 key。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum HostReplaceTargetDto {
    /// 替换整个区域。
    Region(String),
    /// 替换指定 key 的容器。
    Key(String),
}

/// 一次事务产生的语义更新：当前 Passage 与节点树。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HostUpdateDto {
    /// 当前 Passage 名。
    pub current: String,
    /// 语义节点列表（按渲染顺序）。
    pub nodes: Vec<HostNodeDto>,
}

/// 把 Core 的 HostUpdate 转换为 IPC DTO。
pub fn convert(update: &HostUpdate) -> HostUpdateDto {
    HostUpdateDto {
        current: update.current().to_owned(),
        nodes: convert_output(update.surface(), &format!("passage:{}", update.current())),
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
                    styles: styles.iter().copied().map(text_style).collect(),
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
                    action: surface_action(*action),
                    role: action_role(*role),
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

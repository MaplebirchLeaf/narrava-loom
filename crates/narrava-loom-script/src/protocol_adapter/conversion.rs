//! Script Surface 到 Core 语义输出的单向转换。
//!
//! Surface 只承接脚本 builder 的不可信输入。Core 输出会直接转换为 Protocol DTO，
//! 不再为了渲染复制成一棵同构 Surface。

use narrava_loom_core::semantic::{
    SemanticInputBinding, SemanticInputKind, SemanticNode, SemanticOutput, SemanticTarget,
    SemanticValue,
};

use super::surface::{
    Surface, SurfaceAction, SurfaceInputBinding, SurfaceInputKind, SurfaceNode, SurfaceTarget,
    SurfaceValue,
};

/// 把 Host 脚本 bridge 产生的 Surface 协议表示转回 Core 语义输出（宏执行路径）。
impl From<&Surface> for SemanticOutput {
    fn from(surface: &Surface) -> Self {
        let mut output = SemanticOutput::from_nodes(Vec::new());
        for (index, node) in surface.nodes().iter().enumerate() {
            match surface.key(index) {
                Some(key) => output
                    .push_keyed(key.clone(), SemanticNode::from(node))
                    .expect("Surface 已保证 key 唯一"),
                None => output.push(SemanticNode::from(node)),
            }
        }
        output
    }
}

impl From<&SurfaceNode> for SemanticNode {
    fn from(node: &SurfaceNode) -> Self {
        match node {
            SurfaceNode::Text(text) => SemanticNode::Text(text.clone()),
            SurfaceNode::HardBreak => SemanticNode::HardBreak,
            SurfaceNode::StyledText {
                text,
                styles,
                color,
                delay,
                heading,
            } => SemanticNode::StyledText {
                text: text.clone(),
                styles: styles.clone(),
                color: *color,
                delay: *delay,
                heading: *heading,
            },
            SurfaceNode::Image {
                resource,
                alt,
                caption,
            } => SemanticNode::Image {
                resource: resource.clone(),
                alt: alt.clone(),
                caption: caption.clone(),
            },
            SurfaceNode::Region { region, content } => SemanticNode::Region {
                region: region.clone(),
                content: SemanticOutput::from(content),
            },
            SurfaceNode::Container {
                presentation,
                flow,
                content,
            } => SemanticNode::Container {
                presentation: *presentation,
                flow: *flow,
                content: SemanticOutput::from(content),
            },
            SurfaceNode::Component {
                capability,
                version,
                properties,
                fallback,
            } => SemanticNode::Component {
                capability: capability.clone(),
                version: *version,
                properties: properties
                    .iter()
                    .map(|(key, value)| (key.clone(), SemanticValue::from(value)))
                    .collect(),
                fallback: SemanticOutput::from(fallback),
            },
            SurfaceNode::Replace { target, content } => SemanticNode::Replace {
                target: SemanticTarget::from(target),
                content: SemanticOutput::from(content),
            },
            SurfaceNode::Action {
                label,
                action,
                role,
            } => SemanticNode::Action {
                label: label.clone(),
                action: match action {
                    SurfaceAction::Dismiss => narrava_loom_core::semantic::SemanticAction::Dismiss,
                },
                role: *role,
            },
            SurfaceNode::Input { id, binding } => SemanticNode::Input {
                id: id.clone(),
                binding: SemanticInputBinding::from(binding),
            },
            SurfaceNode::Navigation {
                id,
                label,
                target,
                role,
            } => SemanticNode::Navigation {
                id: id.clone(),
                label: label.clone(),
                target: target.clone(),
                role: *role,
            },
            SurfaceNode::SafeReturn { id, target } => SemanticNode::SafeReturn {
                id: id.clone(),
                target: target.clone(),
            },
        }
    }
}

impl From<&SurfaceTarget> for SemanticTarget {
    fn from(target: &SurfaceTarget) -> Self {
        match target {
            SurfaceTarget::Region(region) => SemanticTarget::Region(region.clone()),
            SurfaceTarget::Key(key) => SemanticTarget::Key(key.clone()),
        }
    }
}

impl From<&SurfaceInputBinding> for SemanticInputBinding {
    fn from(binding: &SurfaceInputBinding) -> Self {
        SemanticInputBinding {
            receiver: binding.receiver.clone(),
            kind: SemanticInputKind::from(&binding.kind),
        }
    }
}

impl From<&SurfaceInputKind> for SemanticInputKind {
    fn from(kind: &SurfaceInputKind) -> Self {
        match kind {
            SurfaceInputKind::Checkbox {
                unchecked,
                checked,
                selected,
            } => SemanticInputKind::Checkbox {
                unchecked: SemanticValue::from(unchecked),
                checked: SemanticValue::from(checked),
                selected: *selected,
            },
            SurfaceInputKind::Radio {
                group,
                value,
                selected,
            } => SemanticInputKind::Radio {
                group: group.clone(),
                value: SemanticValue::from(value),
                selected: *selected,
            },
            SurfaceInputKind::Text { value } => SemanticInputKind::Text {
                value: value.clone(),
            },
        }
    }
}

/// Surface 值 → 语义值（递归）。
impl From<&SurfaceValue> for SemanticValue {
    fn from(value: &SurfaceValue) -> Self {
        match value {
            SurfaceValue::Null => SemanticValue::Null,
            SurfaceValue::Boolean(value) => SemanticValue::Boolean(*value),
            SurfaceValue::Number(value) => SemanticValue::Number(*value),
            SurfaceValue::Text(value) => SemanticValue::Text(value.clone()),
            SurfaceValue::List(values) => {
                SemanticValue::List(values.iter().map(SemanticValue::from).collect())
            }
            SurfaceValue::Map(values) => SemanticValue::Map(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), SemanticValue::from(value)))
                    .collect(),
            ),
        }
    }
}

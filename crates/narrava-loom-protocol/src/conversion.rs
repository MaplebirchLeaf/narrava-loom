//! Core 内部语义输出到 Surface 协议表示的转换。
//!
//! Core 执行产生 `narrava_loom_core::semantic::SemanticOutput`，Host 渲染/DTO 层消费
//! `Surface`。转换是结构同构的逐节点映射：共用语义原子（`TextStyle`/`TextColor`/
//! `InteractionId` 等）直接复用，只有 `SemanticValue` 与容器节点需要递归转换。

use narrava_loom_core::semantic::{
    SemanticAction, SemanticInputBinding, SemanticInputKind, SemanticKey, SemanticNode,
    SemanticOutput, SemanticTarget, SemanticValue,
};

use crate::surface::{
    Surface, SurfaceAction, SurfaceInputBinding, SurfaceInputKind, SurfaceKey, SurfaceNode,
    SurfaceTarget, SurfaceValue,
};

/// 把 Core 内部语义输出转换为 Host 消费的 Surface 协议表示。
impl From<&SemanticOutput> for Surface {
    fn from(output: &SemanticOutput) -> Self {
        let mut surface = Surface::from_nodes(Vec::new());
        for (index, node) in output.nodes().iter().enumerate() {
            match output.key(index) {
                Some(key) => surface
                    .push_keyed(SurfaceKey::from(key), SurfaceNode::from(node))
                    .expect("Core 语义输出已保证 key 唯一"),
                None => surface.push(SurfaceNode::from(node)),
            }
        }
        surface
    }
}

impl From<&SemanticKey> for SurfaceKey {
    fn from(key: &SemanticKey) -> Self {
        SurfaceKey::parse(key.as_str()).expect("Core 语义 key 已通过同一校验")
    }
}

/// 语义节点 → Surface 节点：共用原子直接复用，`SemanticValue` 与容器递归转换。
impl From<&SemanticNode> for SurfaceNode {
    fn from(node: &SemanticNode) -> Self {
        match node {
            SemanticNode::Text(text) => SurfaceNode::Text(text.clone()),
            SemanticNode::HardBreak => SurfaceNode::HardBreak,
            SemanticNode::StyledText {
                text,
                styles,
                color,
                delay,
                heading,
            } => SurfaceNode::StyledText {
                text: text.clone(),
                styles: styles.clone(),
                color: *color,
                delay: *delay,
                heading: *heading,
            },
            SemanticNode::Image {
                resource,
                alt,
                caption,
            } => SurfaceNode::Image {
                resource: resource.clone(),
                alt: alt.clone(),
                caption: caption.clone(),
            },
            SemanticNode::Region { region, content } => SurfaceNode::Region {
                region: region.clone(),
                content: Surface::from(content),
            },
            SemanticNode::Container { content } => SurfaceNode::Container {
                content: Surface::from(content),
            },
            SemanticNode::Component {
                capability,
                version,
                properties,
                fallback,
            } => SurfaceNode::Component {
                capability: capability.clone(),
                version: *version,
                properties: properties
                    .iter()
                    .map(|(key, value)| (key.clone(), SurfaceValue::from(value)))
                    .collect(),
                fallback: Surface::from(fallback),
            },
            SemanticNode::Replace { target, content } => SurfaceNode::Replace {
                target: SurfaceTarget::from(target),
                content: Surface::from(content),
            },
            SemanticNode::Action {
                label,
                action,
                role,
            } => SurfaceNode::Action {
                label: label.clone(),
                action: SurfaceAction::from(action),
                role: *role,
            },
            SemanticNode::Input { id, binding } => SurfaceNode::Input {
                id: id.clone(),
                binding: SurfaceInputBinding::from(binding),
            },
            SemanticNode::Navigation {
                id,
                label,
                target,
                role,
            } => SurfaceNode::Navigation {
                id: id.clone(),
                label: label.clone(),
                target: target.clone(),
                role: *role,
            },
            SemanticNode::SafeReturn { id, target } => SurfaceNode::SafeReturn {
                id: id.clone(),
                target: target.clone(),
            },
        }
    }
}

impl From<&SemanticAction> for SurfaceAction {
    fn from(action: &SemanticAction) -> Self {
        match action {
            SemanticAction::Dismiss => SurfaceAction::Dismiss,
        }
    }
}

impl From<&SemanticTarget> for SurfaceTarget {
    fn from(target: &SemanticTarget) -> Self {
        match target {
            SemanticTarget::Region(region) => SurfaceTarget::Region(region.clone()),
            SemanticTarget::Key(key) => SurfaceTarget::Key(SurfaceKey::from(key)),
        }
    }
}

impl From<&SemanticInputBinding> for SurfaceInputBinding {
    fn from(binding: &SemanticInputBinding) -> Self {
        SurfaceInputBinding {
            receiver: binding.receiver.clone(),
            kind: SurfaceInputKind::from(&binding.kind),
        }
    }
}

impl From<&SemanticInputKind> for SurfaceInputKind {
    fn from(kind: &SemanticInputKind) -> Self {
        match kind {
            SemanticInputKind::Checkbox {
                unchecked,
                checked,
                selected,
            } => SurfaceInputKind::Checkbox {
                unchecked: SurfaceValue::from(unchecked),
                checked: SurfaceValue::from(checked),
                selected: *selected,
            },
            SemanticInputKind::Radio {
                group,
                value,
                selected,
            } => SurfaceInputKind::Radio {
                group: group.clone(),
                value: SurfaceValue::from(value),
                selected: *selected,
            },
            SemanticInputKind::Text { value } => SurfaceInputKind::Text {
                value: value.clone(),
            },
        }
    }
}

/// 语义值 → Surface 值（递归）。
impl From<&SemanticValue> for SurfaceValue {
    fn from(value: &SemanticValue) -> Self {
        match value {
            SemanticValue::Null => SurfaceValue::Null,
            SemanticValue::Boolean(value) => SurfaceValue::Boolean(*value),
            SemanticValue::Number(value) => SurfaceValue::Number(*value),
            SemanticValue::Text(value) => SurfaceValue::Text(value.clone()),
            SemanticValue::List(values) => {
                SurfaceValue::List(values.iter().map(SurfaceValue::from).collect())
            }
            SemanticValue::Map(values) => SurfaceValue::Map(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), SurfaceValue::from(value)))
                    .collect(),
            ),
        }
    }
}

/// 把 Host 脚本 bridge 产生的 Surface 协议表示转回 Core 语义输出（宏执行路径）。
impl From<&Surface> for SemanticOutput {
    fn from(surface: &Surface) -> Self {
        let mut output = SemanticOutput::from_nodes(Vec::new());
        for (index, node) in surface.nodes().iter().enumerate() {
            match surface.key(index) {
                Some(key) => output
                    .push_keyed(
                        SemanticKey::parse(key.as_str()).expect("Surface key 已通过同一校验"),
                        SemanticNode::from(node),
                    )
                    .expect("Surface 已保证 key 唯一"),
                None => output.push(SemanticNode::from(node)),
            }
        }
        output
    }
}

impl From<&SurfaceKey> for SemanticKey {
    fn from(key: &SurfaceKey) -> Self {
        SemanticKey::parse(key.as_str()).expect("Surface key 已通过同一校验")
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
            SurfaceNode::Container { content } => SemanticNode::Container {
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
            SurfaceTarget::Key(key) => SemanticTarget::Key(SemanticKey::from(key)),
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

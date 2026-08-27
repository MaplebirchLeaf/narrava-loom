//! 跨 Host 的 Surface 协议语义：Core 执行输出的宿主无关表示。
//!
//! Core 执行产生 `semantic::SemanticOutput`（内部语义），本模块定义 Host 消费的
//! `Surface` 协议版本，并依赖 Core 的共用语义原子（`TextStyle`/`TextColor`/
//! `InteractionId` 等）。转换见 [`convert`](crate::convert)。

use std::{collections::BTreeMap, error::Error, fmt};

use narrava_loom_core::{
    expression::value::TextValue,
    semantic::{
        ActionRole, ComponentCapability, HeadingLevel, InputGroupId, InteractionId, NavigationRole,
        RegionId, TextColor, TextStyle,
    },
};

/// Renderer 跨更新复用语义节点的稳定身份。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceKey(String);

/// SurfaceKey 在公开边界的验证错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceKeyError {
    Empty,
    Duplicate(String),
}

impl fmt::Display for SurfaceKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("Surface key 不能为空"),
            Self::Duplicate(key) => write!(formatter, "Surface key 重复：{key}"),
        }
    }
}

impl Error for SurfaceKeyError {}

impl SurfaceKey {
    /// 校验并构造 key；空字符串被拒绝。
    pub fn parse(key: impl Into<String>) -> Result<Self, SurfaceKeyError> {
        let key: String = key.into();
        if key.is_empty() {
            return Err(SurfaceKeyError::Empty);
        }
        Ok(Self(key))
    }

    /// 稳定字符串表示，供 Host 跨更新引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `replace` 可跨 Host 解析的目标；不包含 CSS selector 或终端坐标。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceTarget {
    Region(RegionId),
    Key(SurfaceKey),
}

/// 不进入 Story 导航的宿主级动作。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceAction {
    /// 关闭当前对话层，不触发 Story 导航。
    Dismiss,
}

/// 状态绑定输入控件的宿主无关种类。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceInputKind {
    Checkbox {
        unchecked: SurfaceValue,
        checked: SurfaceValue,
        selected: bool,
    },
    Radio {
        group: InputGroupId,
        value: SurfaceValue,
        selected: bool,
    },
    Text {
        value: TextValue,
    },
}

/// Core 保留的输入写回契约；receiver 不会发送给 WebView。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceInputBinding {
    pub receiver: String,
    pub kind: SurfaceInputKind,
}

impl SurfaceInputBinding {
    /// 校验 Host 回送值是否属于控件公开的可提交值集合。
    pub fn accepts(&self, value: &SurfaceValue) -> bool {
        match &self.kind {
            SurfaceInputKind::Checkbox {
                unchecked, checked, ..
            } => value == unchecked || value == checked,
            SurfaceInputKind::Radio {
                value: expected, ..
            } => value == expected,
            SurfaceInputKind::Text { .. } => matches!(value, SurfaceValue::Text(_)),
        }
    }
}

/// Component 属性使用的纯数据，不携带脚本函数或平台对象。
#[derive(Clone, Debug)]
pub enum SurfaceValue {
    Null,
    Boolean(bool),
    Number(f64),
    Text(String),
    List(Vec<SurfaceValue>),
    Map(BTreeMap<String, SurfaceValue>),
}

impl PartialEq for SurfaceValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Boolean(left), Self::Boolean(right)) => left == right,
            (Self::Number(left), Self::Number(right)) => left.to_bits() == right.to_bits(),
            (Self::Text(left), Self::Text(right)) => left == right,
            (Self::List(left), Self::List(right)) => left == right,
            (Self::Map(left), Self::Map(right)) => left == right,
            _ => false,
        }
    }
}

impl Eq for SurfaceValue {}

/// 当前已实现的最小语义节点；后续类型只按稳定跨宿主语义扩展。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceNode {
    Text(TextValue),
    /// 作者显式写出的硬换行；Host 映射到对应平台的换行能力。
    HardBreak,
    StyledText {
        text: TextValue,
        styles: Vec<TextStyle>,
        color: TextColor,
        /// 到达指定毫秒数前内容不可见；动画方式完全属于 Host。
        delay: Option<u64>,
        /// 结构性标题级别（可选）：用于页面划分（如弹窗页签的页面标题），
        /// 不属于字形样式；Host 决定如何呈现标题层级。
        heading: Option<HeadingLevel>,
    },
    Image {
        resource: String,
        alt: TextValue,
        caption: Option<TextValue>,
    },
    Region {
        region: RegionId,
        content: Surface,
    },
    /// 由稳定 key 标识、可被后续 `replace` 定位的普通内容容器。
    Container {
        content: Surface,
    },
    Component {
        capability: ComponentCapability,
        version: u16,
        properties: BTreeMap<String, SurfaceValue>,
        fallback: Surface,
    },
    Replace {
        target: SurfaceTarget,
        content: Surface,
    },
    Action {
        label: TextValue,
        action: SurfaceAction,
        role: ActionRole,
    },
    Input {
        id: InteractionId,
        binding: SurfaceInputBinding,
    },
    Navigation {
        id: InteractionId,
        label: TextValue,
        target: String,
        role: NavigationRole,
    },
    /// 没有作者导航动作时，由 Engine 追加的安全返回语义。
    SafeReturn {
        id: InteractionId,
        target: String,
    },
}

/// 一次 Core 执行产生的有序语义输出，不规定宿主如何呈现。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surface {
    nodes: Vec<SurfaceNode>,
    keys: Vec<Option<SurfaceKey>>,
}

impl Surface {
    /// 从已经排好顺序的语义节点建立输出。
    pub fn from_nodes(nodes: Vec<SurfaceNode>) -> Self {
        let keys: Vec<Option<SurfaceKey>> = vec![None; nodes.len()];
        Self { nodes, keys }
    }

    /// 按产生顺序读取语义节点。
    pub fn nodes(&self) -> &[SurfaceNode] {
        &self.nodes
    }

    /// 在当前执行输出末尾追加一个语义节点。
    pub fn push(&mut self, node: SurfaceNode) {
        self.nodes.push(node);
        self.keys.push(None);
    }

    /// 使用稳定 key 追加节点；同一输出内拒绝重复 key。
    pub fn push_keyed(
        &mut self,
        key: SurfaceKey,
        node: SurfaceNode,
    ) -> Result<(), SurfaceKeyError> {
        if self
            .keys
            .iter()
            .flatten()
            .any(|candidate| candidate == &key)
        {
            return Err(SurfaceKeyError::Duplicate(key.0));
        }
        self.nodes.push(node);
        self.keys.push(Some(key));
        Ok(())
    }

    /// 返回节点对应的显式稳定 key；旧式节点没有 key。
    pub fn key(&self, index: usize) -> Option<&SurfaceKey> {
        self.keys.get(index).and_then(Option::as_ref)
    }

    /// 消费另一份输出，并保持两者原有顺序。
    pub fn append(&mut self, other: Surface) {
        for key in other.keys.iter().flatten() {
            assert!(
                !self.keys.iter().flatten().any(|candidate| candidate == key),
                "合并 Surface 时 key 重复：{}",
                key.as_str()
            );
        }
        self.nodes.extend(other.nodes);
        self.keys.extend(other.keys);
    }

    /// 当前执行是否没有产生语义节点。
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// 返回当前语义节点数量。
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// 是否包含作者产生的导航动作；SafeReturn 不计入作者动作。
    pub fn has_navigation(&self) -> bool {
        self.nodes.iter().any(|node: &SurfaceNode| match node {
            SurfaceNode::Navigation { .. } => true,
            SurfaceNode::Region { content, .. }
            | SurfaceNode::Container { content }
            | SurfaceNode::Replace { content, .. } => content.has_navigation(),
            SurfaceNode::Component { fallback, .. } => fallback.has_navigation(),
            _ => false,
        })
    }

    /// 只解析本次 Core 输出中真实存在的交互身份。
    pub fn interaction_target(&self, id: &InteractionId) -> Option<&str> {
        self.nodes.iter().find_map(|node: &SurfaceNode| match node {
            SurfaceNode::Navigation {
                id: candidate,
                target,
                ..
            }
            | SurfaceNode::SafeReturn {
                id: candidate,
                target,
            } if candidate == id => Some(target.as_str()),
            SurfaceNode::Region { content, .. }
            | SurfaceNode::Container { content }
            | SurfaceNode::Replace { content, .. } => content.interaction_target(id),
            SurfaceNode::Component { fallback, .. } => fallback.interaction_target(id),
            _ => None,
        })
    }

    /// 从当前可见输出查找输入契约；Region、Component fallback 与替换内容均递归验证。
    pub fn input_binding(&self, id: &InteractionId) -> Option<&SurfaceInputBinding> {
        self.nodes.iter().find_map(|node: &SurfaceNode| match node {
            SurfaceNode::Input {
                id: candidate,
                binding,
            } if candidate == id => Some(binding),
            SurfaceNode::Region { content, .. } => content.input_binding(id),
            SurfaceNode::Container { content } => content.input_binding(id),
            SurfaceNode::Replace { content, .. } => content.input_binding(id),
            SurfaceNode::Component { fallback, .. } => fallback.input_binding(id),
            _ => None,
        })
    }
}

//! Narrava Core 产生的宿主无关语义输出。

use std::{collections::BTreeMap, error::Error, fmt};

use crate::expression::value::TextValue;

/// Core 产生、Host 只能原样回送的交互身份。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InteractionId(String);

/// Renderer 跨更新复用语义节点的稳定身份。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PresentationKey(String);

/// 同一组单选输入共享的宿主无关身份；Host 可映射为 TUI 组或 HTML `name`。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InputGroupId(String);

impl InputGroupId {
    pub(crate) fn from_key(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// PresentationKey 在公开边界的验证错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresentationKeyError {
    Empty,
    Duplicate(String),
}

impl fmt::Display for PresentationKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("Presentation key 不能为空"),
            Self::Duplicate(key) => write!(formatter, "Presentation key 重复：{key}"),
        }
    }
}

impl Error for PresentationKeyError {}

impl PresentationKey {
    pub fn parse(key: impl Into<String>) -> Result<Self, PresentationKeyError> {
        let key: String = key.into();
        if key.is_empty() {
            return Err(PresentationKeyError::Empty);
        }
        Ok(Self(key))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 文本的跨宿主含义，不包含 CSS class、颜色或 HTML 标签。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextStyle {
    Emphasis,
    Strong,
    Code,
    Deleted,
    Inserted,
    Marked,
    Small,
    Subscript,
    Superscript,
    Quote,
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Heading6,
}

/// 与主题配色关联的语义语气；颜色由 Host 决定。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextTone {
    #[default]
    Default,
    Muted,
    Accent,
    Informational,
    Positive,
    Warning,
    Negative,
    Critical,
}

/// Host 可映射到稳定容器的布局区域。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PresentationRegion {
    Header,
    Main,
    Footer,
    Bar,
    BarStowed,
    Dialog,
}

/// `replace` 可跨 Host 解析的目标；不包含 CSS selector 或终端坐标。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PresentationTarget {
    Region(PresentationRegion),
    Key(PresentationKey),
}

/// 不进入 Story 导航的宿主级动作。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PresentationAction {
    Dismiss,
}

/// 动作在交互层级中的语义，不规定具体颜色或控件。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ActionRole {
    #[default]
    Default,
    Primary,
    Secondary,
    Danger,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NavigationRole {
    #[default]
    Link,
    Button,
}

/// 状态绑定输入控件的宿主无关种类。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresentationInputKind {
    Checkbox {
        unchecked: PresentationValue,
        checked: PresentationValue,
        selected: bool,
    },
    Radio {
        group: InputGroupId,
        value: PresentationValue,
        selected: bool,
    },
    Text {
        value: TextValue,
    },
}

/// Core 保留的输入写回契约；receiver 不会发送给 WebView。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresentationInputBinding {
    pub receiver: String,
    pub kind: PresentationInputKind,
}

impl PresentationInputBinding {
    /// 校验 Host 回送值是否属于控件公开的可提交值集合。
    pub fn accepts(&self, value: &PresentationValue) -> bool {
        match &self.kind {
            PresentationInputKind::Checkbox {
                unchecked, checked, ..
            } => value == unchecked || value == checked,
            PresentationInputKind::Radio {
                value: expected, ..
            } => value == expected,
            PresentationInputKind::Text { .. } => matches!(value, PresentationValue::Text(_)),
        }
    }
}

/// Host 可理解或降级显示的版本化组件能力名称。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ComponentCapability(String);

impl ComponentCapability {
    pub fn parse(value: impl Into<String>) -> Result<Self, PresentationKeyError> {
        let value: String = value.into();
        if value.is_empty() {
            return Err(PresentationKeyError::Empty);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Component 属性使用的纯数据，不携带脚本函数或平台对象。
#[derive(Clone, Debug)]
pub enum PresentationValue {
    Null,
    Boolean(bool),
    Number(f64),
    Text(String),
    List(Vec<PresentationValue>),
    Map(BTreeMap<String, PresentationValue>),
}

impl PartialEq for PresentationValue {
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

impl Eq for PresentationValue {}

/// Binding 传入的交互身份格式无效。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionIdError {
    Empty,
}

impl fmt::Display for InteractionIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("交互身份不能为空"),
        }
    }
}

impl Error for InteractionIdError {}

impl InteractionId {
    /// Core 内部从编译身份或执行身份建立稳定键。
    pub(crate) fn from_key(key: impl Into<String>) -> Self {
        Self::parse(key).expect("Core 生成的 InteractionId 不能为空")
    }

    /// 从 Binding 可传输的字符串恢复身份；权限仍由 PresentationOutput 验证。
    pub fn parse(key: impl Into<String>) -> Result<Self, InteractionIdError> {
        let key: String = key.into();
        if key.is_empty() {
            return Err(InteractionIdError::Empty);
        }
        Ok(Self(key))
    }

    /// Binding 使用的稳定字符串表示。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 当前已实现的最小语义节点；后续类型只按稳定跨宿主语义扩展。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresentationNode {
    Text(TextValue),
    StyledText {
        text: TextValue,
        styles: Vec<TextStyle>,
        tone: TextTone,
    },
    Image {
        resource: String,
        alt: TextValue,
        caption: Option<TextValue>,
    },
    Region {
        region: PresentationRegion,
        content: PresentationOutput,
    },
    /// 由稳定 key 标识、可被后续 `replace` 定位的普通内容容器。
    Container {
        content: PresentationOutput,
    },
    Component {
        capability: ComponentCapability,
        version: u16,
        properties: BTreeMap<String, PresentationValue>,
        fallback: PresentationOutput,
    },
    Replace {
        target: PresentationTarget,
        content: PresentationOutput,
    },
    Action {
        label: TextValue,
        action: PresentationAction,
        role: ActionRole,
    },
    Input {
        id: InteractionId,
        binding: PresentationInputBinding,
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
pub struct PresentationOutput {
    nodes: Vec<PresentationNode>,
    keys: Vec<Option<PresentationKey>>,
}

impl PresentationOutput {
    /// 从已经排好顺序的语义节点建立输出。
    pub fn from_nodes(nodes: Vec<PresentationNode>) -> Self {
        let keys: Vec<Option<PresentationKey>> = vec![None; nodes.len()];
        Self { nodes, keys }
    }

    /// 按产生顺序读取语义节点。
    pub fn nodes(&self) -> &[PresentationNode] {
        &self.nodes
    }

    /// 在当前执行输出末尾追加一个语义节点。
    pub fn push(&mut self, node: PresentationNode) {
        self.nodes.push(node);
        self.keys.push(None);
    }

    /// 使用稳定 key 追加节点；同一输出内拒绝重复 key。
    pub fn push_keyed(
        &mut self,
        key: PresentationKey,
        node: PresentationNode,
    ) -> Result<(), PresentationKeyError> {
        if self
            .keys
            .iter()
            .flatten()
            .any(|candidate| candidate == &key)
        {
            return Err(PresentationKeyError::Duplicate(key.0));
        }
        self.nodes.push(node);
        self.keys.push(Some(key));
        Ok(())
    }

    /// 返回节点对应的显式稳定 key；旧式节点没有 key。
    pub fn key(&self, index: usize) -> Option<&PresentationKey> {
        self.keys.get(index).and_then(Option::as_ref)
    }

    /// 消费另一份输出，并保持两者原有顺序。
    pub fn append(&mut self, other: PresentationOutput) {
        for key in other.keys.iter().flatten() {
            assert!(
                !self.keys.iter().flatten().any(|candidate| candidate == key),
                "合并 PresentationOutput 时 key 重复：{}",
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
        self.nodes.iter().any(|node: &PresentationNode| match node {
            PresentationNode::Navigation { .. } => true,
            PresentationNode::Region { content, .. }
            | PresentationNode::Container { content }
            | PresentationNode::Replace { content, .. } => content.has_navigation(),
            PresentationNode::Component { fallback, .. } => fallback.has_navigation(),
            _ => false,
        })
    }

    /// 只解析本次 Core 输出中真实存在的交互身份。
    pub fn interaction_target(&self, id: &InteractionId) -> Option<&str> {
        self.nodes
            .iter()
            .find_map(|node: &PresentationNode| match node {
                PresentationNode::Navigation {
                    id: candidate,
                    target,
                    ..
                }
                | PresentationNode::SafeReturn {
                    id: candidate,
                    target,
                } if candidate == id => Some(target.as_str()),
                PresentationNode::Region { content, .. }
                | PresentationNode::Container { content }
                | PresentationNode::Replace { content, .. } => content.interaction_target(id),
                PresentationNode::Component { fallback, .. } => fallback.interaction_target(id),
                _ => None,
            })
    }

    /// 从当前可见输出查找输入契约；Region、Component fallback 与替换内容均递归验证。
    pub fn input_binding(&self, id: &InteractionId) -> Option<&PresentationInputBinding> {
        self.nodes
            .iter()
            .find_map(|node: &PresentationNode| match node {
                PresentationNode::Input {
                    id: candidate,
                    binding,
                } if candidate == id => Some(binding),
                PresentationNode::Region { content, .. } => content.input_binding(id),
                PresentationNode::Container { content } => content.input_binding(id),
                PresentationNode::Replace { content, .. } => content.input_binding(id),
                PresentationNode::Component { fallback, .. } => fallback.input_binding(id),
                _ => None,
            })
    }
}

//! Narrava Core 产生的宿主无关语义输出。

use std::{collections::BTreeMap, error::Error, fmt};

use crate::expression::value::TextValue;

/// Core 产生、Host 只能原样回送的交互身份。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InteractionId(String);

/// Renderer 跨更新复用语义节点的稳定身份。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceKey(String);

/// 同一组单选输入共享的宿主无关身份；Host 可映射为 TUI 组或 HTML `name`。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InputGroupId(String);

impl InputGroupId {
    /// 从编译生成的 key 建立组身份；仅 crate 内部使用。
    pub(crate) fn from_key(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// 稳定字符串表示，供 Host 序列化。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

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

/// 由各 Host 映射视觉效果的文本语义。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextStyle {
    /// 强调（斜体）。
    Emphasis,
    /// 加粗。
    Strong,
    /// 代码/等宽。
    Code,
    /// 引用。
    Quote,
    /// 标记/高亮。
    Marked,
    /// 小字。
    Small,
    /// 新增内容（编辑痕迹）。
    Inserted,
    /// 删除内容（编辑痕迹）。
    Deleted,
}

/// 结构性标题级别：表达文档层级（如弹窗页签的页面标题），不属于字形样式。
/// Host 用它划分页面或渲染标题元素；无标题时是普通文本。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HeadingLevel {
    /// 一级标题。
    H1,
    /// 二级标题。
    H2,
}

impl HeadingLevel {
    /// 从 `1..=2` 解析；其他值返回 `None`。
    pub fn from_u8(level: u8) -> Option<Self> {
        match level {
            1 => Some(Self::H1),
            2 => Some(Self::H2),
            _ => None,
        }
    }

    /// 数值级别（`1` 或 `2`），供 Host 序列化。
    pub fn level(self) -> u8 {
        match self {
            Self::H1 => 1,
            Self::H2 => 2,
        }
    }
}

/// Narrava 标准 64 色调色板索引。Protocol 只传递稳定色号，实际颜色由 Host 映射。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextColor(u8);

impl TextColor {
    /// 色阶最大值（含）。
    pub const MAX_INDEX: u8 = 63;
    /// 正文默认（不染色，由 Host 默认前景呈现）。
    pub const DEFAULT: TextColor = TextColor(0);
    /// 白（灰阶）。
    pub const WHITE: TextColor = TextColor(1);
    /// 亮灰（灰阶）。
    pub const BRIGHT_GRAY: TextColor = TextColor(2);
    /// 浅灰（灰阶）。
    pub const LIGHT_GRAY: TextColor = TextColor(3);
    /// 灰（灰阶）。
    pub const GRAY: TextColor = TextColor(4);
    /// 深灰（灰阶）。
    pub const DARK_GRAY: TextColor = TextColor(5);
    /// 暗灰/近黑（灰阶）。
    pub const NEAR_BLACK: TextColor = TextColor(6);
    /// 黑（灰阶）。
    pub const BLACK: TextColor = TextColor(7);
    /// 红（光谱，8 级色相族起点）。
    pub const RED: TextColor = TextColor(8);
    /// 橙（光谱）。
    pub const ORANGE: TextColor = TextColor(16);
    /// 黄（光谱）。
    pub const YELLOW: TextColor = TextColor(24);
    /// 绿（光谱）。
    pub const GREEN: TextColor = TextColor(32);
    /// 蓝（光谱）。
    pub const BLUE: TextColor = TextColor(40);
    /// 紫（光谱）。
    pub const VIOLET: TextColor = TextColor(48);
    /// 深紫（光谱）。
    pub const DEEP_VIOLET: TextColor = TextColor(56);
    /// 光谱终点。
    pub const PEAK: TextColor = TextColor(63);

    /// 从 0..=63 构造；越界返回 `None`。
    pub fn from_index(value: u8) -> Option<TextColor> {
        (value <= Self::MAX_INDEX).then_some(TextColor(value))
    }

    /// 色阶序号 0..=63。
    pub fn index(self) -> u8 {
        self.0
    }
}

/// 逻辑区域身份；不是 CSS selector、DOM id 或终端坐标。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionId(String);

/// RegionId 输入无效。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionIdError {
    Empty,
}

impl fmt::Display for RegionIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RegionId 不能为空")
    }
}

impl Error for RegionIdError {}

impl RegionId {
    /// 校验并建立开放区域身份；标准名称和 Host 未知的自定义名称使用同一规则。
    pub fn parse(value: impl Into<String>) -> Result<Self, RegionIdError> {
        let value: String = value.into();
        if value.is_empty() {
            return Err(RegionIdError::Empty);
        }
        Ok(Self(value))
    }

    /// Passage 正文区域。
    pub fn main() -> Self {
        Self(String::from("main"))
    }
    /// Passage 页眉区域。
    pub fn header() -> Self {
        Self(String::from("header"))
    }
    /// Passage 页脚区域。
    pub fn footer() -> Self {
        Self(String::from("footer"))
    }
    /// 展开的作者侧栏区域。
    pub fn bar() -> Self {
        Self(String::from("bar"))
    }
    /// 收起后的作者侧栏摘要区域。
    pub fn bar_stowed() -> Self {
        Self(String::from("bar-stowed"))
    }
    /// 模态内容区域。
    pub fn dialog() -> Self {
        Self(String::from("dialog"))
    }

    /// 稳定逻辑名称；Host 不得把它直接解释为 selector 或坐标。
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

/// 动作在交互层级中的语义，不规定具体颜色或控件。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ActionRole {
    /// 常规动作。
    #[default]
    Default,
    /// 主动作。
    Primary,
    /// 次级动作。
    Secondary,
    /// 危险动作。
    Danger,
}

/// 导航交互在界面语义中的角色，不规定具体控件样式。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NavigationRole {
    /// 链接式导航。
    #[default]
    Link,
    /// 按钮式导航。
    Button,
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

/// Host 可理解或降级显示的版本化组件能力名称。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ComponentCapability(String);

impl ComponentCapability {
    /// 校验并构造能力名；空字符串被拒绝。
    pub fn parse(value: impl Into<String>) -> Result<Self, SurfaceKeyError> {
        let value: String = value.into();
        if value.is_empty() {
            return Err(SurfaceKeyError::Empty);
        }
        Ok(Self(value))
    }

    /// 稳定字符串表示，供 Host 与能力注册表对照。
    pub fn as_str(&self) -> &str {
        &self.0
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

    /// 从 Binding 可传输的字符串恢复身份；权限仍由 Surface 验证。
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

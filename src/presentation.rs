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
    /// 从编译生成的 key 建立组身份；仅 crate 内部使用。
    pub(crate) fn from_key(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// 稳定字符串表示，供 Host 序列化。
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
    /// 校验并构造 key；空字符串被拒绝。
    pub fn parse(key: impl Into<String>) -> Result<Self, PresentationKeyError> {
        let key: String = key.into();
        if key.is_empty() {
            return Err(PresentationKeyError::Empty);
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

/// 64 级状态色阶（0..=63）：单一"状态"强度维度，0 最弱、63 最强。
/// 0..=63 恰好占 6 位，可紧凑地二进制记录。
/// 两端 Host 的映射参考（对齐二进制边界）：灰阶 0-7（3 位），
/// 光谱 8-63、每个色相 8 级：红`8`→橙`16`→黄`24`→绿`32`→蓝`40`→紫`48`→深紫`56`→`63`。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextTone(u8);

impl TextTone {
    /// 色阶最大值（含）。
    pub const MAX_INDEX: u8 = 63;
    /// 正文默认（不染色，由 Host 默认前景呈现）。
    pub const DEFAULT: TextTone = TextTone(0);
    /// 白（灰阶）。
    pub const WHITE: TextTone = TextTone(1);
    /// 亮灰（灰阶）。
    pub const BRIGHT_GRAY: TextTone = TextTone(2);
    /// 浅灰（灰阶）。
    pub const LIGHT_GRAY: TextTone = TextTone(3);
    /// 灰（灰阶）。
    pub const GRAY: TextTone = TextTone(4);
    /// 深灰（灰阶）。
    pub const DARK_GRAY: TextTone = TextTone(5);
    /// 暗灰/近黑（灰阶）。
    pub const NEAR_BLACK: TextTone = TextTone(6);
    /// 黑（灰阶）。
    pub const BLACK: TextTone = TextTone(7);
    /// 红（光谱，8 级色相族起点）。
    pub const RED: TextTone = TextTone(8);
    /// 橙（光谱）。
    pub const ORANGE: TextTone = TextTone(16);
    /// 黄（光谱）。
    pub const YELLOW: TextTone = TextTone(24);
    /// 绿（光谱）。
    pub const GREEN: TextTone = TextTone(32);
    /// 蓝（光谱）。
    pub const BLUE: TextTone = TextTone(40);
    /// 紫（光谱）。
    pub const VIOLET: TextTone = TextTone(48);
    /// 深紫（光谱）。
    pub const DEEP_VIOLET: TextTone = TextTone(56);
    /// 光谱终点。
    pub const PEAK: TextTone = TextTone(63);

    /// 从 0..=63 构造；越界返回 `None`。
    pub fn from_index(value: u8) -> Option<TextTone> {
        (value <= Self::MAX_INDEX).then_some(TextTone(value))
    }

    /// 色阶序号 0..=63。
    pub fn index(self) -> u8 {
        self.0
    }
}

/// Host 可映射到稳定容器的布局区域。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PresentationRegion {
    /// 顶部区域。
    Header,
    /// 主体区域。
    Main,
    /// 底部区域。
    Footer,
    /// 侧栏（工具栏）区域。
    Bar,
    /// 侧栏收起状态区域。
    BarStowed,
    /// 对话框区域。
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
    /// 校验并构造能力名；空字符串被拒绝。
    pub fn parse(value: impl Into<String>) -> Result<Self, PresentationKeyError> {
        let value: String = value.into();
        if value.is_empty() {
            return Err(PresentationKeyError::Empty);
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
        /// 显示延迟（毫秒）：渲染器应在此之前保持文本不可见，到时淡入浮现。
        /// 属于表现时序提示，不改变文本语义与 I18n 身份。
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

    /// 收束最终可见文本的换行语义：源码中的 CR/LF 只作为排版空白，作者只有写出
    /// `<br>` 才能请求硬换行。转换发生在 HostUpdate 边界，因此 VM、脚本 Macro、
    /// I18n 与嵌套 Region/Component fallback 都遵守同一规则，各 Host 无需重复处理。
    pub(crate) fn normalize_visible_line_breaks(&mut self) {
        for node in &mut self.nodes {
            normalize_node_line_breaks(node);
        }
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

/// 递归规范一个最终 Presentation 节点中确实会显示给玩家的文本字段。
fn normalize_node_line_breaks(node: &mut PresentationNode) {
    match node {
        PresentationNode::Text(text) | PresentationNode::StyledText { text, .. } => {
            *text = normalize_visible_text(text);
        }
        PresentationNode::Image { alt, caption, .. } => {
            *alt = normalize_visible_text(alt);
            if let Some(caption) = caption {
                *caption = normalize_visible_text(caption);
            }
        }
        PresentationNode::Region { content, .. }
        | PresentationNode::Container { content }
        | PresentationNode::Replace { content, .. } => content.normalize_visible_line_breaks(),
        PresentationNode::Component {
            properties,
            fallback,
            ..
        } => {
            for value in properties.values_mut() {
                normalize_presentation_value_line_breaks(value);
            }
            fallback.normalize_visible_line_breaks();
        }
        PresentationNode::Action { label, .. } | PresentationNode::Navigation { label, .. } => {
            *label = normalize_visible_text(label);
        }
        PresentationNode::Input { .. } | PresentationNode::SafeReturn { .. } => {}
    }
}

/// Component 属性由 Host 决定哪些字段可见，因此递归规范其中的文本值；Input 的
/// State 绑定不经过这里，避免把玩家实际输入的数据改写为排版文本。
fn normalize_presentation_value_line_breaks(value: &mut PresentationValue) {
    match value {
        PresentationValue::Text(text) => *text = normalize_visible_string(text),
        PresentationValue::List(values) => {
            for value in values {
                normalize_presentation_value_line_breaks(value);
            }
        }
        PresentationValue::Map(properties) => {
            for value in properties.values_mut() {
                normalize_presentation_value_line_breaks(value);
            }
        }
        PresentationValue::Null | PresentationValue::Boolean(_) | PresentationValue::Number(_) => {}
    }
}

/// PresentationValue 的文本保证是 Unicode String，复用同一 UTF-16 规则后再转回。
fn normalize_visible_string(text: &str) -> String {
    normalize_visible_text(&TextValue::from(text))
        .to_unicode_string()
        .expect("Unicode String 经过 ASCII 换行规范后仍应是 Unicode")
}

/// 在 UTF-16 码元上识别 ASCII `<br>`，因此即使文本含孤立代理项也不会损失数据。
/// 普通源码换行及其缩进折叠为一个空格；显式 `<br>` 转成宿主无关的 `\n`。
fn normalize_visible_text(text: &TextValue) -> TextValue {
    const BREAK: [u16; 4] = [b'<' as u16, b'b' as u16, b'r' as u16, b'>' as u16];

    let input: &[u16] = text.as_units();
    let mut output: Vec<u16> = Vec::with_capacity(input.len());
    let mut index: usize = 0;
    while index < input.len() {
        if input[index..].starts_with(&BREAK) {
            while matches!(output.last(), Some(unit) if matches!(*unit, 0x20 | 0x09)) {
                output.pop();
            }
            output.push(b'\n' as u16);
            index += BREAK.len();
            while matches!(input.get(index), Some(unit) if matches!(*unit, 0x20 | 0x09 | 0x0a | 0x0d))
            {
                index += 1;
            }
            continue;
        }
        if matches!(input[index], 0x0a | 0x0d) {
            while matches!(output.last(), Some(unit) if matches!(*unit, 0x20 | 0x09)) {
                output.pop();
            }
            index += 1;
            while matches!(input.get(index), Some(unit) if matches!(*unit, 0x20 | 0x09 | 0x0a | 0x0d))
            {
                index += 1;
            }
            if !output.is_empty() && output.last() != Some(&(b'\n' as u16)) && index < input.len() {
                output.push(b' ' as u16);
            }
            continue;
        }
        output.push(input[index]);
        index += 1;
    }
    TextValue::from_units(output)
}

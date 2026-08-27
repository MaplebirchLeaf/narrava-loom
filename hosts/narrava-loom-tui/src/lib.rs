//! Core Surface 到终端区域、文本和交互列表的最小 Host Renderer。
//!
//! 语义与视觉分离：颜色只由 64 级 color 色阶（0 最弱、63 最强）决定，字形只由语义
//! TextStyle 决定；`delay > 0` 的文本停放在 `frame.delayed`，由消费方在最小
//! `delay_ms` 之后用 `render_at` 重新渲染。

use std::{
    collections::BTreeMap,
    fmt,
    io::{self, BufRead, Write},
};

use narrava_loom_core::semantic::{HeadingLevel, NavigationRole, RegionId, TextColor, TextStyle};
use narrava_loom_protocol::{
    Surface, SurfaceAction, SurfaceInputKind, SurfaceNode, SurfaceTarget, SurfaceValue,
};

/// 输入控件执行时需要的完整语义。TUI 保留这些值，避免终端层根据标签反推状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TuiInput {
    Checkbox {
        unchecked: SurfaceValue,
        checked: SurfaceValue,
        selected: bool,
    },
    Radio {
        value: SurfaceValue,
        selected: bool,
    },
    Text {
        value: String,
    },
}

/// 一帧中可供终端玩家触发的动作（导航、按钮或输入控件）。
/// `id` 为 `None` 的动作（如 dismiss）没有可回传的身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TuiInteraction {
    /// Core 交互身份；`None` 表示纯客户端动作。
    pub id: Option<String>,
    /// 展示给玩家的动作文本。
    pub label: String,
    /// 动作类别：`link`/`button`/`checkbox`/`radiobutton`/`textbox`/`dismiss`/`safe-return`。
    pub kind: &'static str,
    /// 仅输入控件携带；导航、按钮与客户端动作均为 `None`。
    pub input: Option<TuiInput>,
}

/// 玩家在终端提示符中可输入的命令。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TuiCommand {
    Select(usize),
    Set { index: usize, value: String },
    Help,
    Redraw,
    Quit,
}

impl TuiCommand {
    /// 解析面向玩家的一基序号；空行只重绘，不会误触首个交互。
    pub fn parse(line: &str) -> Result<Self, TuiCommandError> {
        let trimmed: &str = line.trim();
        if trimmed.is_empty() || matches!(trimmed, "r" | "redraw") {
            return Ok(Self::Redraw);
        }
        if matches!(trimmed, "h" | "help" | "?") {
            return Ok(Self::Help);
        }
        if matches!(trimmed, "q" | "quit" | "exit") {
            return Ok(Self::Quit);
        }
        if let Ok(index) = trimmed.parse::<usize>() {
            return one_based(index).map(Self::Select);
        }
        let mut parts = trimmed.splitn(3, char::is_whitespace);
        if matches!(parts.next(), Some("set" | "input")) {
            let index: usize = parts
                .next()
                .ok_or(TuiCommandError::Usage)?
                .parse()
                .map_err(|_| TuiCommandError::Usage)?;
            let value: String = parts.next().unwrap_or_default().to_owned();
            return Ok(Self::Set {
                index: one_based(index)?,
                value,
            });
        }
        Err(TuiCommandError::Unknown)
    }

    /// 把命令解析为 Host 可执行操作，并按当前帧验证交互类型。
    pub fn resolve(&self, frame: &TuiFrame) -> Result<TuiOperation, TuiCommandError> {
        match self {
            Self::Select(index) => resolve_select(frame, *index),
            Self::Set { index, value } => {
                let interaction = interaction_at(frame, *index)?;
                if !matches!(interaction.input, Some(TuiInput::Text { .. })) {
                    return Err(TuiCommandError::NotTextInput);
                }
                let id = interaction
                    .id
                    .clone()
                    .ok_or(TuiCommandError::MissingIdentity)?;
                Ok(TuiOperation::Input {
                    id,
                    value: SurfaceValue::Text(value.clone()),
                })
            }
            Self::Help => Ok(TuiOperation::Help),
            Self::Redraw => Ok(TuiOperation::Redraw),
            Self::Quit => Ok(TuiOperation::Quit),
        }
    }
}

/// 终端协议解析出的平台无关操作。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TuiOperation {
    Activate { id: String },
    Input { id: String, value: SurfaceValue },
    Dismiss,
    Help,
    Redraw,
    Quit,
}

/// 玩家命令错误；消息可以直接显示在终端中。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuiCommandError {
    Unknown,
    Usage,
    ZeroIndex,
    OutOfRange,
    TextNeedsValue,
    NotTextInput,
    MissingIdentity,
}

impl fmt::Display for TuiCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unknown => "未知命令；输入 help 查看帮助",
            Self::Usage => "用法：输入序号，或 set <序号> <文字>",
            Self::ZeroIndex => "交互序号从 1 开始",
            Self::OutOfRange => "该交互序号不在当前画面中",
            Self::TextNeedsValue => "文本框需要使用 set <序号> <文字>",
            Self::NotTextInput => "set 只能用于文本框",
            Self::MissingIdentity => "该交互没有可回传的 Core 身份",
        };
        formatter.write_str(message)
    }
}

fn one_based(index: usize) -> Result<usize, TuiCommandError> {
    index.checked_sub(1).ok_or(TuiCommandError::ZeroIndex)
}

fn interaction_at(frame: &TuiFrame, index: usize) -> Result<&TuiInteraction, TuiCommandError> {
    frame
        .interactions
        .get(index)
        .ok_or(TuiCommandError::OutOfRange)
}

fn resolve_select(frame: &TuiFrame, index: usize) -> Result<TuiOperation, TuiCommandError> {
    let interaction = interaction_at(frame, index)?;
    if interaction.kind == "dismiss" {
        return Ok(TuiOperation::Dismiss);
    }
    let id = interaction
        .id
        .clone()
        .ok_or(TuiCommandError::MissingIdentity)?;
    match &interaction.input {
        Some(TuiInput::Checkbox {
            unchecked,
            checked,
            selected,
        }) => Ok(TuiOperation::Input {
            id,
            value: if *selected {
                unchecked.clone()
            } else {
                checked.clone()
            },
        }),
        Some(TuiInput::Radio { value, .. }) => Ok(TuiOperation::Input {
            id,
            value: value.clone(),
        }),
        Some(TuiInput::Text { .. }) => Err(TuiCommandError::TextNeedsValue),
        None => Ok(TuiOperation::Activate { id }),
    }
}

/// 运行阻塞式终端输入循环。
///
/// `dispatch` 只接收已经过当前帧验证的操作；返回新帧时立即替换画面，返回 `None`
/// 表示状态已写入但无需重绘。该边界不持有 Runtime，便于 Native Host 把 Core worker、
/// 存档或远程会话接到同一套终端协议上。
pub fn run_terminal<R, W, F, E>(
    reader: &mut R,
    writer: &mut W,
    mut frame: TuiFrame,
    mut dispatch: F,
) -> io::Result<()>
where
    R: BufRead,
    W: Write,
    F: FnMut(TuiOperation) -> Result<Option<TuiFrame>, E>,
    E: fmt::Display,
{
    write_frame(writer, &frame)?;
    let mut line = String::new();
    loop {
        write!(writer, "\n> ")?;
        writer.flush()?;
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            writeln!(writer)?;
            return Ok(());
        }
        let command = match TuiCommand::parse(&line) {
            Ok(command) => command,
            Err(error) => {
                writeln!(writer, "! {error}")?;
                continue;
            }
        };
        let operation = match command.resolve(&frame) {
            Ok(operation) => operation,
            Err(error) => {
                writeln!(writer, "! {error}")?;
                continue;
            }
        };
        match operation {
            TuiOperation::Quit => return Ok(()),
            TuiOperation::Help => {
                write_help(writer)?;
                continue;
            }
            TuiOperation::Redraw => {
                write_frame(writer, &frame)?;
                continue;
            }
            operation => match dispatch(operation) {
                Ok(Some(next)) => {
                    frame = next;
                    write_frame(writer, &frame)?;
                }
                Ok(None) => writeln!(writer, "已更新。")?,
                Err(error) => writeln!(writer, "! {error}")?,
            },
        }
    }
}

/// 打印一帧完整终端画面。空区域不会制造无意义标题。
pub fn write_frame(writer: &mut impl Write, frame: &TuiFrame) -> io::Result<()> {
    writeln!(writer, "\n== {} ==", frame.current)?;
    write_region(writer, "页眉", &frame.header)?;
    write_region(writer, "正文", &frame.main)?;
    write_region(writer, "侧栏", &frame.bar)?;
    write_region(writer, "收起侧栏", &frame.bar_stowed)?;
    write_region(writer, "弹窗", &frame.dialog)?;
    write_region(writer, "页脚", &frame.footer)?;
    if frame.interactions.is_empty() {
        writeln!(writer, "\n（当前没有可操作项；输入 help 查看命令）")?;
    } else {
        writeln!(writer, "\n操作：")?;
        for (index, interaction) in frame.interactions.iter().enumerate() {
            writeln!(
                writer,
                "  {}. {} {}",
                index + 1,
                interaction.label,
                interaction_hint(interaction)
            )?;
        }
    }
    Ok(())
}

fn write_region(writer: &mut impl Write, name: &str, lines: &[String]) -> io::Result<()> {
    if !lines.is_empty() {
        writeln!(writer, "\n{name}：")?;
        for line in lines {
            writeln!(writer, "{line}")?;
        }
    }
    Ok(())
}

fn interaction_hint(interaction: &TuiInteraction) -> &'static str {
    match interaction.input {
        Some(TuiInput::Text { .. }) => "（用 set 序号 文字 修改）",
        Some(_) => "（输入序号切换）",
        None => "",
    }
}

fn write_help(writer: &mut impl Write) -> io::Result<()> {
    writeln!(writer, "命令：")?;
    writeln!(
        writer,
        "  <序号>              激活链接、按钮、单选框或复选框"
    )?;
    writeln!(writer, "  set <序号> <文字>   修改文本框；空文字也允许")?;
    writeln!(writer, "  redraw              重绘当前画面")?;
    writeln!(writer, "  help                显示本帮助")?;
    writeln!(writer, "  quit                退出")
}

/// 区域内的一个行文本块；`key` 为 `None` 时只能整块替换，为 `Some` 时可被
/// Replace 按 key 局部覆盖。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TuiBlock {
    key: Option<String>,
    lines: Vec<String>,
}

/// 单个区域下按出现顺序排列的文本块集合。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TuiSurface {
    blocks: Vec<TuiBlock>,
}

impl TuiSurface {
    /// 按块顺序拼接全部行。
    fn lines(&self) -> Vec<String> {
        self.blocks
            .iter()
            .flat_map(|block| block.lines.iter().cloned())
            .collect()
    }

    /// 用新行替换首个匹配 key 的块；找不到匹配时返回 `false`。
    fn replace_key(&mut self, key: &str, lines: &[String]) -> bool {
        let Some(block) = self
            .blocks
            .iter_mut()
            .find(|block| block.key.as_deref() == Some(key))
        else {
            return false;
        };
        block.lines = lines.to_vec();
        true
    }
}

/// 延迟显示的一段文本：由终端消费方在 `delay_ms` 之后用 `render_at` 重新渲染。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TuiDelayedText {
    /// 文本最终应进入的区域名。
    pub region: String,
    /// 待显示的行文本。
    pub lines: Vec<String>,
    /// 到达显示时刻前还需等待的毫秒数。
    pub delay_ms: u64,
}

/// 一帧静态可打印画面：按区域拆分的行文本、可触发动作与尚未到时的延迟文本。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TuiFrame {
    /// 当前 Passage 名。
    pub current: String,
    /// 页眉区域。
    pub header: Vec<String>,
    /// 正文区域。
    pub main: Vec<String>,
    /// 页脚区域。
    pub footer: Vec<String>,
    /// 侧栏区域。
    pub bar: Vec<String>,
    /// 收起状态的侧栏区域。
    pub bar_stowed: Vec<String>,
    /// 弹窗区域。
    pub dialog: Vec<String>,
    /// Host 不认识的开放区域；按逻辑 RegionId 原名保留，绝不静默丢弃。
    pub custom: BTreeMap<String, Vec<String>>,
    /// 玩家可触发的动作列表。
    pub interactions: Vec<TuiInteraction>,
    /// `delay > 0` 且未到时刻的文本；消费方按其最小 `delay_ms` 安排下次 `render_at`。
    pub delayed: Vec<TuiDelayedText>,
}

/// 无状态的纯渲染器：每次 `render`/`render_at` 都从输出重建帧，
/// 不保存跨帧状态；真实终端的输入循环与屏幕管理由消费方负责。
#[derive(Clone, Debug, Default)]
pub struct TuiRenderer {
    /// 各区域名到其文本面的缓冲。
    surfaces: BTreeMap<String, TuiSurface>,
    /// 本帧收集到的可触发交互。
    interactions: Vec<TuiInteraction>,
    /// 本帧停放、尚未到时的延迟文本。
    delayed: Vec<TuiDelayedText>,
}

impl TuiRenderer {
    /// 渲染当前时刻（elapsed = 0）的帧；`delay > 0` 的文本停放在 `frame.delayed`。
    pub fn render(&mut self, current: &str, output: &Surface) -> TuiFrame {
        self.render_at(current, output, 0)
    }

    /// 渲染经过 `elapsed_ms` 毫秒后的帧：`delay <= elapsed_ms` 的文本进入对应区域，
    /// 其余仍停放在 `frame.delayed` 供消费方继续等待。
    pub fn render_at(&mut self, current: &str, output: &Surface, elapsed_ms: u64) -> TuiFrame {
        self.surfaces.clear();
        self.interactions.clear();
        self.delayed.clear();
        self.render_output(RegionId::main(), output, elapsed_ms);
        self.frame(current)
    }

    /// 递归渲染输出树：Region 下钻、Replace 就地覆盖、未到时的延迟文本停放。
    fn render_output(&mut self, region: RegionId, output: &Surface, elapsed_ms: u64) {
        for (index, node) in output.nodes().iter().enumerate() {
            let key = output.key(index).map(|key| key.as_str().to_owned());
            match node {
                SurfaceNode::Region { region, content } => {
                    self.render_output(region.clone(), content, elapsed_ms)
                }
                SurfaceNode::Replace { target, content } => {
                    let lines = render_content(content, &mut self.interactions);
                    match target {
                        SurfaceTarget::Region(target) => {
                            self.surface_mut(target).blocks = vec![TuiBlock { key: None, lines }];
                        }
                        SurfaceTarget::Key(target) => {
                            for surface in self.surfaces.values_mut() {
                                if surface.replace_key(target.as_str(), &lines) {
                                    break;
                                }
                            }
                        }
                    }
                }
                SurfaceNode::StyledText {
                    delay: Some(delay), ..
                } if *delay > elapsed_ms => {
                    let lines = render_node(node, &mut self.interactions);
                    if !lines.is_empty() {
                        self.delayed.push(TuiDelayedText {
                            region: region.as_str().to_owned(),
                            lines,
                            delay_ms: *delay,
                        });
                    }
                }
                _ => {
                    let lines = render_node(node, &mut self.interactions);
                    if !lines.is_empty() {
                        self.surface_mut(&region)
                            .blocks
                            .push(TuiBlock { key, lines });
                    }
                }
            }
        }
    }

    /// 取（必要时创建）某区域的文本面。
    fn surface_mut(&mut self, region: &RegionId) -> &mut TuiSurface {
        self.surfaces.entry(region.as_str().to_owned()).or_default()
    }

    /// 把当前缓冲整理为一帧可打印画面。
    fn frame(&self, current: &str) -> TuiFrame {
        TuiFrame {
            current: current.to_owned(),
            header: self.lines(RegionId::header()),
            main: self.lines(RegionId::main()),
            footer: self.lines(RegionId::footer()),
            bar: self.lines(RegionId::bar()),
            bar_stowed: self.lines(RegionId::bar_stowed()),
            dialog: self.lines(RegionId::dialog()),
            custom: self
                .surfaces
                .iter()
                .filter(|(region, _)| !is_standard_region(region))
                .map(|(region, surface)| (region.clone(), surface.lines()))
                .collect(),
            interactions: self.interactions.clone(),
            delayed: self.delayed.clone(),
        }
    }

    /// 取某区域的全部行；区域从未出现时返回空。
    fn lines(&self, region: RegionId) -> Vec<String> {
        self.surfaces
            .get(region.as_str())
            .map_or_else(Vec::new, TuiSurface::lines)
    }
}

/// 渲染子输出并返回其全部行（顺带把可触发节点收集进 `interactions`）。
fn render_content(output: &Surface, interactions: &mut Vec<TuiInteraction>) -> Vec<String> {
    output
        .nodes()
        .iter()
        .flat_map(|node| render_node(node, interactions))
        .collect()
}

/// 渲染单个节点：文本/样式文本成行，图像转占位，Action/Input/Navigation/SafeReturn
/// 收集为交互（不产出行）。
fn render_node(node: &SurfaceNode, interactions: &mut Vec<TuiInteraction>) -> Vec<String> {
    match node {
        SurfaceNode::Text(text) => visible_lines(unicode(text)),
        SurfaceNode::HardBreak => Vec::new(),
        SurfaceNode::StyledText {
            text,
            styles,
            color,
            heading,
            ..
        } => visible_lines(styled(unicode(text), styles, *color, *heading)),
        SurfaceNode::Image {
            resource,
            alt,
            caption,
        } => vec![format!(
            "[图像: {} <{}>{}]",
            unicode(alt),
            resource,
            caption
                .as_ref()
                .map(|value| format!(" — {}", unicode(value)))
                .unwrap_or_default()
        )],
        SurfaceNode::Component { fallback, .. } => render_content(fallback, interactions),
        SurfaceNode::Container { content } => render_content(content, interactions),
        SurfaceNode::Action { label, action, .. } => {
            interactions.push(TuiInteraction {
                id: None,
                label: unicode(label),
                kind: match action {
                    SurfaceAction::Dismiss => "dismiss",
                },
                input: None,
            });
            Vec::new()
        }
        SurfaceNode::Input { id, binding } => {
            let (label, kind, input) = match &binding.kind {
                SurfaceInputKind::Checkbox {
                    unchecked,
                    checked,
                    selected,
                } => (
                    if *selected { "[x]" } else { "[ ]" }.to_owned(),
                    "checkbox",
                    TuiInput::Checkbox {
                        unchecked: unchecked.clone(),
                        checked: checked.clone(),
                        selected: *selected,
                    },
                ),
                SurfaceInputKind::Radio {
                    value, selected, ..
                } => (
                    if *selected { "(o)" } else { "( )" }.to_owned(),
                    "radiobutton",
                    TuiInput::Radio {
                        value: value.clone(),
                        selected: *selected,
                    },
                ),
                SurfaceInputKind::Text { value } => {
                    let value: String = unicode(value);
                    (format!("[{value}]"), "textbox", TuiInput::Text { value })
                }
            };
            interactions.push(TuiInteraction {
                id: Some(id.as_str().to_owned()),
                label,
                kind,
                input: Some(input),
            });
            Vec::new()
        }
        SurfaceNode::Navigation {
            id, label, role, ..
        } => {
            interactions.push(TuiInteraction {
                id: Some(id.as_str().to_owned()),
                label: unicode(label),
                kind: match role {
                    NavigationRole::Link => "link",
                    NavigationRole::Button => "button",
                },
                input: None,
            });
            Vec::new()
        }
        SurfaceNode::SafeReturn { id, target } => {
            interactions.push(TuiInteraction {
                id: Some(id.as_str().to_owned()),
                label: format!("返回 {target}"),
                kind: "safe-return",
                input: None,
            });
            Vec::new()
        }
        SurfaceNode::Region { content, .. } | SurfaceNode::Replace { content, .. } => {
            render_content(content, interactions)
        }
    }
}

/// Text 是一个可见文本片段；HardBreak 已是相邻片段之间的独立协议节点。
fn visible_lines(text: String) -> Vec<String> {
    vec![text]
}

/// 按语义样式包裹标记符，结构性标题加粗下划线，并按 color 梯度染色；color 为 0 时不染色。
fn styled(
    mut text: String,
    styles: &[TextStyle],
    color: TextColor,
    heading: Option<HeadingLevel>,
) -> String {
    for style in styles.iter().rev() {
        text = match style {
            TextStyle::Strong => format!("**{text}**"),
            TextStyle::Emphasis => format!("*{text}*"),
            TextStyle::Code => format!("`{text}`"),
            TextStyle::Inserted => format!("++{text}++"),
            TextStyle::Deleted => format!("~~{text}~~"),
            _ => text,
        };
    }
    if heading.is_some() {
        text = format!("\x1b[1;4m{text}\x1b[0m");
    }
    palette_rgb(color.index())
        .map(|(r, g, b)| format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m"))
        .unwrap_or(text)
}

/// 64 级色阶 → RGB（灰阶 0-7：白 1 → 黑 7；光谱 8-63：红 8 → 橙 16 → 黄 24 → 绿 32 → 蓝 40 → 紫 48 → 深紫 63）；
/// 0 不染色，由终端默认前景呈现。
fn palette_rgb(index: u8) -> Option<(u8, u8, u8)> {
    if index == 0 {
        return None;
    }
    const STOPS: [(u8, (u8, u8, u8)); 15] = [
        (1, (0xff, 0xff, 0xff)),  // 白
        (2, (0xe5, 0xe5, 0xe5)),  // 亮灰
        (3, (0xc9, 0xc9, 0xc9)),  // 浅灰
        (4, (0x8a, 0x8a, 0x8a)),  // 灰
        (5, (0x55, 0x55, 0x55)),  // 深灰
        (6, (0x32, 0x32, 0x32)),  // 暗灰
        (7, (0x00, 0x00, 0x00)),  // 黑
        (8, (0xff, 0x5a, 0x5a)),  // 红
        (16, (0xff, 0x9e, 0x45)), // 橙
        (24, (0xf2, 0xc9, 0x4c)), // 黄
        (32, (0x52, 0xc8, 0x78)), // 绿
        (40, (0x4f, 0xa3, 0xff)), // 蓝
        (48, (0xa7, 0x8b, 0xfa)), // 紫
        (56, (0x7c, 0x3a, 0xed)), // 深紫
        (63, (0x58, 0x1c, 0x87)), // 光谱终点
    ];
    let position: f64 = f64::from(index);
    for window in STOPS.windows(2) {
        let (start_index, start) = window[0];
        let (end_index, end) = window[1];
        if position <= f64::from(end_index) {
            let span: f64 = f64::from(end_index - start_index);
            let t: f64 = (position - f64::from(start_index)) / span;
            let lerp =
                |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as u8;
            return Some((
                lerp(start.0, end.0),
                lerp(start.1, end.1),
                lerp(start.2, end.2),
            ));
        }
    }
    None
}

/// TextValue 转 Unicode 字符串；非 Unicode 文本给占位。
fn unicode(value: &narrava_loom_core::expression::value::TextValue) -> String {
    value
        .to_unicode_string()
        .unwrap_or_else(|| String::from("<非 Unicode 文本>"))
}

fn is_standard_region(region: &str) -> bool {
    matches!(
        region,
        "header" | "main" | "footer" | "bar" | "bar-stowed" | "dialog"
    )
}

#[cfg(test)]
mod tests {
    use narrava_loom_core::{
        expression::value::TextValue,
        semantic::{RegionId, TextColor},
    };
    use narrava_loom_protocol::{Surface, SurfaceKey, SurfaceNode, SurfaceTarget};

    use super::{
        TuiCommand, TuiCommandError, TuiFrame, TuiInput, TuiInteraction, TuiOperation, TuiRenderer,
        run_terminal,
    };

    /// Region 与 Replace（按 key）就地更新对应终端区域，交互被收集进帧。
    #[test]
    fn region_and_key_replacements_update_terminal_surfaces() {
        let mut main = Surface::default();
        main.push_keyed(
            SurfaceKey::parse("status").unwrap(),
            SurfaceNode::Container {
                content: Surface::from_nodes(vec![SurfaceNode::Text(TextValue::from("旧状态"))]),
            },
        )
        .unwrap();
        main.push(SurfaceNode::Replace {
            target: SurfaceTarget::Key(SurfaceKey::parse("status").unwrap()),
            content: Surface::from_nodes(vec![
                SurfaceNode::Text(TextValue::from("新状态")),
                SurfaceNode::Navigation {
                    id: narrava_loom_core::semantic::InteractionId::parse("status:continue")
                        .unwrap(),
                    label: TextValue::from("继续"),
                    target: String::from("Next"),
                    role: narrava_loom_core::semantic::NavigationRole::Link,
                },
            ]),
        });
        main.push(SurfaceNode::Region {
            region: RegionId::header(),
            content: Surface::from_nodes(vec![SurfaceNode::Text(TextValue::from("标题"))]),
        });

        let frame = TuiRenderer::default().render("Start", &main);

        assert_eq!(frame.current, "Start");
        assert_eq!(frame.header, ["标题"]);
        assert_eq!(frame.main, ["新状态"]);
        assert_eq!(frame.interactions.len(), 1);
        assert_eq!(frame.interactions[0].label, "继续");
    }

    /// delay 文本先停放在 `delayed`，超过延迟后 `render_at` 才让其进入正文。
    #[test]
    fn styled_text_with_delay_is_parked_then_revealed() {
        let output = Surface::from_nodes(vec![
            SurfaceNode::StyledText {
                text: TextValue::from("立即显示"),
                styles: Vec::new(),
                color: TextColor::DEFAULT,
                delay: None,
                heading: None,
            },
            SurfaceNode::StyledText {
                text: TextValue::from("两秒后出现"),
                styles: Vec::new(),
                color: TextColor::DEFAULT,
                delay: Some(2000),
                heading: None,
            },
        ]);

        let mut renderer = TuiRenderer::default();
        let now = renderer.render("Delay", &output);
        assert_eq!(now.main, ["立即显示"], "未到延迟的文本不应出现在当前帧");
        assert_eq!(now.delayed.len(), 1);
        assert_eq!(now.delayed[0].region, "main");
        assert_eq!(now.delayed[0].delay_ms, 2000);
        assert_eq!(now.delayed[0].lines, ["两秒后出现"]);

        let later = renderer.render_at("Delay", &output, 2500);
        assert_eq!(
            later.main,
            ["立即显示", "两秒后出现"],
            "超过延迟后应进入正文"
        );
        assert!(later.delayed.is_empty());
    }

    /// 结构化 HardBreak 把相邻文本保持为两条终端行。
    #[test]
    fn explicit_line_break_becomes_two_terminal_lines() {
        let output = Surface::from_nodes(vec![
            SurfaceNode::Text(TextValue::from("第一行")),
            SurfaceNode::HardBreak,
            SurfaceNode::Text(TextValue::from("第二行")),
        ]);

        let frame = TuiRenderer::default().render("Break", &output);

        assert_eq!(frame.main, ["第一行", "第二行"]);
    }

    #[test]
    fn custom_region_falls_back_without_losing_content() {
        let output = Surface::from_nodes(vec![SurfaceNode::Region {
            region: RegionId::parse("hud").unwrap(),
            content: Surface::from_nodes(vec![SurfaceNode::Text(TextValue::from("状态"))]),
        }]);

        let frame = TuiRenderer::default().render("Custom", &output);

        assert_eq!(frame.custom.get("hud").unwrap(), &["状态"]);
    }

    /// 玩家使用一基序号；文本框必须显式使用 set，避免直接选择时误清空内容。
    #[test]
    fn terminal_commands_resolve_against_current_frame() {
        let frame = TuiFrame {
            interactions: vec![
                TuiInteraction {
                    id: Some(String::from("route:quiet")),
                    label: String::from("( )"),
                    kind: "radiobutton",
                    input: Some(TuiInput::Radio {
                        value: narrava_loom_protocol::SurfaceValue::Text(String::from("quiet")),
                        selected: false,
                    }),
                },
                TuiInteraction {
                    id: Some(String::from("name")),
                    label: String::from("[旅人]"),
                    kind: "textbox",
                    input: Some(TuiInput::Text {
                        value: String::from("旅人"),
                    }),
                },
            ],
            ..TuiFrame::default()
        };

        assert_eq!(
            TuiCommand::parse("1").unwrap().resolve(&frame).unwrap(),
            TuiOperation::Input {
                id: String::from("route:quiet"),
                value: narrava_loom_protocol::SurfaceValue::Text(String::from("quiet")),
            }
        );
        assert_eq!(
            TuiCommand::parse("set 2 游侠")
                .unwrap()
                .resolve(&frame)
                .unwrap(),
            TuiOperation::Input {
                id: String::from("name"),
                value: narrava_loom_protocol::SurfaceValue::Text(String::from("游侠")),
            }
        );
        assert_eq!(
            TuiCommand::parse("2").unwrap().resolve(&frame),
            Err(TuiCommandError::TextNeedsValue)
        );
        assert_eq!(TuiCommand::parse("0"), Err(TuiCommandError::ZeroIndex));
    }

    /// 输入循环能恢复错误、显示帮助并继续处理后续有效动作。
    #[test]
    fn terminal_loop_is_operable_with_plain_stdin_and_stdout() {
        let frame = TuiFrame {
            current: String::from("Start"),
            main: vec![String::from("请选择。")],
            interactions: vec![TuiInteraction {
                id: Some(String::from("go")),
                label: String::from("继续"),
                kind: "link",
                input: None,
            }],
            ..TuiFrame::default()
        };
        let mut input = std::io::Cursor::new(b"wat\nhelp\n1\nquit\n".to_vec());
        let mut output = Vec::new();
        let mut activated = false;

        run_terminal(&mut input, &mut output, frame, |operation| {
            if operation
                == (TuiOperation::Activate {
                    id: String::from("go"),
                })
            {
                activated = true;
            }
            Ok::<Option<TuiFrame>, &str>(None)
        })
        .unwrap();

        let printed = String::from_utf8(output).unwrap();
        assert!(printed.contains("== Start =="));
        assert!(printed.contains("未知命令"));
        assert!(printed.contains("set <序号> <文字>"));
        assert!(activated);
    }
}

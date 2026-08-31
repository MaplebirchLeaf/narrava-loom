//! Core/Protocol Surface 到终端帧的呈现适配。
//!
//! 块缓冲、同行 panel 拼接和样式降级共同决定一次 Surface 到行文本的转换，
//! 因此保留在同一紧密算法模块内，不形成平台无关布局契约。

use crate::{TuiInput, TuiInteraction};
use narrava_loom_core::semantic::{
    ContainerFlow, ContainerPresentation, HeadingLevel, NavigationRole, RegionId, TextColor,
    TextStyle,
};
use narrava_loom_protocol::{
    ContainerFlowDto, ContainerPresentationDto, HostNodeDto, HostReplaceTargetDto, HostUpdateDto,
};
use narrava_loom_script::protocol_adapter::{
    Surface, SurfaceAction, SurfaceInputKind, SurfaceNode, SurfaceTarget, SurfaceValue,
};
use std::collections::{BTreeMap, BTreeSet};

/// TUI 对 Protocol 内容分组语义的本地映射。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TuiBlockPresentation {
    #[default]
    Plain,
    Panel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TuiBlockFlow {
    #[default]
    Stack,
    Row,
}

impl From<ContainerFlow> for TuiBlockFlow {
    fn from(value: ContainerFlow) -> Self {
        match value {
            ContainerFlow::Stack => Self::Stack,
            ContainerFlow::Row => Self::Row,
        }
    }
}

impl From<ContainerFlowDto> for TuiBlockFlow {
    fn from(value: ContainerFlowDto) -> Self {
        match value {
            ContainerFlowDto::Stack => Self::Stack,
            ContainerFlowDto::Row => Self::Row,
        }
    }
}

impl TuiBlockPresentation {
    fn render(self, lines: Vec<String>) -> Vec<String> {
        match self {
            Self::Plain => lines,
            Self::Panel => panel_lines(lines),
        }
    }
}

impl From<ContainerPresentation> for TuiBlockPresentation {
    fn from(value: ContainerPresentation) -> Self {
        match value {
            ContainerPresentation::Plain => Self::Plain,
            ContainerPresentation::Panel => Self::Panel,
        }
    }
}

impl From<ContainerPresentationDto> for TuiBlockPresentation {
    fn from(value: ContainerPresentationDto) -> Self {
        match value {
            ContainerPresentationDto::Plain => Self::Plain,
            ContainerPresentationDto::Panel => Self::Panel,
        }
    }
}

/// 区域内的一个行文本块；保留 key 与表现语义，使 Replace 只更新内容。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TuiBlock {
    key: Option<String>,
    presentation: TuiBlockPresentation,
    flow: TuiBlockFlow,
    lines: Vec<String>,
    page_title: Option<String>,
    inline: bool,
}

/// 单个区域下按出现顺序排列的文本块集合。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TuiSurface {
    blocks: Vec<TuiBlock>,
}

impl TuiSurface {
    /// 按块顺序输出；连续 Panel 横向拼成同一组终端行。
    fn lines(&self) -> Vec<String> {
        Self::lines_for(&self.blocks)
    }

    fn lines_for(blocks: &[TuiBlock]) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut index: usize = 0;
        let mut inline_open: bool = false;
        while index < blocks.len() {
            if blocks[index].presentation == TuiBlockPresentation::Plain
                || blocks[index].flow == TuiBlockFlow::Stack
            {
                if blocks[index].inline {
                    for fragment in &blocks[index].lines {
                        if inline_open {
                            if let Some(line) = lines.last_mut() {
                                line.push_str(fragment);
                            }
                        } else {
                            lines.push(fragment.clone());
                        }
                        inline_open = true;
                    }
                } else {
                    lines.extend(blocks[index].lines.iter().cloned());
                    inline_open = false;
                }
                index += 1;
                continue;
            }
            let start: usize = index;
            while index < blocks.len()
                && blocks[index].presentation == TuiBlockPresentation::Panel
                && blocks[index].flow == TuiBlockFlow::Row
            {
                index += 1;
            }
            lines.extend(join_panel_blocks(&blocks[start..index]));
            inline_open = false;
        }
        lines
    }

    /// Tauri 用页签切换 Dialog；TUI 没有页签，因此把每个标题页呈现为独立面板。
    fn dialog_lines(&self) -> Vec<String> {
        let mut pages: Vec<&[TuiBlock]> = Vec::new();
        let mut start: usize = 0;
        for index in 1..self.blocks.len() {
            if self.blocks[index].page_title.is_some() {
                pages.push(&self.blocks[start..index]);
                start = index;
            }
        }
        if start < self.blocks.len() {
            pages.push(&self.blocks[start..]);
        }
        let mut lines: Vec<String> = Vec::new();
        for page in pages {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.extend(panel_lines(Self::lines_for(page)));
        }
        lines
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
        block.lines = block.presentation.render(lines.to_vec());
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

/// TUI 侧栏当前显示状态；两套 Region 内容始终保留，但只呈现其中一套。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TuiSidebarMode {
    #[default]
    Expanded,
    Stowed,
}

impl TuiSidebarMode {
    pub(super) fn toggled(self) -> Self {
        match self {
            Self::Expanded => Self::Stowed,
            Self::Stowed => Self::Expanded,
        }
    }
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
    /// 决定当前呈现 `bar` 还是 `bar-stowed`；两者不会同时显示。
    pub sidebar_mode: TuiSidebarMode,
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
    /// TUI 本地侧栏状态，不进入 Core 或 Protocol。
    sidebar_mode: TuiSidebarMode,
}

impl TuiRenderer {
    /// 在两套作者提供的侧栏内容之间切换；不会修改 Runtime Surface。
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_mode = self.sidebar_mode.toggled();
    }

    /// 直接渲染 Runtime Protocol DTO；Native Host 不需要回借 Core `HostUpdate`。
    pub fn render_update(&mut self, update: &HostUpdateDto) -> TuiFrame {
        self.surfaces.clear();
        self.interactions.clear();
        self.delayed.clear();
        self.render_dto_nodes("main", &update.nodes, 0);
        self.frame(&update.current)
    }

    fn render_dto_nodes(&mut self, region: &str, nodes: &[HostNodeDto], elapsed_ms: u64) {
        let mut interaction_group: String = region_group(region);
        for node in nodes {
            if region == "dialog"
                && let HostNodeDto::StyledText {
                    text,
                    heading: Some(_),
                    ..
                } = node
            {
                interaction_group = format!("弹窗 · {}", text.trim());
            }
            let interaction_start: usize = self.interactions.len();
            match node {
                HostNodeDto::Region { region, nodes, .. } => {
                    self.render_dto_nodes(region, nodes, elapsed_ms);
                }
                HostNodeDto::Replace { target, nodes, .. } => {
                    let lines: Vec<String> = render_dto_content(nodes, &mut self.interactions);
                    match target {
                        HostReplaceTargetDto::Region(target) => {
                            self.surfaces.entry(target.clone()).or_default().blocks =
                                vec![TuiBlock {
                                    key: None,
                                    presentation: TuiBlockPresentation::Plain,
                                    flow: TuiBlockFlow::Stack,
                                    lines,
                                    page_title: None,
                                    inline: false,
                                }];
                        }
                        HostReplaceTargetDto::Key(target) => {
                            for surface in self.surfaces.values_mut() {
                                if surface.replace_key(target, &lines) {
                                    break;
                                }
                            }
                        }
                    }
                }
                HostNodeDto::StyledText {
                    delay: Some(delay), ..
                } if *delay > elapsed_ms => {
                    let lines: Vec<String> = render_dto_node(node, &mut self.interactions);
                    if !lines.is_empty() {
                        self.delayed.push(TuiDelayedText {
                            region: region.to_owned(),
                            lines,
                            delay_ms: *delay,
                        });
                    }
                }
                _ => {
                    let (lines, presentation, flow): (
                        Vec<String>,
                        TuiBlockPresentation,
                        TuiBlockFlow,
                    ) = match node {
                        HostNodeDto::Container {
                            presentation,
                            flow,
                            nodes,
                            ..
                        } => {
                            let presentation: TuiBlockPresentation = (*presentation).into();
                            let flow: TuiBlockFlow = (*flow).into();
                            let lines: Vec<String> =
                                render_dto_content(nodes, &mut self.interactions);
                            (presentation.render(lines), presentation, flow)
                        }
                        _ => (
                            render_dto_node(node, &mut self.interactions),
                            TuiBlockPresentation::Plain,
                            TuiBlockFlow::Stack,
                        ),
                    };
                    if !lines.is_empty()
                        || matches!(
                            node,
                            HostNodeDto::Container { .. } | HostNodeDto::HardBreak { .. }
                        )
                    {
                        self.surfaces
                            .entry(region.to_owned())
                            .or_default()
                            .blocks
                            .push(TuiBlock {
                                key: Some(dto_key(node).to_owned()),
                                presentation,
                                flow,
                                lines,
                                page_title: dto_page_title(node),
                                inline: matches!(
                                    node,
                                    HostNodeDto::Text { .. } | HostNodeDto::StyledText { .. }
                                ),
                            });
                    }
                }
            }
            self.label_new_interactions(interaction_start, &interaction_group);
        }
    }

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
        let mut interaction_group: String = region_group(region.as_str());
        for (index, node) in output.nodes().iter().enumerate() {
            if region == RegionId::dialog()
                && let SurfaceNode::StyledText {
                    text,
                    heading: Some(_),
                    ..
                } = node
            {
                interaction_group = format!("弹窗 · {}", unicode(text).trim());
            }
            let interaction_start: usize = self.interactions.len();
            let key = output.key(index).map(|key| key.as_str().to_owned());
            match node {
                SurfaceNode::Region { region, content } => {
                    self.render_output(region.clone(), content, elapsed_ms)
                }
                SurfaceNode::Replace { target, content } => {
                    let lines = render_content(content, &mut self.interactions);
                    match target {
                        SurfaceTarget::Region(target) => {
                            self.surface_mut(target).blocks = vec![TuiBlock {
                                key: None,
                                presentation: TuiBlockPresentation::Plain,
                                flow: TuiBlockFlow::Stack,
                                lines,
                                page_title: None,
                                inline: false,
                            }];
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
                    let (lines, presentation, flow): (
                        Vec<String>,
                        TuiBlockPresentation,
                        TuiBlockFlow,
                    ) = match node {
                        SurfaceNode::Container {
                            presentation,
                            flow,
                            content,
                        } => {
                            let presentation: TuiBlockPresentation = (*presentation).into();
                            let flow: TuiBlockFlow = (*flow).into();
                            let lines: Vec<String> =
                                render_content(content, &mut self.interactions);
                            (presentation.render(lines), presentation, flow)
                        }
                        _ => (
                            render_node(node, &mut self.interactions),
                            TuiBlockPresentation::Plain,
                            TuiBlockFlow::Stack,
                        ),
                    };
                    if !lines.is_empty()
                        || matches!(node, SurfaceNode::Container { .. } | SurfaceNode::HardBreak)
                    {
                        self.surface_mut(&region).blocks.push(TuiBlock {
                            key,
                            presentation,
                            flow,
                            lines,
                            page_title: surface_page_title(node),
                            inline: matches!(
                                node,
                                SurfaceNode::Text(_) | SurfaceNode::StyledText { .. }
                            ),
                        });
                    }
                }
            }
            self.label_new_interactions(interaction_start, &interaction_group);
        }
    }

    fn label_new_interactions(&mut self, start: usize, group: &str) {
        for interaction in &mut self.interactions[start..] {
            if interaction.group.is_empty() {
                interaction.group = group.to_owned();
            }
        }
    }

    /// 取（必要时创建）某区域的文本面。
    fn surface_mut(&mut self, region: &RegionId) -> &mut TuiSurface {
        self.surfaces.entry(region.as_str().to_owned()).or_default()
    }

    /// 把当前缓冲整理为一帧可打印画面。
    fn frame(&self, current: &str) -> TuiFrame {
        let mut seen_interactions: BTreeSet<&str> = BTreeSet::new();
        let hidden_sidebar_group: &str = match self.sidebar_mode {
            TuiSidebarMode::Expanded => "收起侧栏",
            TuiSidebarMode::Stowed => "侧栏",
        };
        let interactions: Vec<TuiInteraction> = self
            .interactions
            .iter()
            .filter(|interaction| interaction.group != hidden_sidebar_group)
            .filter(|interaction| {
                interaction
                    .id
                    .as_deref()
                    .is_none_or(|id| seen_interactions.insert(id))
            })
            .cloned()
            .collect();
        TuiFrame {
            current: current.to_owned(),
            header: self.lines(RegionId::header()),
            main: self.lines(RegionId::main()),
            footer: self.lines(RegionId::footer()),
            bar: self.lines(RegionId::bar()),
            bar_stowed: self.lines(RegionId::bar_stowed()),
            sidebar_mode: self.sidebar_mode,
            dialog: self
                .surfaces
                .get(RegionId::dialog().as_str())
                .map_or_else(Vec::new, TuiSurface::dialog_lines),
            custom: self
                .surfaces
                .iter()
                .filter(|(region, _)| !is_standard_region(region))
                .map(|(region, surface)| (region.clone(), surface.lines()))
                .collect(),
            interactions,
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

fn dto_key(node: &HostNodeDto) -> &str {
    match node {
        HostNodeDto::Text { key, .. }
        | HostNodeDto::HardBreak { key }
        | HostNodeDto::StyledText { key, .. }
        | HostNodeDto::Image { key, .. }
        | HostNodeDto::Region { key, .. }
        | HostNodeDto::Container { key, .. }
        | HostNodeDto::Component { key, .. }
        | HostNodeDto::Replace { key, .. }
        | HostNodeDto::Action { key, .. }
        | HostNodeDto::Checkbox { key, .. }
        | HostNodeDto::Radiobutton { key, .. }
        | HostNodeDto::Textbox { key, .. }
        | HostNodeDto::Navigation { key, .. }
        | HostNodeDto::Button { key, .. }
        | HostNodeDto::SafeReturn { key, .. } => key,
    }
}

fn render_dto_content(
    nodes: &[HostNodeDto],
    interactions: &mut Vec<TuiInteraction>,
) -> Vec<String> {
    nodes
        .iter()
        .flat_map(|node| render_dto_node(node, interactions))
        .collect()
}

fn render_dto_node(node: &HostNodeDto, interactions: &mut Vec<TuiInteraction>) -> Vec<String> {
    match node {
        HostNodeDto::Text { text, .. } => visible_lines(text.clone()),
        HostNodeDto::HardBreak { .. } => Vec::new(),
        HostNodeDto::StyledText {
            text,
            styles,
            color,
            heading,
            ..
        } => visible_lines(styled_dto(text.clone(), styles, *color, *heading)),
        HostNodeDto::Image {
            resource,
            alt,
            caption,
            ..
        } => vec![format!(
            "[图像: {alt} <{resource}>{}]",
            caption
                .as_ref()
                .map(|value| format!(" — {value}"))
                .unwrap_or_default()
        )],
        HostNodeDto::Container {
            presentation,
            nodes,
            ..
        } => {
            let presentation: TuiBlockPresentation = (*presentation).into();
            presentation.render(render_dto_content(nodes, interactions))
        }
        HostNodeDto::Component { fallback, .. }
        | HostNodeDto::Region {
            nodes: fallback, ..
        }
        | HostNodeDto::Replace {
            nodes: fallback, ..
        } => render_dto_content(fallback, interactions),
        HostNodeDto::Action { label, action, .. } => {
            interactions.push(TuiInteraction {
                group: String::new(),
                id: None,
                label: label.clone(),
                kind: match action.as_str() {
                    "dismiss" => "dismiss",
                    _ => "action",
                },
                input: None,
            });
            Vec::new()
        }
        HostNodeDto::Checkbox {
            id,
            unchecked,
            checked,
            selected,
            ..
        } => {
            interactions.push(TuiInteraction {
                group: String::new(),
                id: Some(id.clone()),
                label: if *selected { "[x]" } else { "[ ]" }.to_owned(),
                kind: "checkbox",
                input: Some(TuiInput::Checkbox {
                    unchecked: dto_surface_value(unchecked),
                    checked: dto_surface_value(checked),
                    selected: *selected,
                }),
            });
            Vec::new()
        }
        HostNodeDto::Radiobutton {
            id,
            value,
            selected,
            ..
        } => {
            interactions.push(TuiInteraction {
                group: String::new(),
                id: Some(id.clone()),
                label: if *selected { "(o)" } else { "( )" }.to_owned(),
                kind: "radiobutton",
                input: Some(TuiInput::Radio {
                    value: dto_surface_value(value),
                    selected: *selected,
                }),
            });
            Vec::new()
        }
        HostNodeDto::Textbox { id, value, .. } => {
            interactions.push(TuiInteraction {
                group: String::new(),
                id: Some(id.clone()),
                label: format!("[{value}]"),
                kind: "textbox",
                input: Some(TuiInput::Text {
                    value: value.clone(),
                }),
            });
            Vec::new()
        }
        HostNodeDto::Navigation { id, label, .. } => {
            push_dto_action(interactions, id, label, "link")
        }
        HostNodeDto::Button { id, label, .. } => push_dto_action(interactions, id, label, "button"),
        HostNodeDto::SafeReturn { id, target, .. } => {
            push_dto_action(interactions, id, &format!("返回 {target}"), "safe-return")
        }
    }
}

pub(super) fn panel_lines(lines: Vec<String>) -> Vec<String> {
    let mut lines: Vec<String> = lines
        .into_iter()
        .flat_map(|line| line.split('\n').map(str::to_owned).collect::<Vec<_>>())
        .collect();
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    let width = lines
        .iter()
        .map(|line| terminal_width(line))
        .max()
        .unwrap_or(0);
    let mut framed = Vec::with_capacity(lines.len() + 2);
    framed.push(format!("┌{}┐", "─".repeat(width + 2)));
    for line in lines {
        let padding = width.saturating_sub(terminal_width(line.as_str()));
        framed.push(format!("│ {line}{} │", " ".repeat(padding)));
    }
    framed.push(format!("└{}┘", "─".repeat(width + 2)));
    framed
}

fn join_panel_blocks(blocks: &[TuiBlock]) -> Vec<String> {
    let height: usize = blocks
        .iter()
        .map(|block| block.lines.len())
        .max()
        .unwrap_or(0);
    (0..height)
        .map(|row| {
            blocks
                .iter()
                .map(|block| {
                    let width: usize = block
                        .lines
                        .iter()
                        .map(|line| terminal_width(line))
                        .max()
                        .unwrap_or(0);
                    block
                        .lines
                        .get(row)
                        .cloned()
                        .unwrap_or_else(|| " ".repeat(width))
                })
                .collect::<Vec<String>>()
                .join(" ")
        })
        .collect()
}

fn terminal_width(line: &str) -> usize {
    use unicode_width::UnicodeWidthChar;

    let mut width = 0;
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' && matches!(characters.next(), Some('[')) {
            for control in characters.by_ref() {
                if control.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        width += UnicodeWidthChar::width(character).unwrap_or(0);
    }
    width
}

fn push_dto_action(
    interactions: &mut Vec<TuiInteraction>,
    id: &str,
    label: &str,
    kind: &'static str,
) -> Vec<String> {
    interactions.push(TuiInteraction {
        group: String::new(),
        id: Some(id.to_owned()),
        label: label.to_owned(),
        kind,
        input: None,
    });
    Vec::new()
}

fn dto_surface_value(value: &serde_json::Value) -> SurfaceValue {
    match value {
        serde_json::Value::Null => SurfaceValue::Null,
        serde_json::Value::Bool(value) => SurfaceValue::Boolean(*value),
        serde_json::Value::Number(value) => value
            .as_f64()
            .map(SurfaceValue::Number)
            .unwrap_or(SurfaceValue::Null),
        serde_json::Value::String(value) => SurfaceValue::Text(value.clone()),
        _ => SurfaceValue::Null,
    }
}

fn styled_dto(mut text: String, styles: &[String], color: u8, heading: Option<u8>) -> String {
    for style in styles.iter().rev() {
        text = match style.as_str() {
            "strong" => format!("**{text}**"),
            "emphasis" => format!("*{text}*"),
            "code" => format!("`{text}`"),
            "inserted" => format!("++{text}++"),
            "deleted" => format!("~~{text}~~"),
            _ => text,
        };
    }
    if heading.is_some() {
        text = format!("\x1b[1;4m{text}\x1b[0m");
    }
    palette_rgb(color)
        .map(|(r, g, b)| format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m"))
        .unwrap_or(text)
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
        SurfaceNode::Container {
            presentation,
            content,
            ..
        } => {
            let presentation: TuiBlockPresentation = (*presentation).into();
            presentation.render(render_content(content, interactions))
        }
        SurfaceNode::Action { label, action, .. } => {
            interactions.push(TuiInteraction {
                group: String::new(),
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
                group: String::new(),
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
                group: String::new(),
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
                group: String::new(),
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

fn region_group(region: &str) -> String {
    match region {
        "main" => String::from("正文"),
        "header" => String::from("页眉"),
        "footer" => String::from("页脚"),
        "bar" => String::from("侧栏"),
        "bar-stowed" => String::from("收起侧栏"),
        "dialog" => String::from("弹窗"),
        custom => format!("区域 · {custom}"),
    }
}

fn dto_page_title(node: &HostNodeDto) -> Option<String> {
    match node {
        HostNodeDto::StyledText {
            text,
            heading: Some(_),
            ..
        } => Some(text.trim().to_owned()),
        _ => None,
    }
}

fn surface_page_title(node: &SurfaceNode) -> Option<String> {
    match node {
        SurfaceNode::StyledText {
            text,
            heading: Some(_),
            ..
        } => Some(unicode(text).trim().to_owned()),
        _ => None,
    }
}

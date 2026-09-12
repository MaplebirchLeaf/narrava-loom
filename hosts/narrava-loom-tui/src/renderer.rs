//! 终端帧与区域缓冲；节点适配和终端布局由私有子模块负责。

mod layout;
mod protocol;
mod semantic;

pub(super) use layout::panel_lines;
use layout::{decorate_text, join_blocks, render_meter, visible_lines};

use crate::{TuiInput, TuiInteraction};
use narrava_loom_core::semantic::{
    ContainerFlow, ContainerPresentation, HeadingLevel, NavigationRole, RegionId, TextColor,
    TextStyle,
};
use narrava_loom_core::semantic::{
    SemanticAction, SemanticInputKind, SemanticNode, SemanticOutput, SemanticTarget, SemanticValue,
};
use narrava_loom_protocol::{
    ContainerFlowDto, ContainerPresentationDto, HostNodeDto, HostReplaceTargetDto, HostUpdateDto,
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

    /// 收起侧栏把每个顶层块映射为一个紧邻方格，不把这种 Host 布局泄漏到 Protocol。
    fn stowed_lines(&self) -> Vec<String> {
        let cells: Vec<TuiBlock> = self
            .blocks
            .iter()
            .filter(|block: &&TuiBlock| !block.lines.is_empty())
            .map(|block: &TuiBlock| TuiBlock {
                lines: if block.presentation == TuiBlockPresentation::Panel {
                    block.lines.clone()
                } else {
                    panel_lines(block.lines.clone())
                },
                ..TuiBlock::default()
            })
            .collect();
        join_blocks(&cells, "")
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
            lines.extend(join_blocks(&blocks[start..index], " "));
            inline_open = false;
        }
        lines
    }

    /// 旧式 dialog Region 仍可显示普通单页；分页仅由显式 Dialog 指定。
    fn dialog_lines(&self) -> Vec<String> {
        panel_lines(self.lines())
    }

    fn dialog_pages(&self) -> Vec<TuiDialogPage> {
        vec![TuiDialogPage {
            title: String::from("消息"),
            group: String::from("弹窗"),
            lines: self.lines(),
        }]
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

/// 完整屏幕 TUI 使用的单个 Dialog 页面；边框由 Host 布局层统一绘制。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TuiDialogPage {
    pub title: String,
    pub group: String,
    pub lines: Vec<String>,
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
    /// Dialog 的结构化页面；避免完整屏幕层二次解析字符边框。
    pub dialog_pages: Vec<TuiDialogPage>,
    pub dialog_key: Option<String>,
    pub dialog_initial: String,
    /// Host 不认识的开放区域；按逻辑 RegionId 原名保留，绝不静默丢弃。
    pub custom: BTreeMap<String, Vec<String>>,
    /// 玩家可触发的动作列表。
    pub interactions: Vec<TuiInteraction>,
    /// `delay > 0` 且未到时刻的文本；消费方按其最小 `delay_ms` 安排下次 `render_at`。
    pub delayed: Vec<TuiDelayedText>,
}

/// 每次渲染从输出重建帧，保留本地侧栏选择；终端输入与屏幕管理由消费方负责。
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
    dialog_key: Option<String>,
    dialog_initial: String,
    page_regions: Vec<(String, String)>,
}

impl TuiRenderer {
    /// 在两套作者提供的侧栏内容之间切换；不会修改 Runtime SemanticOutput。
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_mode = self.sidebar_mode.toggled();
    }

    /// 直接渲染 Runtime Protocol DTO；Native Host 不需要回借 Core `HostUpdate`。
    pub fn render_update(&mut self, update: &HostUpdateDto) -> TuiFrame {
        self.clear_frame();
        self.render_dto_nodes("main", &update.nodes, 0);
        self.frame(&update.current)
    }

    /// 渲染当前时刻（elapsed = 0）的帧；`delay > 0` 的文本停放在 `frame.delayed`。
    pub fn render(&mut self, current: &str, output: &SemanticOutput) -> TuiFrame {
        self.render_at(current, output, 0)
    }

    /// 渲染经过 `elapsed_ms` 毫秒后的帧：`delay <= elapsed_ms` 的文本进入对应区域，
    /// 其余仍停放在 `frame.delayed` 供消费方继续等待。
    pub fn render_at(
        &mut self,
        current: &str,
        output: &SemanticOutput,
        elapsed_ms: u64,
    ) -> TuiFrame {
        self.clear_frame();
        self.render_output(RegionId::main(), output, elapsed_ms);
        self.frame(current)
    }

    fn clear_frame(&mut self) {
        self.surfaces.clear();
        self.dialog_key = None;
        self.dialog_initial.clear();
        self.page_regions.clear();
        self.interactions.clear();
        self.delayed.clear();
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
            bar_stowed: self
                .surfaces
                .get(RegionId::bar_stowed().as_str())
                .map_or_else(Vec::new, TuiSurface::stowed_lines),
            sidebar_mode: self.sidebar_mode,
            dialog: if self.dialog_key.is_some() {
                self.page_regions
                    .iter()
                    .flat_map(|(title, region)| {
                        let mut lines: Vec<String> = vec![title.clone()];
                        lines.extend(
                            self.surfaces
                                .get(region)
                                .map_or_else(Vec::new, TuiSurface::lines),
                        );
                        panel_lines(lines)
                    })
                    .collect()
            } else {
                self.surfaces
                    .get(RegionId::dialog().as_str())
                    .map_or_else(Vec::new, TuiSurface::dialog_lines)
            },
            dialog_key: self.dialog_key.clone(),
            dialog_initial: self.dialog_initial.clone(),
            dialog_pages: if self.dialog_key.is_some() {
                self.page_regions
                    .iter()
                    .map(|(title, region)| TuiDialogPage {
                        title: title.clone(),
                        group: format!("弹窗 · {title}"),
                        lines: self
                            .surfaces
                            .get(region)
                            .map_or_else(Vec::new, TuiSurface::lines),
                    })
                    .collect()
            } else {
                self.surfaces
                    .get(RegionId::dialog().as_str())
                    .map_or_else(Vec::new, TuiSurface::dialog_pages)
            },
            custom: self
                .surfaces
                .iter()
                .filter(|(region, _)| {
                    !is_standard_region(region) && !region.starts_with("dialog-page:")
                })
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

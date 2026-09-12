//! 真实终端的完整屏幕界面与方向键焦点导航。

use std::{fmt, io};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use narrava_loom_protocol::HostDebugSnapshotDto;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{TuiDialogPage, TuiFrame, TuiInput, TuiInteraction, TuiOperation, TuiSidebarMode};

const ACCENT: Color = Color::Cyan;

/// 检查结果不替换故事帧；关闭覆盖层时保留原焦点与弹窗。
pub(crate) enum ScreenUpdate {
    Frame(Box<TuiFrame>),
    Inspect(Box<HostDebugSnapshotDto>),
}

#[derive(Debug)]
struct Inspection {
    lines: Vec<String>,
    scroll: u16,
}

#[derive(Debug, Default)]
pub(crate) struct ScreenState {
    focus: Option<usize>,
    main_scroll: u16,
    editor: Option<String>,
    status: String,
    dialog_dismissed: bool,
    dialog_key: Option<String>,
    dialog_page: usize,
    dialog_scroll: u16,
    inspection: Option<Inspection>,
}

impl ScreenState {
    pub(crate) fn open_inspection(&mut self, snapshot: &HostDebugSnapshotDto) {
        self.inspection = Some(Inspection {
            lines: crate::debug::snapshot_lines(snapshot),
            scroll: 0,
        });
    }

    /// 检查期间消费全部键盘输入，防止操作穿透到原故事帧。
    pub(crate) fn handle_inspection_key(&mut self, key: KeyEvent) -> bool {
        let Some(inspection) = self.inspection.as_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Esc | KeyCode::F(10) => self.inspection = None,
            KeyCode::Up => inspection.scroll = inspection.scroll.saturating_sub(1),
            KeyCode::Down => inspection.scroll = inspection.scroll.saturating_add(1),
            KeyCode::PageUp => inspection.scroll = inspection.scroll.saturating_sub(10),
            KeyCode::PageDown => inspection.scroll = inspection.scroll.saturating_add(10),
            KeyCode::Home => inspection.scroll = 0,
            _ => {}
        }
        true
    }

    fn normalize(&mut self, frame: &TuiFrame) {
        if self.dialog_key != frame.dialog_key {
            self.dialog_key.clone_from(&frame.dialog_key);
            self.dialog_dismissed = false;
            self.dialog_scroll = 0;
            self.dialog_page = frame
                .dialog_pages
                .iter()
                .position(|page| page.title == frame.dialog_initial)
                .unwrap_or(0);
        }
        self.dialog_page = self
            .dialog_page
            .min(frame.dialog_pages.len().saturating_sub(1));
        let indices: Vec<usize> = self.navigation_indices(frame);
        self.focus = match (self.focus, indices.is_empty()) {
            (_, true) => None,
            (Some(index), false) if indices.contains(&index) => Some(index),
            (_, false) => indices.first().copied(),
        };
    }

    fn navigation_indices(&self, frame: &TuiFrame) -> Vec<usize> {
        frame
            .interactions
            .iter()
            .enumerate()
            .filter_map(|(index, interaction)| {
                (if self.dialog_open(frame) {
                    frame
                        .dialog_pages
                        .get(self.dialog_page)
                        .is_some_and(|page| interaction.group == page.group)
                } else {
                    !interaction.group.starts_with("弹窗")
                })
                .then_some(index)
            })
            .collect()
    }

    fn dialog_open(&self, frame: &TuiFrame) -> bool {
        !self.dialog_dismissed && !frame.dialog_pages.is_empty()
    }

    fn action_indices(&self, frame: &TuiFrame) -> Vec<usize> {
        let indices: Vec<usize> = self.navigation_indices(frame);
        if !self.dialog_open(frame) {
            return indices;
        }
        let group: Option<&str> = self
            .focus
            .and_then(|index: usize| frame.interactions.get(index))
            .map(|interaction: &TuiInteraction| interaction.group.as_str());
        indices
            .into_iter()
            .filter(|index: &usize| {
                group.is_none_or(|group: &str| frame.interactions[*index].group == group)
            })
            .collect()
    }

    pub(crate) fn move_vertical(&mut self, frame: &TuiFrame, delta: isize) {
        self.normalize(frame);
        let Some(current) = self.focus else {
            return;
        };
        let group: &str = &frame.interactions[current].group;
        let indices: Vec<usize> = frame
            .interactions
            .iter()
            .enumerate()
            .filter_map(|(index, interaction)| (interaction.group == group).then_some(index))
            .collect();
        let position: usize = indices
            .iter()
            .position(|index: &usize| *index == current)
            .unwrap_or(0);
        let next: usize = (position as isize + delta).rem_euclid(indices.len() as isize) as usize;
        self.focus = indices.get(next).copied();
    }

    pub(crate) fn move_horizontal(&mut self, frame: &TuiFrame, delta: isize) {
        self.normalize(frame);
        if self.dialog_open(frame) {
            let indices: Vec<usize> = self.navigation_indices(frame);
            let row: usize = self
                .focus
                .and_then(|focus| indices.iter().position(|index| *index == focus))
                .unwrap_or(0);
            self.dialog_scroll = 0;
            self.dialog_page = (self.dialog_page as isize + delta)
                .rem_euclid(frame.dialog_pages.len() as isize)
                as usize;
            let indices: Vec<usize> = self.navigation_indices(frame);
            self.focus = indices.get(row).or_else(|| indices.last()).copied();
            self.normalize(frame);
            return;
        }
        let Some(current) = self.focus else {
            return;
        };
        let navigation_indices: Vec<usize> = self.navigation_indices(frame);
        let groups: Vec<&str> = navigation_indices
            .iter()
            .map(|index: &usize| &frame.interactions[*index])
            .fold(Vec::new(), |mut groups, item| {
                if groups.last().copied() != Some(item.group.as_str()) {
                    groups.push(item.group.as_str());
                }
                groups
            });
        let group_position: usize = groups
            .iter()
            .position(|group: &&str| *group == frame.interactions[current].group)
            .unwrap_or(0);
        let next_group: usize =
            (group_position as isize + delta).rem_euclid(groups.len() as isize) as usize;
        let row: usize = frame.interactions[..current]
            .iter()
            .rev()
            .take_while(|item: &&TuiInteraction| item.group == frame.interactions[current].group)
            .count();
        let candidates: Vec<usize> = self
            .navigation_indices(frame)
            .into_iter()
            .filter(|index: &usize| frame.interactions[*index].group == groups[next_group])
            .collect();
        self.focus = candidates.get(row).or_else(|| candidates.last()).copied();
    }

    fn cycle(&mut self, frame: &TuiFrame, delta: isize) {
        self.normalize(frame);
        let indices: Vec<usize> = self.navigation_indices(frame);
        if indices.is_empty() {
            return;
        }
        let current: usize = self.focus.unwrap_or(indices[0]);
        let position: usize = indices
            .iter()
            .position(|index: &usize| *index == current)
            .unwrap_or(0);
        let next: usize = (position as isize + delta).rem_euclid(indices.len() as isize) as usize;
        self.focus = Some(indices[next]);
    }

    #[cfg(test)]
    pub(crate) fn focus(&self) -> Option<usize> {
        self.focus
    }
}

pub(crate) fn run_screen<F, E>(mut frame: TuiFrame, mut dispatch: F) -> Result<(), HostScreenError>
where
    F: FnMut(TuiOperation) -> Result<Option<ScreenUpdate>, E>,
    E: fmt::Display,
{
    let mut terminal = TerminalGuard::enter()?;
    let mut state = ScreenState::default();
    state.normalize(&frame);
    loop {
        terminal.draw(|surface: &mut Frame<'_>| draw(surface, &frame, &state))?;
        let event: Event = event::read()?;
        let Event::Key(key) = event else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if state.handle_inspection_key(key) {
            continue;
        }
        let editor_operation: Option<Option<TuiOperation>> = handle_editor(key, &frame, &mut state);
        let operation: Option<TuiOperation> = if let Some(operation) = editor_operation {
            operation
        } else {
            match key.code {
                KeyCode::Up => {
                    state.move_vertical(&frame, -1);
                    None
                }
                KeyCode::Down => {
                    state.move_vertical(&frame, 1);
                    None
                }
                KeyCode::Left => {
                    state.move_horizontal(&frame, -1);
                    None
                }
                KeyCode::Right => {
                    state.move_horizontal(&frame, 1);
                    None
                }
                KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    state.cycle(&frame, -1);
                    None
                }
                KeyCode::BackTab => {
                    state.cycle(&frame, -1);
                    None
                }
                KeyCode::Tab => {
                    state.cycle(&frame, 1);
                    None
                }
                KeyCode::PageUp => {
                    if state.dialog_open(&frame) {
                        state.dialog_scroll = state.dialog_scroll.saturating_sub(5);
                    } else {
                        state.main_scroll = state.main_scroll.saturating_sub(5);
                    }
                    None
                }
                KeyCode::PageDown => {
                    if state.dialog_open(&frame) {
                        state.dialog_scroll = state.dialog_scroll.saturating_add(5);
                    } else {
                        state.main_scroll = state.main_scroll.saturating_add(5);
                    }
                    None
                }
                KeyCode::Enter | KeyCode::Char(' ') => focused_operation(&frame, &mut state),
                KeyCode::Esc | KeyCode::Backspace if state.dialog_open(&frame) => {
                    state.dialog_dismissed = true;
                    state.normalize(&frame);
                    None
                }
                KeyCode::Esc | KeyCode::Backspace => Some(TuiOperation::Back),
                KeyCode::Char('q') => Some(TuiOperation::Quit),
                KeyCode::Char('s') => Some(TuiOperation::ToggleSidebar),
                KeyCode::Char('b') => Some(TuiOperation::Back),
                KeyCode::Char('f') => Some(TuiOperation::Forward),
                KeyCode::F(2) => Some(TuiOperation::QuickSave),
                KeyCode::F(3) => Some(TuiOperation::QuickLoad),
                KeyCode::F(4) => Some(TuiOperation::NextLanguage),
                KeyCode::F(10) => Some(TuiOperation::Inspect),
                _ => None,
            }
        };
        let Some(operation) = operation else {
            continue;
        };
        if operation == TuiOperation::Quit {
            return Ok(());
        }
        if operation == TuiOperation::Dismiss {
            state.dialog_dismissed = true;
            state.normalize(&frame);
            continue;
        }
        let previous_status: String = std::mem::replace(&mut state.status, String::from("…"));
        terminal.draw(|surface: &mut Frame<'_>| draw(surface, &frame, &state))?;
        let feedback: TuiOperation = operation.clone();
        match dispatch(operation) {
            Ok(Some(ScreenUpdate::Frame(next))) => {
                frame = *next;
                state.main_scroll = 0;
                state.status.clear();
                state.focus = None;
                state.normalize(&frame);
            }
            Ok(Some(ScreenUpdate::Inspect(snapshot))) => {
                state.status = previous_status;
                state.open_inspection(&snapshot);
            }
            Ok(None) => {
                apply_input_feedback(&mut frame, &feedback);
                state.status = String::from("✓");
            }
            Err(error) => state.status = format!("! {error}"),
        }
    }
}

fn handle_editor(
    key: KeyEvent,
    frame: &TuiFrame,
    state: &mut ScreenState,
) -> Option<Option<TuiOperation>> {
    let mut value: String = state.editor.take()?;
    let operation: Option<TuiOperation> = match key.code {
        KeyCode::Esc => None,
        KeyCode::Enter => {
            let Some(index) = state.focus else {
                return Some(None);
            };
            let interaction: &TuiInteraction = &frame.interactions[index];
            let Some(id) = interaction.id.clone() else {
                state.status = String::from("! 输入项缺少身份");
                return Some(None);
            };
            Some(TuiOperation::Input {
                id,
                value: narrava_loom_core::semantic::SemanticValue::Text(value),
            })
        }
        KeyCode::Backspace => {
            value.pop();
            state.editor = Some(value);
            None
        }
        KeyCode::Char(character) => {
            value.push(character);
            state.editor = Some(value);
            None
        }
        _ => {
            state.editor = Some(value);
            None
        }
    };
    Some(operation)
}

fn focused_operation(frame: &TuiFrame, state: &mut ScreenState) -> Option<TuiOperation> {
    state.normalize(frame);
    let interaction: &TuiInteraction = frame.interactions.get(state.focus?)?;
    if let Some(TuiInput::Text { value }) = &interaction.input {
        state.editor = Some(value.clone());
        return None;
    }
    crate::TuiCommand::Select(state.focus?).resolve(frame).ok()
}

pub(crate) fn draw(surface: &mut Frame<'_>, frame: &TuiFrame, state: &ScreenState) {
    let area: Rect = surface.area();
    let sidebar: &[String] = match frame.sidebar_mode {
        TuiSidebarMode::Expanded => &frame.bar,
        TuiSidebarMode::Stowed => &frame.bar_stowed,
    };
    let header_height: u16 = region_height(&frame.header, area.width, 4);
    let sidebar_height: u16 = region_height(sidebar, area.width, 8);
    let footer_height: u16 = region_height(&frame.footer, area.width, 4);
    let action_height: u16 = (frame.interactions.len() as u16 + 2).clamp(3, 9);
    let layout: Vec<Rect> = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Length(sidebar_height),
            Constraint::Min(3),
            Constraint::Length(footer_height),
            Constraint::Length(action_height),
            Constraint::Length(1),
        ])
        .split(area)
        .to_vec();
    draw_region(surface, layout[0], &frame.header, true);
    draw_region(
        surface,
        layout[1],
        sidebar,
        frame.sidebar_mode == TuiSidebarMode::Expanded,
    );
    let main = Paragraph::new(styled_text(&frame.main))
        .wrap(Wrap { trim: false })
        .scroll((state.main_scroll, 0));
    surface.render_widget(main, layout[2].inner(Margin::new(1, 0)));
    draw_region(surface, layout[3], &frame.footer, true);
    if !state.dialog_open(frame) {
        draw_actions(surface, layout[4], frame, state);
    }
    let editor: String = state
        .editor
        .as_ref()
        .map(|value: &String| format!("⌨ {value}_"))
        .unwrap_or_default();
    let help: String = if editor.is_empty() {
        format!(
            "q退出  ↑↓选择  ←→分组  Enter确认  PgUp/Dn滚动  F2存  F3读  F4语言  F10检查  s侧栏  {}",
            state.status
        )
    } else {
        format!("Enter 提交  Esc 取消  {editor}")
    };
    surface.render_widget(
        Paragraph::new(help).style(Style::default().fg(Color::DarkGray)),
        layout[5],
    );
    if state.dialog_open(frame)
        && let Some(page) = active_dialog_page(frame, state)
    {
        let popup: Rect = centered_rect(76, 60, area);
        surface.render_widget(Clear, popup);
        let block: Block<'_> = Block::default()
            .title(format!(
                " {} ({}/{}) ←→切页 Esc关闭 ",
                page.title,
                state.dialog_page + 1,
                frame.dialog_pages.len()
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ACCENT));
        let inner: Rect = block.inner(popup);
        surface.render_widget(block, popup);
        let page_layout: Vec<Rect> = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length((state.action_indices(frame).len() as u16 + 1).clamp(2, 7)),
            ])
            .split(inner)
            .to_vec();
        surface.render_widget(
            Paragraph::new(styled_text(&page.lines))
                .wrap(Wrap { trim: false })
                .scroll((state.dialog_scroll, 0)),
            page_layout[0],
        );
        draw_actions(surface, page_layout[1], frame, state);
    }
    if let Some(inspection) = &state.inspection {
        let popup: Rect = centered_rect(92, 86, area);
        surface.render_widget(Clear, popup);
        surface.render_widget(
            Paragraph::new(inspection.lines.join("\n"))
                .wrap(Wrap { trim: false })
                .scroll((inspection.scroll, 0))
                .block(
                    Block::default()
                        .title(" 只读检查 · ↑↓ / PgUp/Dn 滚动 · Esc / F10 关闭 ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(ACCENT)),
                ),
            popup,
        );
    }
}

fn draw_region(surface: &mut Frame<'_>, area: Rect, lines: &[String], bordered: bool) {
    if lines.is_empty() || area.height == 0 {
        return;
    }
    let block: Block<'_> = if bordered {
        Block::default().borders(Borders::ALL)
    } else {
        Block::default()
    };
    surface.render_widget(
        Paragraph::new(styled_text(lines))
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

fn draw_actions(surface: &mut Frame<'_>, area: Rect, frame: &TuiFrame, state: &ScreenState) {
    let indices: Vec<usize> = state.action_indices(frame);
    let items: Vec<ListItem<'_>> = indices
        .iter()
        .map(|index: &usize| {
            let interaction: &TuiInteraction = &frame.interactions[*index];
            ListItem::new(Line::from(format!("  {}", interaction.label)))
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::TOP))
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::default()
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        );
    let selected: Option<usize> = state
        .focus
        .and_then(|focus: usize| indices.iter().position(|index: &usize| *index == focus));
    let mut list_state: ListState = ListState::default().with_selected(selected);
    surface.render_stateful_widget(list, area, &mut list_state);
}

fn region_height(lines: &[String], width: u16, maximum: u16) -> u16 {
    if lines.is_empty() {
        return 0;
    }
    let content_width: usize = usize::from(width.saturating_sub(2)).max(1);
    let rows: usize = lines
        .iter()
        .map(|line: &String| line.chars().count().div_ceil(content_width).max(1))
        .sum();
    (rows as u16 + 2).min(maximum)
}

/// Ratatui owns terminal styling, so legacy renderer ANSI color escapes must not become content.
fn active_dialog_page<'frame>(
    frame: &'frame TuiFrame,
    state: &ScreenState,
) -> Option<&'frame TuiDialogPage> {
    frame
        .dialog_pages
        .get(state.dialog_page)
        .or_else(|| frame.dialog_pages.first())
}

pub(crate) fn apply_input_feedback(frame: &mut TuiFrame, operation: &TuiOperation) {
    let TuiOperation::Input { id, value } = operation else {
        return;
    };
    let Some(index) = frame
        .interactions
        .iter()
        .position(|interaction: &TuiInteraction| interaction.id.as_deref() == Some(id))
    else {
        return;
    };
    let group: String = frame.interactions[index].group.clone();
    match &mut frame.interactions[index].input {
        Some(TuiInput::Checkbox {
            checked, selected, ..
        }) => {
            *selected = value == checked;
            frame.interactions[index].label = if *selected { "[x]" } else { "[ ]" }.to_owned();
        }
        Some(TuiInput::Radio { .. }) => {
            for interaction in &mut frame.interactions {
                if interaction.group == group
                    && let Some(TuiInput::Radio { selected, .. }) = &mut interaction.input
                {
                    *selected = interaction.id.as_deref() == Some(id);
                    interaction.label = if *selected { "(o)" } else { "( )" }.to_owned();
                }
            }
        }
        Some(TuiInput::Text { value: current }) => {
            if let narrava_loom_core::semantic::SemanticValue::Text(value) = value {
                *current = value.clone();
                frame.interactions[index].label = format!("[{value}]");
            }
        }
        None => {}
    }
}

fn styled_text(lines: &[String]) -> Text<'static> {
    Text::from(
        lines
            .iter()
            .map(|line: &String| styled_line(line))
            .collect::<Vec<_>>(),
    )
}

pub(crate) fn styled_line(text: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut style: Style = Style::default();
    let mut content: String = String::new();
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' && characters.peek() == Some(&'[') {
            characters.next();
            if !content.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut content), style));
            }
            let mut sequence: String = String::new();
            for value in characters.by_ref() {
                if value.is_ascii_alphabetic() {
                    break;
                }
                sequence.push(value);
            }
            style = apply_sgr(style, &sequence);
        } else {
            content.push(character);
        }
    }
    if !content.is_empty() || spans.is_empty() {
        spans.push(Span::styled(content, style));
    }
    Line::from(spans)
}

fn apply_sgr(mut style: Style, sequence: &str) -> Style {
    let values: Vec<u8> = sequence
        .split(';')
        .filter_map(|value: &str| value.parse::<u8>().ok())
        .collect();
    if values.is_empty() || values.contains(&0) {
        style = Style::default();
    }
    if values.contains(&1) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if values.contains(&4) {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if values.len() >= 5 && values[0..2] == [38, 2] {
        style = style.fg(Color::Rgb(values[2], values[3], values[4]));
    }
    style
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical: Vec<Rect> = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area)
        .to_vec();
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout: io::Stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ignored = disable_raw_mode();
            return Err(error);
        }
        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut stdout: io::Stdout = io::stdout();
                let _ignored = execute!(stdout, LeaveAlternateScreen);
                let _ignored = disable_raw_mode();
                return Err(error);
            }
        };
        Ok(Self { terminal })
    }

    fn draw<F>(&mut self, render: F) -> io::Result<()>
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.terminal.draw(render).map(|_| ())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ignored = disable_raw_mode();
        let _ignored = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ignored = self.terminal.show_cursor();
    }
}

#[derive(Debug)]
pub(crate) struct HostScreenError(String);

impl From<io::Error> for HostScreenError {
    fn from(error: io::Error) -> Self {
        Self(error.to_string())
    }
}

impl fmt::Display for HostScreenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

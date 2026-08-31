//! 标准输入输出循环与完整 TUI 帧打印。

use crate::renderer::panel_lines;
use crate::{TuiCommand, TuiFrame, TuiInput, TuiInteraction, TuiOperation, TuiSidebarMode};
use terminal_size::{Width, terminal_size};

const FALLBACK_TERMINAL_WIDTH: usize = 80;
const MINIMUM_DIVIDER_WIDTH: usize = 8;
use std::{
    fmt,
    io::{self, BufRead, Write},
};

/// 运行阻塞式终端输入循环。
///
/// `dispatch` 只接收已经过当前帧验证的操作；返回新帧时立即替换画面，返回 `None`
/// 表示状态已写入但无需重绘。该边界不持有 Runtime，便于 Native Host 接入执行后端。
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
            TuiOperation::ToggleSidebar => {
                frame.sidebar_mode = frame.sidebar_mode.toggled();
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
    let terminal_width: usize = terminal_size()
        .map(|(Width(width), _)| usize::from(width))
        .unwrap_or(FALLBACK_TERMINAL_WIDTH);
    let section_divider: String = "=".repeat(terminal_width.max(MINIMUM_DIVIDER_WIDTH));
    let group_divider: String = "-".repeat(terminal_width.max(MINIMUM_DIVIDER_WIDTH));
    write_bordered_region(writer, &frame.header)?;
    match frame.sidebar_mode {
        TuiSidebarMode::Expanded => write_bordered_region(writer, &frame.bar)?,
        TuiSidebarMode::Stowed => write_bordered_region(writer, &frame.bar_stowed)?,
    }
    if !frame.main.is_empty() {
        writeln!(writer, "\n{section_divider}")?;
    }
    write_region(writer, &frame.main)?;
    write_region(writer, &frame.dialog)?;
    write_bordered_region(writer, &frame.footer)?;
    writeln!(writer, "\n{section_divider}")?;
    if frame.interactions.is_empty() {
        writeln!(writer, "（当前没有可操作项；输入 help 查看命令）")?;
    } else {
        let mut group: &str = "";
        for (index, interaction) in frame.interactions.iter().enumerate() {
            if interaction.group != group {
                if !group.is_empty() {
                    writeln!(writer, "{group_divider}")?;
                }
                group = interaction.group.as_str();
            }
            writeln!(
                writer,
                "    {}. {} {}",
                index + 1,
                interaction.label,
                interaction_hint(interaction)
            )?;
        }
    }
    Ok(())
}

fn write_region(writer: &mut impl Write, lines: &[String]) -> io::Result<()> {
    if !lines.is_empty() {
        for line in lines {
            writeln!(writer, "{line}")?;
        }
    }
    Ok(())
}

fn write_bordered_region(writer: &mut impl Write, lines: &[String]) -> io::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    writeln!(writer)?;
    for line in panel_lines(lines.to_vec()) {
        writeln!(writer, "{line}")?;
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
    writeln!(writer, "  sidebar             切换侧栏展开／收起")?;
    writeln!(writer, "  help                显示本帮助")?;
    writeln!(writer, "  quit                退出")
}

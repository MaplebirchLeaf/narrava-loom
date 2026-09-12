//! TUI 的边框、同行拼接、显示宽度与终端样式。

use super::TuiBlock;

/// 十格字符条属于 TUI 表现；越界值保留文字，填充限制在量程内。
pub(super) fn render_meter(
    label: &str,
    value: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
) -> String {
    let min: f64 = min.filter(|value| value.is_finite()).unwrap_or(0.0);
    let max: f64 = max
        .filter(|value| value.is_finite())
        .unwrap_or(100.0)
        .max(min);
    let value: f64 = value.filter(|value| value.is_finite()).unwrap_or(min);
    let scale: f64 = min.abs().max(max.abs()).max(1.0);
    let fraction: f64 = if max > min {
        (value.clamp(min, max) / scale - min / scale) / (max / scale - min / scale)
    } else {
        0.0
    };
    let filled: usize = (fraction * 10.0).round().clamp(0.0, 10.0) as usize;
    format!(
        "{label} {}{} {value}/{max}",
        "█".repeat(filled),
        "░".repeat(10 - filled)
    )
}

pub(crate) fn panel_lines(lines: Vec<String>) -> Vec<String> {
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
    let width: usize = lines
        .iter()
        .map(|line| terminal_width(line))
        .max()
        .unwrap_or(0);
    let mut framed: Vec<String> = Vec::with_capacity(lines.len() + 2);
    framed.push(format!("┌{}┐", "─".repeat(width + 2)));
    for line in lines {
        let padding: usize = width.saturating_sub(terminal_width(line.as_str()));
        framed.push(format!("│ {line}{} │", " ".repeat(padding)));
    }
    framed.push(format!("└{}┘", "─".repeat(width + 2)));
    framed
}

pub(super) fn join_blocks(blocks: &[TuiBlock], separator: &str) -> Vec<String> {
    let height: usize = blocks
        .iter()
        .map(|block| block.lines.len())
        .max()
        .unwrap_or(0);
    let widths: Vec<usize> = blocks
        .iter()
        .map(|block| {
            block
                .lines
                .iter()
                .map(|line| terminal_width(line))
                .max()
                .unwrap_or(0)
        })
        .collect();
    (0..height)
        .map(|row| {
            let mut line: String = String::new();
            for (block, width) in blocks.iter().zip(&widths) {
                let cell: String = block
                    .lines
                    .get(row)
                    .cloned()
                    .unwrap_or_else(|| " ".repeat(*width));
                let junction: Option<char> = if separator.is_empty() {
                    match (line.chars().last(), cell.chars().next()) {
                        (Some('┐'), Some('┌')) => Some('┬'),
                        (Some('│'), Some('│')) => Some('│'),
                        (Some('┘'), Some('└')) => Some('┴'),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(junction) = junction {
                    line.pop();
                    line.push(junction);
                    line.push_str(&cell['│'.len_utf8()..]);
                } else {
                    if !line.is_empty() {
                        line.push_str(separator);
                    }
                    line.push_str(&cell);
                }
            }
            line
        })
        .collect()
}

fn terminal_width(line: &str) -> usize {
    use unicode_width::UnicodeWidthChar;

    let mut width: usize = 0;
    let mut characters: std::str::Chars<'_> = line.chars();
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

/// Text 是一个可见文本片段；HardBreak 已是相邻片段之间的独立协议节点。
pub(super) fn visible_lines(text: String) -> Vec<String> {
    vec![text]
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

/// 标题和颜色共用同一终端转义序列，输入格式的差异留在各适配器。
pub(super) fn decorate_text(mut text: String, color: u8, heading: bool) -> String {
    if heading {
        text = format!("\x1b[1;4m{text}\x1b[0m");
    }
    palette_rgb(color)
        .map(|(r, g, b)| format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m"))
        .unwrap_or(text)
}

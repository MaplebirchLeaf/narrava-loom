//! 终端命令解析、校验和可执行操作。

use crate::TuiFrame;
use narrava_loom_script::protocol_adapter::SurfaceValue;
use std::fmt;

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
    /// 终端操作列表中的来源分组；仅影响 Host 展示，不参与回传身份。
    pub group: String,
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
    Back,
    Forward,
    ToggleSidebar,
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
        if matches!(trimmed, "b" | "back") {
            return Ok(Self::Back);
        }
        if matches!(trimmed, "f" | "forward") {
            return Ok(Self::Forward);
        }
        if matches!(trimmed, "s" | "sidebar") {
            return Ok(Self::ToggleSidebar);
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
            Self::Back => Ok(TuiOperation::Back),
            Self::Forward => Ok(TuiOperation::Forward),
            Self::ToggleSidebar => Ok(TuiOperation::ToggleSidebar),
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
    Back,
    Forward,
    ToggleSidebar,
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

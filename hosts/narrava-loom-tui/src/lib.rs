//! Core Surface 到终端区域、文本和交互列表的最小 Host Renderer。
//!
//! 稳定导出保留在此 facade；命令、终端循环和 Surface 渲染分别由命名子模块负责。

mod command;
mod renderer;
mod terminal;

pub mod host;

pub use command::{TuiCommand, TuiCommandError, TuiInput, TuiInteraction, TuiOperation};
pub use renderer::{TuiDelayedText, TuiFrame, TuiRenderer, TuiSidebarMode};
pub use terminal::{run_terminal, write_frame};

#[cfg(test)]
mod tests;

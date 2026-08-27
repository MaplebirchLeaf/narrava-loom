//! Narrava Loom TUI Host 命令行入口。
//!
//! ```bash
//! cargo run -p narrava-loom-tui -- [game_directory]
//! ```
//!
//! 省略参数时使用仓库根目录的 `examples/`。游戏目录可以是开发目录或含
//! `game.nar` 的发行目录；终端内输入编号选择动作，`h` 帮助、`r` 重绘、`q` 退出。

use std::process::ExitCode;

fn main() -> ExitCode {
    let game_path: String = std::env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("examples"));
    match narrava_loom_tui::host::run(&game_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("TUI Host 错误：{}: {}", error.code, error.message);
            ExitCode::FAILURE
        }
    }
}

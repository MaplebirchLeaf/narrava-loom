//! Narrava Loom 桌面版二进制入口：解析游戏目录并启动 Tauri Host。

/// 解析命令行游戏目录并交给 `narrava_loom_tauri::run`；失败时打印错误并退出。
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let game_path: String = narrava_loom_tauri::game_path_from_args(args);

    if let Err(error) = narrava_loom_tauri::run(game_path.as_str()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

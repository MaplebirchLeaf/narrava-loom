fn main() {
    let args: Vec<String> = std::env::args().collect();
    let game_path: String = narrava_loom_tauri::game_path_from_args(args);

    if let Err(error) = narrava_loom_tauri::run(game_path.as_str()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

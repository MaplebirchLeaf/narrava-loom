//! Tauri 2 command 注册。
//!
//! 每个 command 用 managed state 注入同一个 Runtime Worker；本模块只做参数
//! 转交与结果包装，事务逻辑在 [TauriHost] 与 Runtime Worker 模块。

use tauri::State;

use crate::{HostAssetsDto, HostErrorDto, HostLogDto, HostUpdateDto, TauriHost};

/// 启动游戏并返回起始 Passage 更新。
#[tauri::command]
pub fn start_game(host: State<'_, TauriHost>) -> Result<HostUpdateDto, HostErrorDto> {
    host.start()
}

/// 按交互身份推进导航并返回更新。
#[tauri::command]
pub fn activate(
    interaction: String,
    host: State<'_, TauriHost>,
) -> Result<HostUpdateDto, HostErrorDto> {
    host.activate(interaction.as_str())
}

/// 写回输入控件值。
#[tauri::command]
pub fn input(
    interaction: String,
    value: serde_json::Value,
    host: State<'_, TauriHost>,
) -> Result<(), HostErrorDto> {
    host.input(interaction, value)
}

/// 返回 Host 启动资产。
#[tauri::command]
pub fn host_assets(host: State<'_, TauriHost>) -> HostAssetsDto {
    host.assets()
}

/// 执行存档操作（export/import）。
#[tauri::command]
pub fn save_game(
    operation: String,
    target: String,
    host: State<'_, TauriHost>,
) -> Result<(), HostErrorDto> {
    host.save(operation, target)
}

/// 拉取 Worker 日志快照。
#[tauri::command]
pub fn host_logs(host: State<'_, TauriHost>) -> Result<Vec<HostLogDto>, HostErrorDto> {
    host.logs()
}

/// 拉取可用语言列表。
#[tauri::command]
pub fn available_languages(host: State<'_, TauriHost>) -> Result<Vec<String>, HostErrorDto> {
    host.languages()
}

/// 切换运行时语言。
#[tauri::command]
pub fn select_language(locale: String, host: State<'_, TauriHost>) -> Result<(), HostErrorDto> {
    host.select_language(locale)
}

/// 查询开发者模式是否启用。
#[tauri::command]
pub fn developer_enabled(host: State<'_, TauriHost>) -> bool {
    host.developer()
}

/// 开发者模式下切换 WebView 调试工具开关。
#[tauri::command]
pub fn toggle_devtools(
    window: tauri::WebviewWindow,
    host: State<'_, TauriHost>,
) -> Result<(), HostErrorDto> {
    if !host.developer() {
        return Err(HostErrorDto::new(
            "tauri_host.developer_disabled",
            "请在 config.toml 的 [host.tauri] 中设置 developer = true",
        ));
    }
    if window.is_devtools_open() {
        window.close_devtools();
    } else {
        window.open_devtools();
    }
    Ok(())
}

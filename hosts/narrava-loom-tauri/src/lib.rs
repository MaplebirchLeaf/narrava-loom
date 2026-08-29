//! Narrava Core 与 Tauri IPC 之间的最小 Host Binding。
//!
//! `TauriHost` 把 Core 的 Engine 事务（start/activate/input/save/语言/开发者能力）
//! 投递给常驻 Runtime Worker 线程，并把 Runtime 返回的拥有型 Protocol DTO
//! 输出为 WebView JSON；转换 Core Surface 不属于 Host。

mod assets;
mod config;
mod package;
mod resource_protocol;
mod save_io;

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use narrava_loom_core::resource::ResourceCatalog;
use serde::Serialize;

pub use assets::{HostAssetsDto, HostResourceDto, HostStyleDto};
pub use config::{TauriConfigError, TauriProjectConfig, TauriWindowConfig};
pub use narrava_loom_protocol::{HostErrorDto, HostNodeDto, HostReplaceTargetDto, HostUpdateDto};

use package::{load_release_package, load_tauri_config};

/// Host 管理面板展示的一条有界日志（只含级别与可显示消息）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HostLogDto {
    /// 日志级别（如 `info`/`error`）。
    pub level: String,
    /// 人类可读的日志消息。
    pub message: String,
}

/// Worker 线程收到的请求；除日志/语言查询外都通过一次性 channel 回传结果。
pub struct TauriHost {
    requests: Sender<WorkerRequest>,
    assets: Arc<HostAssetsDto>,
    resources: Arc<ResourceCatalog>,
    developer: bool,
}

impl TauriHost {
    /// 读取配置并启动 Runtime Worker；游戏目录同时是开发目录或含 `game.nar` 的发行目录。
    pub fn spawn(game_path: &str) -> Result<Self, HostErrorDto> {
        let config = load_tauri_config(Path::new(game_path))?;
        Self::spawn_configured(game_path, config.developer())
    }

    /// 按显式 developer 开关启动 Worker（供 `spawn` 与 `run` 复用）。
    fn spawn_configured(game_path: &str, developer: bool) -> Result<Self, HostErrorDto> {
        let (sender, receiver): (Sender<WorkerRequest>, Receiver<WorkerRequest>) = mpsc::channel();
        let root = Path::new(game_path);
        let resources = Arc::new(match load_release_package(root)? {
            Some(package) => package.resources().clone(),
            None => ResourceCatalog::discover(root)
                .map_err(|error| HostErrorDto::new("tauri_host.resource", error.to_string()))?,
        });
        let config: TauriProjectConfig = load_tauri_config(root)?;
        let mut assets: HostAssetsDto = HostAssetsDto::discover_with_catalog(root, &resources)?;
        assets.title = config.title().to_owned();
        let assets = Arc::new(assets);
        let game_path: String = game_path.to_owned();
        let worker_resources: Arc<ResourceCatalog> = Arc::clone(&resources);
        thread::Builder::new()
            .name(String::from("narrava-runtime"))
            .spawn(move || run_worker(game_path, receiver, worker_resources))
            .map_err(|error: std::io::Error| {
                HostErrorDto::new("tauri_host.worker_spawn", error.to_string())
            })?;
        Ok(Self {
            requests: sender,
            assets,
            resources,
            developer,
        })
    }

    /// 启动游戏并返回起始 Passage 的语义更新。
    pub fn start(&self) -> Result<HostUpdateDto, HostErrorDto> {
        let (reply, result): (WorkerReply, WorkerResponse) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Start(reply))
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())?
    }

    /// 按交互身份推进（导航/按钮/返回等），返回渲染后的语义更新。
    pub fn activate(&self, interaction: &str) -> Result<HostUpdateDto, HostErrorDto> {
        let (reply, result): (WorkerReply, WorkerResponse) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Activate {
                interaction: interaction.to_owned(),
                reply,
            })
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())?
    }

    /// 把输入控件的新值写回 Worker State 并落盘。
    pub fn input(&self, interaction: String, value: serde_json::Value) -> InputResult {
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Input {
                interaction,
                value,
                reply,
            })
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())?
    }

    /// 返回 Host 启动资产（标题、样式表、Resource 元数据）。
    pub fn assets(&self) -> HostAssetsDto {
        (*self.assets).clone()
    }

    /// 执行存档操作（`export`/`import`）。
    pub fn save(&self, operation: String, target: String) -> CommandResult {
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Save {
                operation,
                target,
                reply,
            })
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())?
    }

    /// 拉取 Worker 当前日志快照。
    pub fn logs(&self) -> Result<Vec<HostLogDto>, HostErrorDto> {
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Logs(reply))
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())
    }

    /// 拉取可用语言 locale 列表。
    pub fn languages(&self) -> Result<Vec<String>, HostErrorDto> {
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Languages(reply))
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())
    }

    /// 切换运行时语言（下一次渲染生效）。
    pub fn select_language(&self, locale: String) -> CommandResult {
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::SelectLanguage { locale, reply })
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())?
    }

    /// 当前是否启用开发者模式。
    pub fn developer(&self) -> bool {
        self.developer
    }
}

/// 从进程参数选择游戏目录；正式版无参数时使用可执行文件所在目录。
pub fn game_path_from_args(args: impl IntoIterator<Item = String>) -> String {
    let mut args = args.into_iter();
    let _program = args.next();
    args.next().unwrap_or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .into_owned()
    })
}

/// 启动共享 Tauri Host；当前仓库的可运行调用方是桌面入口。
pub fn run(game_path: &str) -> Result<(), HostErrorDto> {
    let project_config = load_tauri_config(Path::new(game_path))?;
    let release_package = load_release_package(Path::new(game_path))?;
    let packaged_icon = match (release_package.as_ref(), project_config.icon()) {
        (Some(package), Some(path)) => package
            .resources()
            .read(path)
            .map_err(|error| HostErrorDto::new("tauri_host.resource_read", error.to_string()))?
            .map(<[u8]>::to_vec),
        _ => None,
    };
    let host: TauriHost = TauriHost::spawn_configured(game_path, project_config.developer())?;
    let protocol_resources = host.resources.clone();
    let game_path: PathBuf = PathBuf::from(game_path);
    tauri::Builder::default()
        .register_uri_scheme_protocol("narrava-resource", move |_context, request| {
            resource_protocol::respond(&protocol_resources, request.uri().path())
        })
        .manage(host)
        .invoke_handler(tauri::generate_handler![
            commands::start_game,
            commands::activate,
            commands::input,
            commands::host_assets,
            commands::save_game,
            commands::host_logs,
            commands::available_languages,
            commands::select_language,
            commands::developer_enabled,
            commands::toggle_devtools
        ])
        .setup(move |app: &mut tauri::App| {
            build_main_window(app, &game_path, &project_config, packaged_icon.as_deref())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|error: tauri::Error| HostErrorDto::new("tauri_host.app", error.to_string()))
}

/// 供后续 Android/iOS 平台工程调用的入口；共享实现不等同于已完成移动打包。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_mobile() {
    let game_path: String = game_path_from_args(std::env::args());
    if let Err(error) = run(game_path.as_str()) {
        panic!("{error}");
    }
}

/// 按平台配置构建主 WebView 窗口（标题、尺寸、图标）。
fn build_main_window(
    app: &mut tauri::App,
    game_path: &Path,
    config: &TauriProjectConfig,
    packaged_icon: Option<&[u8]>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    let window: &TauriWindowConfig = config.window();
    let mut builder: WebviewWindowBuilder<'_, tauri::Wry, _> =
        WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title(config.title())
            .inner_size(window.width(), window.height())
            .min_inner_size(window.min_width(), window.min_height())
            .resizable(window.resizable())
            .fullscreen(window.fullscreen())
            .decorations(window.decorations())
            .maximized(window.maximized());

    if let Some(icon) = config.icon() {
        let image: tauri::image::Image<'static> = if let Some(bytes) = packaged_icon {
            tauri::image::Image::from_bytes(bytes)?.to_owned()
        } else {
            tauri::image::Image::from_path(game_path.join(icon))?
        };
        builder = builder.icon(image)?;
    }

    let _window: tauri::WebviewWindow = builder.build()?;
    Ok(())
}

/// Tauri 2 command 使用 managed state 注入同一个 Runtime Worker。
pub mod commands;
mod worker;

use worker::{
    CommandResult, InputResult, WorkerReply, WorkerRequest, WorkerResponse, run_worker,
    worker_stopped,
};

#[cfg(test)]
mod tests;

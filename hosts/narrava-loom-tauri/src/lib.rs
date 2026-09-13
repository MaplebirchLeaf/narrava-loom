//! Narrava Core 与 Tauri IPC 之间的最小 Host Binding。
//!
//! `TauriHost` 把 Core 的 Engine 事务（start/activate/input/save/语言/开发者能力）
//! 投递给常驻 Runtime Worker 线程，并把 Runtime 返回的拥有型 Protocol DTO
//! 输出为 WebView JSON；转换 Core Surface 不属于 Host。

mod assets;
#[path = "../../audio.rs"]
mod audio;
mod config;
mod debug;
mod package;
mod resource_protocol;
#[path = "../../save_io.rs"]
mod save_io;

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use narrava_loom_core::{nar::ValidatedNarPackage, resource::ResourceCatalog};

pub use assets::{HostAssetsDto, HostResourceDto, HostStyleDto};
pub use config::{TauriConfigError, TauriProjectConfig, TauriWindowConfig};
pub use narrava_loom_protocol::{
    ContainerFlowDto, ContainerPresentationDto, HostDebugSnapshotDto, HostErrorDto, HostNodeDto,
    HostReplaceTargetDto, HostUpdateDto, PendingOperation, PendingResult, RuntimeCommand,
    RuntimeUpdate, SaveOperation,
};

use package::{load_release_package, load_tauri_config};
use save_io::process_save_io;

/// 与 Runtime 共用结构化日志，保留来源和诊断位置。
pub type HostLogDto = narrava_loom_protocol::HostLogRecordDto;

/// 异步 Host facade；Runtime 状态固定由专用 Worker 串行持有。
pub struct TauriHost {
    requests: Sender<WorkerRequest>,
    assets: Arc<HostAssetsDto>,
    resources: Arc<ResourceCatalog>,
    game_path: Arc<PathBuf>,
    current_passage: Mutex<Option<String>>,
    developer: bool,
    console_cancelled: std::sync::atomic::AtomicBool,
}

impl TauriHost {
    /// 读取配置并启动 Runtime Worker；游戏目录同时是开发目录或含 `game.nar` 的发行目录。
    pub fn spawn(game_path: &str) -> Result<Self, HostErrorDto> {
        let root = Path::new(game_path);
        let release = load_release_package(root)?;
        let config = resolved_tauri_config(root, release.as_ref())?;
        Self::spawn_configured(
            game_path,
            config.developer(),
            config.title().to_owned(),
            release,
        )
    }

    /// 按显式 developer 开关启动 Worker（供 `spawn` 与 `run` 复用）。
    fn spawn_configured(
        game_path: &str,
        developer: bool,
        title: String,
        release: Option<ValidatedNarPackage>,
    ) -> Result<Self, HostErrorDto> {
        let (sender, receiver): (Sender<WorkerRequest>, Receiver<WorkerRequest>) = mpsc::channel();
        let root = Path::new(game_path);
        let resources = Arc::new(match release.as_ref() {
            Some(package) => package.resources().clone(),
            None => ResourceCatalog::discover(root)
                .map_err(|error| HostErrorDto::new("tauri_host.resource", error.to_string()))?,
        });
        let mut assets: HostAssetsDto = HostAssetsDto::discover_with_catalog(root, &resources)?;
        assets.title = title;
        let assets = Arc::new(assets);
        let game_path: String = game_path.to_owned();
        let host_game_path: Arc<PathBuf> = Arc::new(PathBuf::from(&game_path));
        let worker_resources: Arc<ResourceCatalog> = Arc::clone(&resources);
        thread::Builder::new()
            .name(String::from("narrava-runtime"))
            .spawn(move || run_worker(game_path, receiver, worker_resources, release))
            .map_err(|error: std::io::Error| {
                HostErrorDto::new("tauri_host.worker_spawn", error.to_string())
            })?;
        Ok(Self {
            requests: sender,
            assets,
            resources,
            game_path: host_game_path,
            current_passage: Mutex::new(None),
            developer,
            console_cancelled: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// 启动游戏并返回起始 Passage 的语义更新。
    pub async fn start(&self) -> Result<HostUpdateDto, HostErrorDto> {
        let update: HostUpdateDto = ready_update(self.execute(RuntimeCommand::Start).await)?;
        self.remember_passage(&update.current)?;
        Ok(update)
    }

    /// 按交互身份推进（导航/按钮/返回等），返回渲染后的语义更新。
    pub async fn activate(&self, interaction: &str) -> Result<HostUpdateDto, HostErrorDto> {
        self.execute_update(RuntimeCommand::Activate {
            interaction: interaction.to_owned(),
        })
        .await?
        .ok_or_else(update_expected)
    }

    /// 沿 Story 历史向前或向后移动。
    pub async fn history(&self, backward: bool) -> Result<HostUpdateDto, HostErrorDto> {
        let command: RuntimeCommand = if backward {
            RuntimeCommand::Back
        } else {
            RuntimeCommand::Forward
        };
        let update: HostUpdateDto = ready_update(self.execute(command).await)?;
        self.remember_passage(&update.current)?;
        Ok(update)
    }

    /// 写回输入值；Reaction 产生的替换或导航帧必须一并交给 WebView。
    pub async fn input(
        &self,
        interaction: String,
        value: serde_json::Value,
    ) -> Result<Option<HostUpdateDto>, HostErrorDto> {
        self.execute_update(RuntimeCommand::Input { interaction, value })
            .await
    }

    /// 返回 Host 启动资产（标题、样式表、Resource 元数据）。
    pub fn assets(&self) -> HostAssetsDto {
        (*self.assets).clone()
    }

    /// 执行存档操作（`export`/`import`）。
    pub async fn save(
        &self,
        operation: String,
        target: String,
    ) -> Result<Option<HostUpdateDto>, HostErrorDto> {
        let operation: SaveOperation = match operation.as_str() {
            "export" => SaveOperation::Export,
            "import" => SaveOperation::Import,
            _ => {
                return Err(HostErrorDto::new(
                    "tauri_host.save_operation",
                    format!("未知 Save 操作：{operation}"),
                ));
            }
        };
        self.execute_update(RuntimeCommand::Save { operation, target })
            .await
    }

    /// 拉取 Worker 当前日志快照。
    pub async fn logs(&self) -> Result<Vec<HostLogDto>, HostErrorDto> {
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Logs(reply))
            .map_err(|_| worker_stopped())?;
        receive_worker(result).await
    }

    /// 拉取可用语言 locale 列表。
    pub async fn languages(&self) -> Result<Vec<String>, HostErrorDto> {
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Languages(reply))
            .map_err(|_| worker_stopped())?;
        receive_worker(result).await
    }

    /// 切换运行时语言；若已经进入故事，则立即重绘当前完整呈现帧。
    pub async fn select_language(
        &self,
        locale: String,
    ) -> Result<Option<HostUpdateDto>, HostErrorDto> {
        self.execute_update(RuntimeCommand::SelectLanguage { locale })
            .await
    }

    /// Applied 没有新画面；其他完成结果沿用必需帧校验，并更新当前 Passage。
    async fn execute_update(
        &self,
        command: RuntimeCommand,
    ) -> Result<Option<HostUpdateDto>, HostErrorDto> {
        let navigation: bool = matches!(
            &command,
            RuntimeCommand::Activate { .. } | RuntimeCommand::Input { .. }
        );
        let previous: Option<String> = self.current_passage()?;
        let result: RuntimeUpdate = self.execute(command).await?;
        if matches!(result, RuntimeUpdate::Applied) {
            return Ok(None);
        }
        let update: HostUpdateDto = ready_update(Ok(result))?;
        self.remember_passage(&update.current)?;
        // 只在玩家交互真正导航后自动保存；读档、语言刷新和历史重放不覆盖存档。
        if navigation && previous.as_deref() != Some(update.current.as_str()) {
            let autosave: RuntimeCommand = RuntimeCommand::Save {
                operation: SaveOperation::Export,
                target: String::from("autosave"),
            };
            // Worker 已把 IO 失败记入统一日志，不能让保存失败撤销已完成导航。
            let _result: Result<RuntimeUpdate, HostErrorDto> = self.execute(autosave).await;
        }
        Ok(Some(update))
    }

    fn current_passage(&self) -> Result<Option<String>, HostErrorDto> {
        self.current_passage
            .lock()
            .map(|current| current.clone())
            .map_err(|_| HostErrorDto::new("tauri_host.current_passage", "当前 Passage 锁已损坏"))
    }

    fn remember_passage(&self, passage: &str) -> Result<(), HostErrorDto> {
        let mut current = self.current_passage.lock().map_err(|_| {
            HostErrorDto::new("tauri_host.current_passage", "当前 Passage 锁已损坏")
        })?;
        *current = Some(passage.to_owned());
        Ok(())
    }

    async fn execute(&self, mut command: RuntimeCommand) -> Result<RuntimeUpdate, HostErrorDto> {
        let console: bool = matches!(command, RuntimeCommand::DebugScript { .. });
        if console {
            self.console_cancelled
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
        loop {
            let cancelled: bool = matches!(command, RuntimeCommand::Cancel { .. });
            let update: RuntimeUpdate = self.execute_step(command).await?;
            if console && cancelled {
                return Err(HostErrorDto::new(
                    "console.cancelled",
                    "已取消等待并回滚 / Wait cancelled and state rolled back",
                ));
            }
            let RuntimeUpdate::Pending { operation } = update else {
                return Ok(update);
            };
            command = self.process_pending_operation(operation, console).await;
        }
    }

    async fn execute_step(&self, command: RuntimeCommand) -> Result<RuntimeUpdate, HostErrorDto> {
        let (reply, result): (WorkerReply, WorkerResponse) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Execute { command, reply })
            .map_err(|_| worker_stopped())?;
        receive_worker(result).await?
    }

    async fn process_pending_operation(
        &self,
        operation: PendingOperation,
        console: bool,
    ) -> RuntimeCommand {
        let operation_id: u64 = operation.id();
        let result: Option<PendingResult> = match operation {
            PendingOperation::Delay { milliseconds, .. } => {
                let deadline =
                    tokio::time::Instant::now() + std::time::Duration::from_millis(milliseconds);
                loop {
                    if console
                        && self
                            .console_cancelled
                            .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        return RuntimeCommand::Cancel {
                            operation: operation_id,
                        };
                    }
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    // 仅开发命令检查取消；普通游戏等待仍只使用一个 timer。
                    tokio::time::sleep(if console {
                        remaining.min(std::time::Duration::from_millis(50))
                    } else {
                        remaining
                    })
                    .await;
                }
                None
            }
            PendingOperation::Save {
                direction,
                target,
                document,
                ..
            } => {
                let game_path: Arc<PathBuf> = Arc::clone(&self.game_path);
                let completed = tokio::task::spawn_blocking(move || {
                    process_save_io(&game_path, direction, &target, document, "tauri_host")
                })
                .await;
                Some(match completed {
                    Ok(Ok(document)) => PendingResult::Save { document },
                    Ok(Err(error)) => PendingResult::Failed { error },
                    Err(error) => PendingResult::Failed {
                        error: HostErrorDto::new("tauri_host.save_task", error.to_string()),
                    },
                })
            }
            PendingOperation::SelectLanguage { .. } => Some(PendingResult::SelectLanguage),
        };
        RuntimeCommand::Resume {
            operation: operation_id,
            result,
        }
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

/// 优先复用已经完成 ZIP/哈希校验的发行包配置，开发目录才单独读取 config.toml。
fn resolved_tauri_config(
    game_path: &Path,
    release: Option<&ValidatedNarPackage>,
) -> Result<TauriProjectConfig, HostErrorDto> {
    match release {
        Some(package) => package
            .config_toml()
            .ok_or_else(|| HostErrorDto::new("tauri_host.config", "game.nar 缺少 config.toml"))
            .and_then(|text| {
                TauriProjectConfig::parse(text)
                    .map_err(|error| HostErrorDto::new("tauri_host.config", error.to_string()))
            }),
        None => load_tauri_config(game_path),
    }
}

/// 启动共享 Tauri Host；当前仓库的可运行调用方是桌面入口。
pub fn run(game_path: &str) -> Result<(), HostErrorDto> {
    let root = Path::new(game_path);
    let release_package = load_release_package(root)?;
    let project_config = resolved_tauri_config(root, release_package.as_ref())?;
    let packaged_icon = match (release_package.as_ref(), project_config.icon()) {
        (Some(package), Some(path)) => package
            .resources()
            .read(path)
            .map_err(|error| HostErrorDto::new("tauri_host.resource_read", error.to_string()))?
            .map(<[u8]>::to_vec),
        _ => None,
    };
    let host: TauriHost = TauriHost::spawn_configured(
        game_path,
        project_config.developer(),
        project_config.title().to_owned(),
        release_package,
    )?;
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
            commands::history,
            commands::input,
            commands::host_assets,
            commands::save_game,
            commands::host_logs,
            commands::debug_snapshot,
            commands::debug_execute,
            commands::debug_complete,
            commands::debug_cancel,
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

use worker::{WorkerReply, WorkerRequest, WorkerResponse, run_worker, worker_stopped};

async fn receive_worker<T: Send + 'static>(receiver: Receiver<T>) -> Result<T, HostErrorDto> {
    tokio::task::spawn_blocking(move || receiver.recv())
        .await
        .map_err(|error| HostErrorDto::new("tauri_host.worker_join", error.to_string()))?
        .map_err(|_| worker_stopped())
}

fn ready_update(
    result: Result<RuntimeUpdate, HostErrorDto>,
) -> Result<HostUpdateDto, HostErrorDto> {
    match result? {
        RuntimeUpdate::Ready { update } => Ok(update),
        RuntimeUpdate::Applied => Err(update_expected()),
        RuntimeUpdate::Audio { .. } => unreachable!("Worker consumes audio effects"),
        RuntimeUpdate::Pending { .. } => Err(HostErrorDto::new(
            "tauri_host.pending_update",
            "Host facade 返回了未处理的 PendingOperation",
        )),
    }
}

fn update_expected() -> HostErrorDto {
    HostErrorDto::new(
        "tauri_host.update_expected",
        "Runtime 命令没有产生可展示更新",
    )
}

#[cfg(test)]
mod tests;

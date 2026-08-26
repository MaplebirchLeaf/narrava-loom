//! Narrava Core 与 Tauri IPC 之间的最小 Host Binding。

mod assets;
mod config;
mod developer;
mod package;
mod presentation_bridge;
mod presentation_dto;
mod resource_protocol;
mod save_io;
mod script_runtime;

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use narrava_loom_core::{
    ProjectConfig, SourceList,
    bytecode::BytecodeProgram,
    diagnostic::{Diagnostic, DiagnosticSeverity},
    engine::{
        EngineExecutionLimits, EngineMirContinuation, EngineMirMacroCallbackFailure,
        EngineMirMacroInvocation,
    },
    expression::{
        evaluator::{assign_value_with_mut, evaluate_with_mut, value_to_text},
        parse as parse_expression,
        value::Value,
    },
    hir::{HirBodyKind, HirBodyNode, HirMacro, HirMacroArguments, HirStory, OwnedHirMacro},
    host::{
        HostApi, HostDriveResult, HostInput, HostMirAdvanceRequest, HostMirRequest,
        HostPendingExecutions, HostResumeOutcome, HostUpdate,
    },
    i18n::I18nRuntimeLanguage,
    lir::LirProgram,
    macro_runtime::{
        MacroDefinition, MacroDefinitions, MacroHandlerOutcome, MacroInteractions,
        MacroLocalScopes, MacroLogicContext, MacroResumeOutcome, MacroSuspension,
        RuntimeMacroHandler, button_with_body, checkbox, link_with_body, parse_argument_list,
        prepare_argument_values, radiobutton, replace, slot, text, textbox,
    },
    mir::MirStory,
    presentation::{
        InteractionId, PresentationNode, PresentationOutput, PresentationRegion, PresentationValue,
    },
    resource::ResourceCatalog,
    runtime::{
        BodyControl, BodyExecution, RuntimeExecutionContext, RuntimeExecutionIdentity,
        RuntimeMacroExecution, execute_logic_body,
    },
    state::{State, StateCheckpoint},
    story::{
        Story, StoryRuntimeRequests,
        special::{BAR_PASSAGE, BAR_STOWED_PASSAGE},
    },
    twee,
};
use serde::Serialize;

pub use assets::{HostAssetsDto, HostResourceDto, HostStyleDto};
pub use config::{TauriConfigError, TauriProjectConfig, TauriWindowConfig};
pub use developer::DeveloperValueDto;
pub use presentation_dto::{HostNodeDto, HostReplaceTargetDto, HostUpdateDto};

use developer::{developer_delete, developer_disabled, developer_set, developer_state};
use package::{
    load_language_packages, load_release_config_text, load_release_package, load_tauri_config,
};
use presentation_dto::convert;
use save_io::{process_save, process_save_operation};

type WorkerResult = Result<HostUpdateDto, HostErrorDto>;
type WorkerReply = Sender<WorkerResult>;
type WorkerResponse = Receiver<WorkerResult>;
type DeveloperResult<T> = Result<T, HostErrorDto>;
type InputResult = Result<(), HostErrorDto>;
type CommandResult = Result<(), HostErrorDto>;

/// One bounded diagnostic line exposed by the Host management panel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HostLogDto {
    pub level: String,
    pub message: String,
}

/// IPC 边界只暴露稳定代码与可显示消息，不泄漏 Rust 错误对象。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HostErrorDto {
    pub code: String,
    pub message: String,
}

impl HostErrorDto {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }

    fn diagnostic(diagnostic: Diagnostic) -> Self {
        Self {
            code: diagnostic.code,
            message: diagnostic.message,
        }
    }
}

impl fmt::Display for HostErrorDto {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

enum WorkerRequest {
    Start(WorkerReply),
    Activate {
        interaction: String,
        reply: WorkerReply,
    },
    Input {
        interaction: String,
        value: serde_json::Value,
        reply: Sender<InputResult>,
    },
    Save {
        operation: String,
        target: String,
        reply: Sender<CommandResult>,
    },
    Logs(Sender<Vec<HostLogDto>>),
    Languages(Sender<Vec<String>>),
    SelectLanguage {
        locale: String,
        reply: Sender<CommandResult>,
    },
    DeveloperState(Sender<DeveloperResult<Vec<DeveloperValueDto>>>),
    DeveloperSet {
        namespace: String,
        name: String,
        value: serde_json::Value,
        reply: Sender<DeveloperResult<()>>,
    },
    DeveloperDelete {
        namespace: String,
        name: String,
        reply: Sender<DeveloperResult<()>>,
    },
}

/// Tauri managed state 中保存的轻量句柄。
///
/// 借用 HIR/MIR 的真实 Runtime 长驻专用线程，避免把自引用编译数据塞入平台对象。
#[derive(Clone, Debug)]
pub struct TauriHost {
    requests: Sender<WorkerRequest>,
    assets: Arc<HostAssetsDto>,
    resources: Arc<ResourceCatalog>,
    developer: bool,
}

impl TauriHost {
    pub fn spawn(game_path: &str) -> Result<Self, HostErrorDto> {
        let config = load_tauri_config(Path::new(game_path))?;
        Self::spawn_configured(game_path, config.developer())
    }

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
            .spawn(move || run_worker(game_path, receiver, developer, worker_resources))
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

    pub fn start(&self) -> Result<HostUpdateDto, HostErrorDto> {
        let (reply, result): (WorkerReply, WorkerResponse) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Start(reply))
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())?
    }

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

    pub fn assets(&self) -> HostAssetsDto {
        (*self.assets).clone()
    }

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

    pub fn logs(&self) -> Result<Vec<HostLogDto>, HostErrorDto> {
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Logs(reply))
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())
    }

    pub fn languages(&self) -> Result<Vec<String>, HostErrorDto> {
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Languages(reply))
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())
    }

    pub fn select_language(&self, locale: String) -> CommandResult {
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::SelectLanguage { locale, reply })
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())?
    }

    pub fn developer(&self) -> bool {
        self.developer
    }

    pub fn developer_state(&self) -> DeveloperResult<Vec<DeveloperValueDto>> {
        self.require_developer()?;
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::DeveloperState(reply))
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())?
    }

    pub fn developer_set(
        &self,
        namespace: String,
        name: String,
        value: serde_json::Value,
    ) -> DeveloperResult<()> {
        self.require_developer()?;
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::DeveloperSet {
                namespace,
                name,
                value,
                reply,
            })
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())?
    }

    pub fn developer_delete(&self, namespace: String, name: String) -> DeveloperResult<()> {
        self.require_developer()?;
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::DeveloperDelete {
                namespace,
                name,
                reply,
            })
            .map_err(|_| worker_stopped())?;
        result.recv().map_err(|_| worker_stopped())?
    }

    fn require_developer(&self) -> DeveloperResult<()> {
        if self.developer {
            Ok(())
        } else {
            Err(HostErrorDto::new(
                "tauri_host.developer_disabled",
                "请在 config.toml 的 [host.tauri] 中设置 developer = true",
            ))
        }
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

/// 启动桌面 Host。平台窗口与 WebView 只存在于这个 crate 中。
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
            commands::developer_state,
            commands::developer_set,
            commands::developer_delete,
            commands::toggle_devtools
        ])
        .setup(move |app: &mut tauri::App| {
            build_main_window(app, &game_path, &project_config, packaged_icon.as_deref())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|error: tauri::Error| HostErrorDto::new("tauri_host.app", error.to_string()))
}

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
pub mod commands {
    use tauri::State;

    use super::{
        DeveloperValueDto, HostAssetsDto, HostErrorDto, HostLogDto, HostUpdateDto, TauriHost,
    };

    #[tauri::command]
    pub fn start_game(host: State<'_, TauriHost>) -> Result<HostUpdateDto, HostErrorDto> {
        host.start()
    }

    #[tauri::command]
    pub fn activate(
        interaction: String,
        host: State<'_, TauriHost>,
    ) -> Result<HostUpdateDto, HostErrorDto> {
        host.activate(interaction.as_str())
    }

    #[tauri::command]
    pub fn input(
        interaction: String,
        value: serde_json::Value,
        host: State<'_, TauriHost>,
    ) -> Result<(), HostErrorDto> {
        host.input(interaction, value)
    }

    #[tauri::command]
    pub fn host_assets(host: State<'_, TauriHost>) -> HostAssetsDto {
        host.assets()
    }

    #[tauri::command]
    pub fn save_game(
        operation: String,
        target: String,
        host: State<'_, TauriHost>,
    ) -> Result<(), HostErrorDto> {
        host.save(operation, target)
    }

    #[tauri::command]
    pub fn host_logs(host: State<'_, TauriHost>) -> Result<Vec<HostLogDto>, HostErrorDto> {
        host.logs()
    }

    #[tauri::command]
    pub fn available_languages(host: State<'_, TauriHost>) -> Result<Vec<String>, HostErrorDto> {
        host.languages()
    }

    #[tauri::command]
    pub fn select_language(locale: String, host: State<'_, TauriHost>) -> Result<(), HostErrorDto> {
        host.select_language(locale)
    }

    #[tauri::command]
    pub fn developer_enabled(host: State<'_, TauriHost>) -> bool {
        host.developer()
    }

    #[tauri::command]
    pub fn developer_state(
        host: State<'_, TauriHost>,
    ) -> Result<Vec<DeveloperValueDto>, HostErrorDto> {
        host.developer_state()
    }

    #[tauri::command]
    pub fn developer_set(
        namespace: String,
        name: String,
        value: serde_json::Value,
        host: State<'_, TauriHost>,
    ) -> Result<(), HostErrorDto> {
        host.developer_set(namespace, name, value)
    }

    #[tauri::command]
    pub fn developer_delete(
        namespace: String,
        name: String,
        host: State<'_, TauriHost>,
    ) -> Result<(), HostErrorDto> {
        host.developer_delete(namespace, name)
    }

    #[tauri::command]
    pub fn toggle_devtools(
        window: tauri::WebviewWindow,
        host: State<'_, TauriHost>,
    ) -> Result<(), HostErrorDto> {
        host.require_developer()?;
        if window.is_devtools_open() {
            window.close_devtools();
        } else {
            window.open_devtools();
        }
        Ok(())
    }
}

fn run_worker(
    game_path: String,
    requests: Receiver<WorkerRequest>,
    developer: bool,
    resources: Arc<ResourceCatalog>,
) {
    let release = match load_release_package(Path::new(&game_path)) {
        Ok(package) => package,
        Err(error) => return fail_worker(requests, &error.code, error.message),
    };
    let development_sources;
    let config = if release.is_some() {
        let Some(text) = load_release_config_text(Path::new(&game_path))
            .ok()
            .flatten()
        else {
            return fail_worker(
                requests,
                "tauri_host.config",
                String::from("game.nar 缺少 config.toml"),
            );
        };
        match ProjectConfig::parse(Path::new("game.nar/config.toml"), &text) {
            Ok(config) => config,
            Err(error) => return fail_worker(requests, "tauri_host.config", error.to_string()),
        }
    } else {
        match ProjectConfig::load(game_path.as_str()) {
            Ok(config) => config,
            Err(error) => return fail_worker(requests, "tauri_host.config", error.to_string()),
        }
    };
    let sources = if let Some(package) = &release {
        package.sources()
    } else {
        development_sources = match SourceList::discover(game_path.as_str()) {
            Ok(sources) => sources,
            Err(error) => return fail_worker(requests, "tauri_host.source", error.to_string()),
        };
        &development_sources
    };
    let ast: twee::Story<'_> = match twee::Story::build(&sources.items) {
        Ok(ast) => ast,
        Err(error) => {
            let diagnostic = error.diagnostic();
            return fail_worker(requests, &diagnostic.code, diagnostic.message);
        }
    };
    let hir: HirStory<'_> = match HirStory::lower(&ast) {
        Ok(hir) => hir,
        Err(error) => {
            return fail_worker(requests, &error.diagnostic.code, error.diagnostic.message);
        }
    };
    let mir: MirStory<'_, '_> = match MirStory::lower(&hir) {
        Ok(mir) => mir,
        Err(error) => {
            return fail_worker(
                requests,
                "mir.unsupported_node",
                format!(
                    "HIR 节点 `{}` 尚未定义 MIR 降低（字节 {}..{}）",
                    error.kind, error.span.start, error.span.end
                ),
            );
        }
    };
    let lir: LirProgram<'_, '_, '_> = match LirProgram::lower(&mir) {
        Ok(lir) => lir,
        Err(error) => {
            let instruction = error
                .instruction()
                .map_or_else(String::new, |index| format!("，指令 {index}"));
            return fail_worker(
                requests,
                "lir.lower_failed",
                format!(
                    "Passage `{}`{} 无法生成可执行程序：{:?}",
                    error.passage(),
                    instruction,
                    error.kind()
                ),
            );
        }
    };
    let bytecode: BytecodeProgram = BytecodeProgram::compile(&lir);
    let language_packages = match load_language_packages(Path::new(&game_path), &config) {
        Ok(packages) => packages,
        Err(error) => return fail_worker(requests, &error.code, error.message),
    };
    let mut available_languages = vec![config.game.default_locale.clone()];
    available_languages.extend(
        language_packages
            .iter()
            .map(|package| package.manifest().manifest().locale().to_owned()),
    );
    available_languages.sort();
    available_languages.dedup();
    let mut runtime_language: Option<I18nRuntimeLanguage> = None;
    let mut state: State = State::new();
    let script = match script_runtime::EcmaBinding::load(
        sources,
        &resources,
        mir.i18n(),
        &config.game.default_locale,
        &mut state,
    ) {
        Ok(script) => script,
        Err(error) => return fail_worker(requests, error.code.as_str(), error.message),
    };
    state.attach_script_dispatcher(script.clone());
    let mut story: Story<'_, '_> = Story::new(&hir);
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();
    let mut pending: HostPendingExecutions<
        EngineMirContinuation<'_, '_, script_runtime::ScriptPending>,
    > = HostPendingExecutions::new();
    let mut presented: Option<HostUpdate> = None;
    let mut sequence: u64 = 1;
    let mut logs: Vec<HostLogDto> = vec![HostLogDto {
        level: String::from("info"),
        message: String::from("Runtime Worker 已就绪"),
    }];

    while let Ok(request) = requests.recv() {
        match request {
            WorkerRequest::Start(reply) => {
                let params: Value = Value::Null;
                let mut scheduled: Option<script_runtime::ScriptPending> = None;
                let result = HostApi::start_mir(
                    &mut pending,
                    &mut state,
                    &mut story,
                    &bytecode,
                    HostMirRequest {
                        params: &params,
                        identity: RuntimeExecutionIdentity::new(1, sequence),
                        limits: limits(),
                        language: runtime_language.as_ref(),
                    },
                    |_passage, _state, _requests, _limits| {
                        Ok::<BodyExecution, Diagnostic>(BodyExecution::default())
                    },
                    |phase, context, _state| emit_passage_event(&script, phase, context),
                    |invocation, state, requests, scopes| {
                        dispatch_macro(
                            &script,
                            &hir,
                            &mut interactions,
                            &mut scheduled,
                            invocation,
                            state,
                            requests,
                            scopes,
                        )
                    },
                )
                .map_err(|error| HostErrorDto::diagnostic(error.diagnostic.clone()));
                let mut result = finish_drive(
                    result,
                    &script,
                    &mut scheduled,
                    &mut pending,
                    &hir,
                    &mut interactions,
                    &mut state,
                    &mut story,
                    &bytecode,
                );
                sequence = sequence.saturating_add(1);
                if let Ok(update) = &mut result
                    && let Err(error) = append_sidebar_regions(
                        update,
                        &script,
                        &hir,
                        &mut interactions,
                        &state,
                        &story,
                        &bytecode,
                        runtime_language.as_ref(),
                        &mut sequence,
                    )
                {
                    result = Err(error);
                }
                if let Ok(update) = &result {
                    presented = Some(update.clone());
                }
                if result.is_ok()
                    && let Err(error) = process_save(
                        Path::new(&game_path),
                        &config,
                        &script,
                        &mut state,
                        &mut story,
                    )
                {
                    result = Err(error);
                }
                let _sent: Result<(), _> = reply.send(result.map(|update| convert(&update)));
            }
            WorkerRequest::Activate { interaction, reply } => {
                let Some(previous) = presented.as_ref() else {
                    let _sent: Result<(), _> = reply.send(Err(HostErrorDto::new(
                        "tauri_host.not_started",
                        "必须先调用 start_game",
                    )));
                    continue;
                };
                let params: Value = Value::Null;
                let mut scheduled: Option<script_runtime::ScriptPending> = None;
                let interaction_id = match InteractionId::parse(interaction) {
                    Ok(id) => id,
                    Err(error) => {
                        let _sent: Result<(), _> = reply.send(Err(HostErrorDto::new(
                            "tauri_host.interaction",
                            error.to_string(),
                        )));
                        continue;
                    }
                };
                let request = HostMirAdvanceRequest {
                    presented: previous,
                    input: HostInput::activate(interaction_id.clone()),
                    params: &params,
                    identity: RuntimeExecutionIdentity::new(1, sequence),
                    limits: limits(),
                    language: runtime_language.as_ref(),
                };
                let result = if interactions.has(&interaction_id) {
                    let mut next_interactions: MacroInteractions<'_, '_> = MacroInteractions::new();
                    let result = HostApi::advance_macro_interaction_mir(
                        &mut pending,
                        &mut interactions,
                        &mut state,
                        &mut story,
                        &bytecode,
                        request,
                        |body, state, requests, scopes| {
                            let mut context = MacroLogicContext::new(state, requests, scopes);
                            let control =
                                execute_logic_body(body, &mut context).map_err(|error| {
                                    Diagnostic::new(
                                        "tauri_host.interaction_body",
                                        DiagnosticSeverity::Error,
                                        &format!("Interaction 正文执行失败：{error:?}"),
                                    )
                                })?;
                            Ok(BodyExecution {
                                control,
                                output: PresentationOutput::default(),
                            })
                        },
                        |phase, context, _state| emit_passage_event(&script, phase, context),
                        |invocation, state, requests, scopes| {
                            dispatch_macro(
                                &script,
                                &hir,
                                &mut next_interactions,
                                &mut scheduled,
                                invocation,
                                state,
                                requests,
                                scopes,
                            )
                        },
                    );
                    if result.is_ok() {
                        interactions = next_interactions;
                    }
                    result
                } else {
                    HostApi::advance_mir(
                        &mut pending,
                        &mut state,
                        &mut story,
                        &bytecode,
                        request,
                        |phase, context, _state| emit_passage_event(&script, phase, context),
                        |invocation, state, requests, scopes| {
                            dispatch_macro(
                                &script,
                                &hir,
                                &mut interactions,
                                &mut scheduled,
                                invocation,
                                state,
                                requests,
                                scopes,
                            )
                        },
                    )
                }
                .map_err(|error| HostErrorDto::diagnostic(error.diagnostic.clone()));
                let mut result = finish_drive(
                    result,
                    &script,
                    &mut scheduled,
                    &mut pending,
                    &hir,
                    &mut interactions,
                    &mut state,
                    &mut story,
                    &bytecode,
                );
                sequence = sequence.saturating_add(1);
                if let Ok(update) = &mut result
                    && let Err(error) = append_sidebar_regions(
                        update,
                        &script,
                        &hir,
                        &mut interactions,
                        &state,
                        &story,
                        &bytecode,
                        runtime_language.as_ref(),
                        &mut sequence,
                    )
                {
                    result = Err(error);
                }
                if let Ok(update) = &result {
                    presented = Some(update.clone());
                }
                if result.is_ok()
                    && let Err(error) = process_save(
                        Path::new(&game_path),
                        &config,
                        &script,
                        &mut state,
                        &mut story,
                    )
                {
                    result = Err(error);
                }
                let _sent: Result<(), _> = reply.send(result.map(|update| convert(&update)));
            }
            WorkerRequest::Input {
                interaction,
                value,
                reply,
            } => {
                let result: InputResult = (|| {
                    let previous: &HostUpdate = presented.as_ref().ok_or_else(|| {
                        HostErrorDto::new("tauri_host.not_started", "必须先调用 start_game")
                    })?;
                    let id: InteractionId = InteractionId::parse(interaction).map_err(|error| {
                        HostErrorDto::new("tauri_host.input_interaction", error.to_string())
                    })?;
                    let binding = previous.presentation().input_binding(&id).ok_or_else(|| {
                        HostErrorDto::new(
                            "tauri_host.unknown_input",
                            "输入身份未出现在上一份 Presentation 中",
                        )
                    })?;
                    let semantic: PresentationValue = json_to_presentation_value(&value)?;
                    if !binding.accepts(&semantic) {
                        return Err(HostErrorDto::new(
                            "tauri_host.input_value",
                            "输入值不属于当前控件允许的值集合",
                        ));
                    }
                    let expression =
                        parse_expression(binding.receiver.as_str()).map_err(|error| {
                            HostErrorDto::new(
                                "tauri_host.input_receiver",
                                format!("输入 receiver 无效：{error:?}"),
                            )
                        })?;
                    let value: Value = script_runtime::json_to_value(&value)?;
                    let checkpoint: StateCheckpoint = state.checkpoint();
                    if let Err(error) = assign_value_with_mut(&expression, value, &mut state) {
                        state.restore_checkpoint(checkpoint);
                        return Err(HostErrorDto::new(
                            "tauri_host.input_assignment",
                            format!("输入值无法写回：{error:?}"),
                        ));
                    }
                    if let Err(error) = process_save(
                        Path::new(&game_path),
                        &config,
                        &script,
                        &mut state,
                        &mut story,
                    ) {
                        state.restore_checkpoint(checkpoint);
                        return Err(error);
                    }
                    Ok(())
                })();
                let _sent = reply.send(result);
            }
            WorkerRequest::Save {
                operation,
                target,
                reply,
            } => {
                let result = process_save_operation(
                    Path::new(&game_path),
                    &config,
                    &mut state,
                    &mut story,
                    &operation,
                    &target,
                );
                if result.is_ok()
                    && operation == "import"
                    && let Err(error) = script.sync_variables(&state)
                {
                    let _sent = reply.send(Err(error));
                    continue;
                }
                let level = if result.is_ok() { "info" } else { "error" };
                logs.push(HostLogDto {
                    level: String::from(level),
                    message: match &result {
                        Ok(()) => format!("Save.{operation}({target}) 已完成"),
                        Err(error) => format!("{}：{}", error.code, error.message),
                    },
                });
                if logs.len() > 200 {
                    logs.remove(0);
                }
                let _sent = reply.send(result);
            }
            WorkerRequest::Logs(reply) => {
                let _sent = reply.send(logs.clone());
            }
            WorkerRequest::Languages(reply) => {
                let _sent = reply.send(available_languages.clone());
            }
            WorkerRequest::SelectLanguage { locale, reply } => {
                let result = I18nRuntimeLanguage::select(
                    mir.i18n(),
                    &config.game.default_locale,
                    &locale,
                    language_packages.clone(),
                )
                .map(|selected| runtime_language = selected)
                .map_err(|error| {
                    HostErrorDto::new("tauri_host.language_select", error.to_string())
                });
                if result.is_ok() {
                    logs.push(HostLogDto {
                        level: String::from("info"),
                        message: format!("语言已切换为 {locale}；下一次渲染生效"),
                    });
                }
                let _sent = reply.send(result);
            }
            WorkerRequest::DeveloperState(reply) => {
                let result = if developer {
                    Ok(developer_state(&state))
                } else {
                    Err(developer_disabled())
                };
                let _sent = reply.send(result);
            }
            WorkerRequest::DeveloperSet {
                namespace,
                name,
                value,
                reply,
            } => {
                let result = if developer {
                    developer_set(&script, &mut state, &namespace, &name, &value)
                } else {
                    Err(developer_disabled())
                };
                let _sent = reply.send(result);
            }
            WorkerRequest::DeveloperDelete {
                namespace,
                name,
                reply,
            } => {
                let result = if developer {
                    developer_delete(&script, &mut state, &namespace, &name)
                } else {
                    Err(developer_disabled())
                };
                let _sent = reply.send(result);
            }
        }
    }
}

/// 将 Engine 的事务生命周期映射为稳定的作者可订阅事件名。
///
/// 事件在对应阶段同步写入 Worker 队列；任何绑定错误都会作为生命周期错误触发
/// Engine 回滚，避免出现“导航已提交但生命周期事实丢失”的半成功状态。
fn emit_passage_event(
    script: &script_runtime::EcmaBinding,
    phase: narrava_loom_core::engine::PassageLifecyclePhase,
    context: narrava_loom_core::engine::PassageLifecycleContext<'_, '_, '_, '_>,
) -> Result<(), Diagnostic> {
    let name = match phase {
        narrava_loom_core::engine::PassageLifecyclePhase::Init => "passage:init",
        narrava_loom_core::engine::PassageLifecyclePhase::Start => "passage:start",
        narrava_loom_core::engine::PassageLifecyclePhase::Render => "passage:render",
        narrava_loom_core::engine::PassageLifecyclePhase::Display => "passage:display",
        narrava_loom_core::engine::PassageLifecyclePhase::End => "passage:end",
    };
    let passage = context.entry().passage();
    script
        .emit_builtin_event(
            name,
            &serde_json::json!({ "passage": passage.name, "tags": passage.tags }),
        )
        .map(|_| ())
        .map_err(|error| {
            Diagnostic::new(
                "tauri_host.passage_event",
                DiagnosticSeverity::Error,
                &error.message,
            )
        })
}

#[allow(clippy::too_many_arguments)]
fn append_sidebar_regions<'hir, 'source>(
    update: &mut HostUpdate,
    script: &script_runtime::EcmaBinding,
    hir: &'hir HirStory<'source>,
    interactions: &mut MacroInteractions<'hir, 'source>,
    state: &State,
    story: &Story<'hir, 'source>,
    bytecode: &BytecodeProgram,
    language: Option<&I18nRuntimeLanguage>,
    sequence: &mut u64,
) -> Result<(), HostErrorDto> {
    for (name, region) in [
        (BAR_PASSAGE, PresentationRegion::Bar),
        (BAR_STOWED_PASSAGE, PresentationRegion::BarStowed),
    ] {
        if !story.has(name) {
            continue;
        }
        let mut view_state: State = state.fork_view();
        let mut view_story: Story<'hir, 'source> = story.fork_view();
        let mut pending: HostPendingExecutions<
            EngineMirContinuation<'hir, 'source, script_runtime::ScriptPending>,
        > = HostPendingExecutions::new();
        let mut scheduled: Option<script_runtime::ScriptPending> = None;
        let params: Value = Value::Null;
        let result = HostApi::render_special_mir(
            &mut pending,
            &mut view_state,
            &mut view_story,
            bytecode,
            name,
            HostMirRequest {
                params: &params,
                identity: RuntimeExecutionIdentity::new(2, *sequence),
                limits: limits(),
                language,
            },
            |_phase, _context, _state| Ok::<(), Diagnostic>(()),
            |invocation, state, requests, scopes| {
                dispatch_macro(
                    script,
                    hir,
                    interactions,
                    &mut scheduled,
                    invocation,
                    state,
                    requests,
                    scopes,
                )
            },
        )
        .map_err(|error| HostErrorDto::diagnostic(error.diagnostic.clone()));
        let rendered: HostUpdate = finish_drive(
            result,
            script,
            &mut scheduled,
            &mut pending,
            hir,
            interactions,
            &mut view_state,
            &mut view_story,
            bytecode,
        )?;
        update.append_region(region, rendered.presentation().clone());
        *sequence = sequence.saturating_add(1);
    }
    Ok(())
}

// 这些参数分别由 Engine、脚本 Worker 与当前事务持有；合并为长期上下文会扩大
// 可变借用范围，并让 start/activate 两条路径更难复用。
#[allow(clippy::too_many_arguments)]
fn finish_drive<'hir, 'source>(
    mut result: Result<HostDriveResult, HostErrorDto>,
    script: &script_runtime::EcmaBinding,
    scheduled: &mut Option<script_runtime::ScriptPending>,
    pending: &mut HostPendingExecutions<
        EngineMirContinuation<'hir, 'source, script_runtime::ScriptPending>,
    >,
    hir: &'hir HirStory<'source>,
    interactions: &mut MacroInteractions<'hir, 'source>,
    state: &mut State,
    story: &mut Story<'hir, 'source>,
    bytecode: &BytecodeProgram,
) -> Result<HostUpdate, HostErrorDto> {
    loop {
        let execution = match result? {
            HostDriveResult::Ready(update) => return Ok(update),
            HostDriveResult::Pending { execution } => execution,
        };
        let operation = scheduled.take().ok_or_else(|| {
            HostErrorDto::new(
                "tauri_host.pending_without_operation",
                "Core 已暂停，但 Host 没有对应的异步操作",
            )
        })?;
        thread::sleep(operation.delay());

        let resumed = HostApi::resume_pending(
            pending,
            state,
            story,
            bytecode,
            execution,
            |handle, state, _requests, _scopes| match script.resume_macro(handle, state) {
                Ok(script_runtime::ScriptMacroOutcome::Complete(value)) => {
                    macro_value_execution(&value)
                        .map(MacroHandlerOutcome::Complete)
                        .map_err(|error| error.to_string())
                }
                Ok(script_runtime::ScriptMacroOutcome::Pending(next)) => {
                    *scheduled = Some(next.clone());
                    Ok(MacroHandlerOutcome::Pending(next))
                }
                Err(error) => Err(error.to_string()),
            },
        )
        .map_err(HostErrorDto::diagnostic)?;

        result = match resumed {
            HostResumeOutcome::Pending { execution } => Ok(HostDriveResult::Pending { execution }),
            HostResumeOutcome::Continue(resumed) => {
                let stable = HostApi::continue_resumed(*resumed, state, story, bytecode)
                    .map_err(HostErrorDto::diagnostic)?;
                HostApi::drive_stable(
                    stable,
                    pending,
                    state,
                    story,
                    bytecode,
                    |phase, context, _state| emit_passage_event(script, phase, context),
                    |invocation, state, requests, scopes| {
                        dispatch_macro(
                            script,
                            hir,
                            interactions,
                            scheduled,
                            invocation,
                            state,
                            requests,
                            scopes,
                        )
                    },
                )
                .map_err(|error| HostErrorDto::diagnostic(error.diagnostic.clone()))
            }
        };
    }
}

fn fail_worker(requests: Receiver<WorkerRequest>, code: &str, message: String) {
    for request in requests {
        let reply: WorkerReply = match request {
            WorkerRequest::Start(reply) | WorkerRequest::Activate { reply, .. } => reply,
            WorkerRequest::DeveloperState(reply) => {
                let _sent = reply.send(Err(HostErrorDto::new(code, message.clone())));
                continue;
            }
            WorkerRequest::Input { reply, .. } => {
                let _sent = reply.send(Err(HostErrorDto::new(code, message.clone())));
                continue;
            }
            WorkerRequest::DeveloperSet { reply, .. }
            | WorkerRequest::DeveloperDelete { reply, .. }
            | WorkerRequest::Save { reply, .. }
            | WorkerRequest::SelectLanguage { reply, .. } => {
                let _sent = reply.send(Err(HostErrorDto::new(code, message.clone())));
                continue;
            }
            WorkerRequest::Logs(reply) => {
                let _sent = reply.send(vec![HostLogDto {
                    level: String::from("error"),
                    message: format!("{code}：{message}"),
                }]);
                continue;
            }
            WorkerRequest::Languages(reply) => {
                let _sent = reply.send(Vec::new());
                continue;
            }
        };
        let _sent: Result<(), _> = reply.send(Err(HostErrorDto::new(code, message.clone())));
    }
}

fn worker_stopped() -> HostErrorDto {
    HostErrorDto::new("tauri_host.worker_stopped", "Narrava Runtime Worker 已停止")
}

fn json_to_presentation_value(
    value: &serde_json::Value,
) -> Result<PresentationValue, HostErrorDto> {
    match value {
        serde_json::Value::Null => Ok(PresentationValue::Null),
        serde_json::Value::Bool(value) => Ok(PresentationValue::Boolean(*value)),
        serde_json::Value::Number(value) => value
            .as_f64()
            .filter(|value: &f64| value.is_finite())
            .map(PresentationValue::Number)
            .ok_or_else(|| HostErrorDto::new("tauri_host.input_value", "输入数值超出范围")),
        serde_json::Value::String(value) => Ok(PresentationValue::Text(value.clone())),
        serde_json::Value::Array(values) => values
            .iter()
            .map(json_to_presentation_value)
            .collect::<Result<Vec<_>, _>>()
            .map(PresentationValue::List),
        serde_json::Value::Object(values) => values
            .iter()
            .map(|(name, value)| Ok((name.clone(), json_to_presentation_value(value)?)))
            .collect::<Result<std::collections::BTreeMap<_, _>, HostErrorDto>>()
            .map(PresentationValue::Map),
    }
}

fn limits() -> EngineExecutionLimits {
    EngineExecutionLimits {
        passages: 8,
        includes: 32,
    }
}

// Macro 回调是 Core 与 Host 的窄适配边界。参数刻意保持显式，避免把短生命周期的
// State、Story 请求和局部作用域藏进可跨暂停点保存的对象。
#[allow(clippy::too_many_arguments)]
fn dispatch_macro<'hir, 'source>(
    script: &script_runtime::EcmaBinding,
    hir: &'hir HirStory<'source>,
    interactions: &mut MacroInteractions<'hir, 'source>,
    scheduled: &mut Option<script_runtime::ScriptPending>,
    invocation: EngineMirMacroInvocation<'_>,
    state: &mut State,
    requests: &mut StoryRuntimeRequests<'_, 'hir, 'source>,
    mut scopes: MacroLocalScopes<Value>,
) -> Result<
    MacroResumeOutcome<RuntimeMacroExecution, script_runtime::ScriptPending>,
    EngineMirMacroCallbackFailure<String>,
> {
    let call: HirMacro<'_> = invocation.call.as_hir();
    let raw: &str = match &call.arguments {
        HirMacroArguments::Raw(raw) => raw,
        HirMacroArguments::None => "",
        HirMacroArguments::Expression(_) => {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("脚本 Macro 暂不接受编译器 Expression 参数：{}", call.name),
                scopes,
            });
        }
    };
    if call.name == "text" {
        let parsed = parse_argument_list(raw).map_err(|error| EngineMirMacroCallbackFailure {
            error: format!("text 参数无效：{error:?}"),
            scopes: scopes.clone(),
        })?;
        let arguments: Vec<Value> = {
            let mut context = MacroLogicContext::new(state, requests, &mut scopes);
            prepare_argument_values(&parsed, |expression| {
                evaluate_with_mut(expression, &mut context)
            })
            .map_err(|error| EngineMirMacroCallbackFailure {
                error: format!("text 参数无法求值：{error:?}"),
                scopes: scopes.clone(),
            })?
        };
        let execution = text(&arguments).map_err(|error| EngineMirMacroCallbackFailure {
            error: error.to_string(),
            scopes: scopes.clone(),
        })?;
        return Ok(MacroResumeOutcome::Complete {
            output: RuntimeMacroExecution {
                execution,
                includes_entered: 0,
            },
            scopes,
        });
    }
    if matches!(call.name, "replace" | "slot") {
        let parsed = parse_argument_list(raw).map_err(|error| EngineMirMacroCallbackFailure {
            error: format!("replace 参数无效：{error:?}"),
            scopes: scopes.clone(),
        })?;
        let arguments: Vec<Value> = {
            let mut context = MacroLogicContext::new(state, requests, &mut scopes);
            prepare_argument_values(&parsed, |expression| {
                evaluate_with_mut(expression, &mut context)
            })
            .map_err(|error| EngineMirMacroCallbackFailure {
                error: format!("replace 参数无法求值：{error:?}"),
                scopes: scopes.clone(),
            })?
        };
        let [Value::String(target)] = arguments.as_slice() else {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("{} 必须接收一个文字 key", call.name),
                scopes,
            });
        };
        let target: String =
            target
                .to_unicode_string()
                .ok_or_else(|| EngineMirMacroCallbackFailure {
                    error: format!("{} key 必须是有效 Unicode", call.name),
                    scopes: scopes.clone(),
                })?;
        let source_call: &'hir HirMacro<'source> = find_hir_macro(hir, invocation.call)
            .ok_or_else(|| EngineMirMacroCallbackFailure {
                error: format!("无法从原始 HIR 找回 {} 容器正文", call.name),
                scopes: scopes.clone(),
            })?;
        let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'hir, 'source, ()>>> =
            MacroDefinitions::new();
        let body_execution = {
            let mut runtime =
                RuntimeExecutionContext::new(&definitions, state, requests, &mut scopes);
            runtime.execute_fragment(source_call.body.as_slice())
        }
        .map_err(|error| EngineMirMacroCallbackFailure {
            error: format!("replace 正文执行失败：{error:?}"),
            scopes: scopes.clone(),
        })?;
        if !matches!(
            body_execution.control,
            BodyControl::Continue | BodyControl::ExitScope
        ) {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("{} 正文不能中断 Passage 或发起导航", call.name),
                scopes,
            });
        }
        let execution: BodyExecution = if call.name == "slot" {
            slot(target.as_str(), body_execution.output)
        } else {
            replace(target.as_str(), body_execution.output)
        }
        .map_err(|error| EngineMirMacroCallbackFailure {
            error: error.to_string(),
            scopes: scopes.clone(),
        })?;
        return Ok(MacroResumeOutcome::Complete {
            output: RuntimeMacroExecution {
                execution,
                includes_entered: 0,
            },
            scopes,
        });
    }
    if matches!(call.name, "checkbox" | "radiobutton" | "textbox") {
        let parsed = parse_argument_list(raw).map_err(|error| EngineMirMacroCallbackFailure {
            error: format!("{} 参数无效：{error:?}", call.name),
            scopes: scopes.clone(),
        })?;
        let arguments: Vec<Value> = {
            let mut context: MacroLogicContext<'_, StoryRuntimeRequests<'_, 'hir, 'source>> =
                MacroLogicContext::new(state, requests, &mut scopes);
            prepare_argument_values(&parsed, |expression| {
                evaluate_with_mut(expression, &mut context)
            })
            .map_err(|error| EngineMirMacroCallbackFailure {
                error: format!("{} 参数无法求值：{error:?}", call.name),
                scopes: scopes.clone(),
            })?
        };
        let receiver: String = match arguments.first() {
            Some(Value::String(value)) => {
                value
                    .to_unicode_string()
                    .ok_or_else(|| EngineMirMacroCallbackFailure {
                        error: format!("{} receiver 必须是有效 Unicode", call.name),
                        scopes: scopes.clone(),
                    })?
            }
            _ => {
                return Err(EngineMirMacroCallbackFailure {
                    error: format!("{} 第一个参数必须是带引号的 receiver", call.name),
                    scopes,
                });
            }
        };
        if receiver.starts_with('@') {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("{} 暂不支持 @ receiver", call.name),
                scopes,
            });
        }
        let receiver_expression =
            parse_expression(receiver.as_str()).map_err(|error| EngineMirMacroCallbackFailure {
                error: format!("{} receiver 无效：{error:?}", call.name),
                scopes: scopes.clone(),
            })?;
        if !receiver_expression.is_assignable_target() {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("{} receiver 不是可写目标", call.name),
                scopes,
            });
        }
        let current_result = {
            let mut context: MacroLogicContext<'_, StoryRuntimeRequests<'_, 'hir, 'source>> =
                MacroLogicContext::new(state, requests, &mut scopes);
            evaluate_with_mut(&receiver_expression, &mut context)
        };
        let mut current: Value = current_result.map_err(|error| EngineMirMacroCallbackFailure {
            error: format!("{} receiver 无法读取：{error:?}", call.name),
            scopes: scopes.clone(),
        })?;
        if call.name == "textbox" && matches!(current, Value::Undefined) {
            let default: Value =
                arguments
                    .get(1)
                    .cloned()
                    .ok_or_else(|| EngineMirMacroCallbackFailure {
                        error: "textbox 需要 receiver 与默认值".to_owned(),
                        scopes: scopes.clone(),
                    })?;
            let assignment = {
                let mut context: MacroLogicContext<'_, StoryRuntimeRequests<'_, 'hir, 'source>> =
                    MacroLogicContext::new(state, requests, &mut scopes);
                assign_value_with_mut(&receiver_expression, default.clone(), &mut context)
            };
            assignment.map_err(|error| EngineMirMacroCallbackFailure {
                error: format!("textbox 默认值无法写入：{error:?}"),
                scopes: scopes.clone(),
            })?;
            current = default;
        }
        let execution: BodyExecution = match (call.name, arguments.as_slice()) {
            ("checkbox", [_, unchecked, checked]) => checkbox(
                receiver.as_str(),
                unchecked,
                checked,
                &current,
                invocation.identity,
                invocation.location.instruction().index(),
            ),
            ("radiobutton", [_, value]) => radiobutton(
                receiver.as_str(),
                value,
                &current,
                invocation.identity,
                invocation.location.instruction().index(),
            ),
            ("textbox", [_, _]) => textbox(
                receiver.as_str(),
                &current,
                invocation.identity,
                invocation.location.instruction().index(),
            ),
            _ => Err(Diagnostic::new(
                "macro.input.invalid_arguments",
                DiagnosticSeverity::Error,
                &format!("{} 参数数量不正确", call.name),
            )),
        }
        .map_err(|error| EngineMirMacroCallbackFailure {
            error: error.to_string(),
            scopes: scopes.clone(),
        })?;
        return Ok(MacroResumeOutcome::Complete {
            output: RuntimeMacroExecution {
                execution,
                includes_entered: 0,
            },
            scopes,
        });
    }
    if !matches!(call.name, "link" | "button") {
        let exists =
            script
                .has_macro(call.name)
                .map_err(|error| EngineMirMacroCallbackFailure {
                    error: error.to_string(),
                    scopes: scopes.clone(),
                })?;
        if !exists {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("Macro 不存在：{}", call.name),
                scopes,
            });
        }
        let outcome = script.call_macro(call.name, raw, state).map_err(|error| {
            EngineMirMacroCallbackFailure {
                error: error.to_string(),
                scopes: scopes.clone(),
            }
        })?;
        let value: Value = match outcome {
            script_runtime::ScriptMacroOutcome::Complete(value) => value,
            script_runtime::ScriptMacroOutcome::Pending(handle) => {
                scopes.enter_call(Vec::new());
                *scheduled = Some(handle.clone());
                let suspended =
                    scopes
                        .suspend()
                        .map_err(|error| EngineMirMacroCallbackFailure {
                            error: format!("Macro 局部域无法暂停：{error:?}"),
                            scopes: MacroLocalScopes::new(),
                        })?;
                return Ok(MacroResumeOutcome::Pending(MacroSuspension {
                    identity: invocation.identity,
                    handle,
                    scopes: suspended,
                }));
            }
        };
        let execution: RuntimeMacroExecution =
            macro_value_execution(&value).map_err(|error| EngineMirMacroCallbackFailure {
                error: error.to_string(),
                scopes: scopes.clone(),
            })?;
        return Ok(MacroResumeOutcome::Complete {
            output: execution,
            scopes,
        });
    }
    let parsed = parse_argument_list(raw).map_err(|error| EngineMirMacroCallbackFailure {
        error: format!("link 参数无效：{error:?}"),
        scopes: scopes.clone(),
    })?;
    let arguments: Vec<Value> =
        prepare_argument_values(&parsed, |_expression| Err::<Value, ()>(())).map_err(|error| {
            EngineMirMacroCallbackFailure {
                error: format!("link 参数不能求值：{error:?}"),
                scopes: scopes.clone(),
            }
        })?;
    let source_call: &'hir HirMacro<'source> =
        find_hir_macro(hir, invocation.call).ok_or_else(|| EngineMirMacroCallbackFailure {
            error: format!("无法从原始 HIR 找回 {} 容器正文", call.name),
            scopes: scopes.clone(),
        })?;
    let execution: BodyExecution = if call.name == "button" {
        button_with_body(
            &arguments,
            invocation.identity,
            source_call.body.as_slice(),
            invocation.captures,
            interactions,
        )
    } else {
        link_with_body(
            &arguments,
            invocation.identity,
            source_call.body.as_slice(),
            invocation.captures,
            interactions,
        )
    }
    .map_err(|error| EngineMirMacroCallbackFailure {
        error: format!("{} 执行失败：{error:?}", call.name),
        scopes: scopes.clone(),
    })?;
    Ok(MacroResumeOutcome::Complete {
        output: RuntimeMacroExecution {
            execution,
            includes_entered: 0,
        },
        scopes,
    })
}

fn find_hir_macro<'hir, 'source>(
    story: &'hir HirStory<'source>,
    owned: &OwnedHirMacro,
) -> Option<&'hir HirMacro<'source>> {
    story
        .passages
        .iter()
        .find_map(|passage| find_hir_macro_in_body(&passage.body, owned))
}

fn find_hir_macro_in_body<'hir, 'source>(
    body: &'hir [HirBodyNode<'source>],
    owned: &OwnedHirMacro,
) -> Option<&'hir HirMacro<'source>> {
    body.iter().find_map(|node| match &node.kind {
        HirBodyKind::Macro(call) if OwnedHirMacro::from(call) == *owned => Some(call),
        HirBodyKind::Macro(call) => find_hir_macro_in_body(&call.body, owned),
        HirBodyKind::If(conditional) => conditional
            .branches
            .iter()
            .find_map(|branch| find_hir_macro_in_body(&branch.body, owned))
            .or_else(|| {
                conditional
                    .fallback
                    .as_deref()
                    .and_then(|body| find_hir_macro_in_body(body, owned))
            }),
        HirBodyKind::Switch(switch) => switch
            .cases
            .iter()
            .find_map(|case| find_hir_macro_in_body(&case.body, owned))
            .or_else(|| {
                switch
                    .default
                    .as_deref()
                    .and_then(|body| find_hir_macro_in_body(body, owned))
            }),
        HirBodyKind::For(loop_node) => find_hir_macro_in_body(&loop_node.body, owned),
        HirBodyKind::While(loop_node) => find_hir_macro_in_body(&loop_node.body, owned),
        HirBodyKind::Silently(body) => find_hir_macro_in_body(body, owned),
        HirBodyKind::Widget(widget) => find_hir_macro_in_body(&widget.body, owned),
        HirBodyKind::Capture(capture) => find_hir_macro_in_body(&capture.body, owned),
        _ => None,
    })
}

fn macro_value_execution(value: &Value) -> Result<RuntimeMacroExecution, HostErrorDto> {
    let output: PresentationOutput = match presentation_bridge::output(value)? {
        Some(output) => output,
        None => {
            let mut output = PresentationOutput::default();
            if let Some(text) = value_to_text(value) {
                output.push(PresentationNode::Text(text));
            }
            output
        }
    };
    Ok(RuntimeMacroExecution {
        execution: BodyExecution {
            control: narrava_loom_core::runtime::BodyControl::Continue,
            output,
        },
        includes_entered: 0,
    })
}

#[cfg(test)]
mod release_tests {
    use std::{fs, path::Path};

    use narrava_loom_core::{
        ProjectConfig, SourceList, nar::NarPackage, package_zip, resource::ResourceCatalog,
    };

    use super::{HostNodeDto, HostUpdateDto, TauriHost};
    use crate::save_io::save_file_name;

    #[test]
    fn host_delay_suspends_and_resumes_the_engine_transaction() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let root = format!("target/test-projects/host-delay-{}", std::process::id());
        let root_path = Path::new(&root);
        fs::create_dir_all(root_path.join("contents/scripts")).unwrap();
        fs::create_dir_all(root_path.join("contents/story")).unwrap();
        fs::copy(
            repository.join("examples/config.toml"),
            root_path.join("config.toml"),
        )
        .unwrap();
        fs::write(
            root_path.join("contents/story/main.twee"),
            ":: Start\n<<delayed>><</delayed>>",
        )
        .unwrap();
        fs::write(
            root_path.join("contents/scripts/main.js"),
            "Macro.add('delayed', { body: 'inline', arguments: 'raw', execution: 'async', handler: async () => { await Host.delay(1); return 'resumed' } })",
        )
        .unwrap();

        let host = TauriHost::spawn(&root).unwrap();
        let update = host.start().expect("Tauri Host 应恢复异步事务");
        assert_eq!(update.current, "Start");
        assert!(
            update.nodes.iter().any(|node| {
                matches!(node, HostNodeDto::Text { text, .. } if text == "resumed")
            })
        );

        drop(host);
        fs::remove_dir_all(root_path).unwrap();
    }

    #[test]
    fn example_presentation_builder_reaches_tauri_semantic_dtos() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let root = format!(
            "target/test-projects/presentation-example-{}",
            std::process::id()
        );
        let root_path = Path::new(&root);
        if root_path.exists() {
            fs::remove_dir_all(root_path).unwrap();
        }
        for directory in [
            "contents/scripts",
            "contents/story",
            "resources/data",
            "resources/images",
        ] {
            fs::create_dir_all(root_path.join(directory)).unwrap();
        }
        for file in [
            "config.toml",
            "contents/scripts/main.ts",
            "contents/story/main.twee",
            "contents/story/widgets.twee",
            "resources/data/guide.txt",
            "resources/images/loom.svg",
        ] {
            fs::copy(repository.join("examples").join(file), root_path.join(file)).unwrap();
        }
        let host = TauriHost::spawn(&root).unwrap();
        let start = host.start().unwrap();
        let hall_id = start
            .nodes
            .iter()
            .find_map(|node| match node {
                HostNodeDto::Navigation { id, target, .. } if target == "Hall" => Some(id),
                _ => None,
            })
            .unwrap();
        let hall = host.activate(hall_id).unwrap();
        let gallery_id = hall
            .nodes
            .iter()
            .find_map(|node| match node {
                HostNodeDto::Navigation { id, target, .. } if target == "PresentationGallery" => {
                    Some(id)
                }
                _ => None,
            })
            .unwrap();

        let gallery = host.activate(gallery_id).unwrap();

        assert!(
            gallery
                .nodes
                .iter()
                .any(|node| matches!(node, HostNodeDto::Region { region: "bar", .. }))
        );
        assert!(gallery.nodes.iter().any(|node| matches!(
            node,
            HostNodeDto::Image { resource, .. } if resource == "images/loom.svg"
        )));
        assert!(gallery.nodes.iter().any(|node| matches!(
            node,
            HostNodeDto::Component { capability, .. } if capability == "future-card"
        )));

        let hall = host
            .activate(
                gallery
                    .nodes
                    .iter()
                    .find_map(|node| match node {
                        HostNodeDto::Navigation { id, target, .. } if target == "Hall" => Some(id),
                        _ => None,
                    })
                    .expect("Gallery 应能返回大厅"),
            )
            .unwrap();
        let forms = host
            .activate(
                hall.nodes
                    .iter()
                    .find_map(|node| match node {
                        HostNodeDto::Navigation { id, target, .. } if target == "FormGallery" => {
                            Some(id)
                        }
                        _ => None,
                    })
                    .expect("大厅应提供表单验收入口"),
            )
            .unwrap();
        let checkbox_id: String = forms
            .nodes
            .iter()
            .find_map(|node| match node {
                HostNodeDto::Checkbox { id, selected, .. } if !selected => Some(id.clone()),
                _ => None,
            })
            .expect("表单页应产生未选中的 checkbox");
        host.input(checkbox_id, serde_json::Value::Bool(true))
            .expect("checkbox 值应写回 Worker State");
        let radio_controls: Vec<(String, String)> = forms
            .nodes
            .iter()
            .filter_map(|node| match node {
                HostNodeDto::Radiobutton { id, group, .. } => Some((id.clone(), group.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            radio_controls.len(),
            2,
            "每个 radiobutton 都应有独立交互 ID"
        );
        assert_ne!(radio_controls[0].0, radio_controls[1].0);
        assert_eq!(
            radio_controls[0].1, radio_controls[1].1,
            "绑定同一 receiver 的 radiobutton 应共享互斥组"
        );
        host.input(
            radio_controls[1].0.clone(),
            serde_json::Value::String(String::from("explore")),
        )
        .expect("radiobutton 值应写回 Worker State");
        assert!(host.developer_state().unwrap().iter().any(|entry| {
            entry.namespace == "variables"
                && entry.name == "tips_enabled"
                && entry.value == serde_json::Value::Bool(true)
        }));
        assert!(host.developer_state().unwrap().iter().any(|entry| {
            entry.namespace == "variables"
                && entry.name == "route"
                && entry.value == serde_json::Value::String(String::from("explore"))
        }));
        let button_id: String = forms
            .nodes
            .iter()
            .find_map(|node| match node {
                HostNodeDto::Button { id, target, .. } if target == "Hall" => Some(id.clone()),
                _ => None,
            })
            .expect("表单页应产生语义 button");
        let hall = host
            .activate(button_id.as_str())
            .expect("button 应执行正文后导航");
        assert_eq!(hall.current, "Hall");
        assert!(host.developer_state().unwrap().iter().any(|entry| {
            entry.namespace == "variables"
                && entry.name == "form_confirmed"
                && entry.value == serde_json::Value::Bool(true)
        }));
        let replace_id: String = hall
            .nodes
            .iter()
            .find_map(|node| match node {
                HostNodeDto::Navigation { id, target, .. } if target == "ReplaceGallery" => {
                    Some(id.clone())
                }
                _ => None,
            })
            .expect("大厅应提供 replace 验收入口");
        let replace_gallery = host
            .activate(replace_id.as_str())
            .expect("replace 页面应执行");
        assert!(replace_gallery.nodes.iter().any(|node| matches!(
            node,
            HostNodeDto::Container { key, .. } if key == "status-panel"
        )));
        assert!(replace_gallery.nodes.iter().any(|node| matches!(
            node,
            HostNodeDto::Replace { target, nodes, .. }
                if matches!(target, super::HostReplaceTargetDto::Key(key) if key == "status-panel")
                    && nodes.iter().any(|node| matches!(node, HostNodeDto::Text { text, .. } if text.contains("已被替换")))
        )));
        assert!(replace_gallery.nodes.iter().any(|node| matches!(
            node,
            HostNodeDto::Replace { target, nodes, .. }
                if matches!(target, super::HostReplaceTargetDto::Region("header"))
                    && nodes.iter().any(|node| matches!(node, HostNodeDto::Text { text, .. } if text.contains("替换后的页眉")))
        )));
        fs::remove_dir_all(root_path).unwrap();
    }

    #[test]
    fn packaged_game_starts_without_development_sources() {
        let root = format!("target/test-projects/packaged-host-{}", std::process::id());
        let root_path = Path::new(&root);
        if root_path.exists() {
            fs::remove_dir_all(root_path).unwrap();
        }
        fs::create_dir_all(root_path.join("save")).unwrap();
        let project = format!("target/test-projects/package-source-{}", std::process::id());
        let project_path = Path::new(&project);
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        fs::create_dir_all(project_path.join("contents")).unwrap();
        fs::create_dir_all(project_path.join("resources/data")).unwrap();
        fs::copy(
            repository.join("examples/config.toml"),
            project_path.join("config.toml"),
        )
        .unwrap();
        fs::copy(
            repository.join("examples/contents/story/main.twee"),
            project_path.join("contents/main.twee"),
        )
        .unwrap();
        fs::copy(
            repository.join("examples/contents/scripts/main.ts"),
            project_path.join("contents/main.ts"),
        )
        .unwrap();
        fs::copy(
            repository.join("examples/resources/data/guide.txt"),
            project_path.join("resources/data/guide.txt"),
        )
        .unwrap();
        let config_text = fs::read_to_string(project_path.join("config.toml")).unwrap();
        let config = ProjectConfig::load(&project).unwrap();
        let sources = SourceList::discover(&project).unwrap();
        let resources = ResourceCatalog::discover(&project).unwrap();
        let package =
            NarPackage::build_release(&config, &sources, &resources, &config_text).unwrap();
        fs::write(
            root_path.join("game.nar"),
            package_zip::encode(package.into_files()).unwrap(),
        )
        .unwrap();

        let host = TauriHost::spawn(&root).unwrap();
        let update: HostUpdateDto = host.start().unwrap();
        assert_eq!(update.current, "Start");
        assert!(
            update.nodes.iter().any(|node| matches!(
                node,
                HostNodeDto::Region { region: "bar", nodes, .. } if !nodes.is_empty()
            )),
            "启动更新必须包含游戏作者定义的 Bar 内容"
        );
        assert!(!update.nodes.iter().any(|node| matches!(
            node,
            HostNodeDto::Component { capability, .. } if capability == "narrava.host-tools"
        )));
        drop(host);
        fs::remove_dir_all(root_path).unwrap();
        fs::remove_dir_all(project_path).unwrap();
    }

    #[test]
    fn save_slot_name_cannot_escape_save_directory() {
        assert_eq!(save_file_name("quick-1").unwrap(), "quick-1.nsave");
        assert!(save_file_name("../outside").is_err());
        assert!(save_file_name("").is_err());
    }

    #[test]
    fn active_dialog_tab_focus_does_not_draw_a_bottom_separator() {
        let css: &str = include_str!("../frontend/main.css");
        let selector: &str = "nv-story #dialog-tabs .dialog-tab.active:focus-visible {";
        let rule: &str = css
            .split_once(selector)
            .expect("活动弹窗页签应有独立焦点样式")
            .1
            .split_once('}')
            .expect("活动弹窗页签焦点样式应闭合")
            .0;

        assert!(rule.contains("inset 1px 0"), "焦点提示应绘制左边");
        assert!(rule.contains("inset -1px 0"), "焦点提示应绘制右边");
        assert!(rule.contains("inset 0 1px"), "焦点提示应绘制上边");
        assert!(
            !rule.contains("inset 0 0 0"),
            "四边内描边会在活动页签底部重新画出分隔线"
        );
    }

    #[test]
    fn dialog_centers_in_the_space_outside_the_sidebar() {
        let css: &str = include_str!("../frontend/main.css");
        let javascript: &str = include_str!("../frontend/main.js");

        assert!(css.contains("--narrava-sidebar-offset: 17.5em;"));
        assert!(css.contains("calc(100% - var(--narrava-sidebar-offset) - 4em)"));
        assert!(css.contains("translateX(calc(var(--narrava-sidebar-offset) / 2))"));
        assert!(css.contains("nv-story.bar-stowed"));
        assert!(javascript.contains("story.classList.toggle(\"bar-stowed\", stowed)"));
    }

    #[test]
    fn dialog_content_has_no_host_owned_save_language_or_log_panels() {
        let html: &str = include_str!("../frontend/index.html");
        let javascript: &str = include_str!("../frontend/main.js");

        assert!(!html.contains("id=\"host-dialog-presentation\""));
        assert!(!javascript.contains("openHostPanel"));
        assert!(!javascript.contains("buildSavePanel"));
        assert!(!javascript.contains("buildLanguagePanel"));
        assert!(!javascript.contains("buildLogsPanel"));
    }

    #[test]
    fn bar_and_dialog_content_are_entirely_author_owned() {
        let html: &str = include_str!("../frontend/index.html");
        let javascript: &str = include_str!("../frontend/main.js");
        let story: &str = include_str!("../../../examples/contents/story/main.twee");
        let script: &str = include_str!("../../../examples/contents/scripts/main.ts");

        assert!(!html.contains("data-host-panel="));
        assert!(!javascript.contains("narrava.host-tools"));
        assert!(story.contains(":: Bar\n<<barDemo>>"));
        assert!(!script.contains("narrava.host-tools"));
    }

    #[test]
    fn sidebar_identity_and_both_content_states_belong_to_the_author() {
        let html: &str = include_str!("../frontend/index.html");
        let css: &str = include_str!("../frontend/main.css");
        let javascript: &str = include_str!("../frontend/main.js");
        let story: &str = include_str!("../../../examples/contents/story/main.twee");

        assert!(!html.contains("id=\"story-title\""));
        assert!(!javascript.contains("#story-title"));
        assert!(!css.contains("#story-title"));
        assert!(!css.contains("nv-ui-bar.stowed #ui-bar-body {\n  visibility: hidden"));
        assert!(css.contains("nv-ui-bar.stowed #ui-bar-body"));
        assert!(css.contains("width: var(--narrava-control-size)"));
        assert!(javascript.contains("regions.get(\"bar-stowed\")"));
        assert!(story.contains(":: Bar\n<<barDemo>>"));
        assert!(story.contains(":: BarStowed\n<<barStowedDemo>>"));
    }

    #[test]
    fn runtime_status_is_in_the_passage_footer_instead_of_the_sidebar() {
        let html: &str = include_str!("../frontend/index.html");
        let sidebar: &str = html
            .split_once("<div id=\"ui-bar-body\">")
            .expect("侧栏正文应存在")
            .1
            .split_once("</div>\n      </nv-ui-bar>")
            .expect("侧栏正文应闭合")
            .0;
        let footer: &str = html
            .split_once("<footer class=\"passage-footer\"")
            .expect("Passage Footer 应存在")
            .1
            .split_once("</footer>")
            .expect("Passage Footer 应闭合")
            .0;

        assert!(!sidebar.contains("id=\"status\""));
        assert!(footer.contains("id=\"status\""));
        assert!(footer.contains("正在连接 Runtime…"));
    }

    #[test]
    fn keyboard_focus_is_thin_and_neutral_instead_of_a_thick_blue_frame() {
        let css: &str = include_str!("../frontend/main.css");

        assert!(css.contains("--narrava-focus: rgb(221 221 221 / 60%);"));
        assert!(css.contains("box-shadow: inset 0 0 0 1px var(--narrava-focus);"));
        assert!(!css.contains("0 0 0 0.25rem var(--narrava-focus)"));
    }
}

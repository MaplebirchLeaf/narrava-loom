//! Worker 持有编译产物与 RuntimeSession，串行处理命令并回传 Protocol 更新。
//! timer 和文件 IO 由外层 Host 完成，Worker 不等待挂起操作。

use std::{
    path::Path,
    sync::{
        Arc,
        mpsc::{Receiver, Sender},
    },
};

use narrava_loom_core::{
    ProjectConfig, SourceList, bytecode::BytecodeProgram, hir::HirStory, lir::LirProgram,
    mir::MirStory, nar::ValidatedNarPackage, resource::ResourceCatalog, state::State, twee,
};

use narrava_loom_protocol::{HostDebugSnapshotDto, HostLogLevelDto, RuntimeCommand, RuntimeUpdate};
use narrava_loom_script::RuntimeData;

use crate::{HostErrorDto, HostLogDto, package::load_language_packages};

pub(crate) type WorkerResult = Result<RuntimeUpdate, HostErrorDto>;
pub(crate) type WorkerReply = Sender<WorkerResult>;
pub(crate) type WorkerResponse = Receiver<WorkerResult>;

pub(crate) enum WorkerRequest {
    /// 执行一个同步 Runtime step；Pending 原样返回 Host facade。
    Execute {
        command: RuntimeCommand,
        reply: WorkerReply,
    },
    Complete {
        path: String,
        reply: Sender<Result<Vec<narrava_loom_protocol::HostDebugValueDto>, HostErrorDto>>,
    },
    /// 拉取当前日志快照。
    Logs(Sender<Vec<HostLogDto>>),
    /// 读取已提交状态；Runtime 拒绝 Pending 中间态。
    Inspect(Sender<Result<HostDebugSnapshotDto, HostErrorDto>>),
    /// 拉取可用语言列表。
    Languages(Sender<Vec<String>>),
}

pub(crate) fn run_worker(
    game_path: String,
    requests: Receiver<WorkerRequest>,
    resources: Arc<ResourceCatalog>,
    release: Option<ValidatedNarPackage>,
) {
    let development_sources;
    let config = if release.is_some() {
        let Some(text) = release.as_ref().and_then(ValidatedNarPackage::config_toml) else {
            return fail_worker(
                requests,
                "tauri_host.config",
                String::from("game.nar 缺少 config.toml"),
            );
        };
        match ProjectConfig::parse(Path::new("game.nar/config.toml"), text) {
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
            return fail_worker_error(
                requests,
                narrava_loom_script::protocol_adapter::diagnostic(diagnostic),
            );
        }
    };
    let hir: HirStory<'_> = match HirStory::lower(&ast) {
        Ok(hir) => hir,
        Err(error) => {
            return fail_worker_error(
                requests,
                narrava_loom_script::protocol_adapter::diagnostic(error.diagnostic),
            );
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
    // 开发目录执行刚编译的程序；发行目录必须执行 `game.nar` 内已经完成哈希与格式
    // 校验的拥有型 Bytecode，不能把它校验后丢弃并悄悄改为执行重编译结果。
    let bytecode: BytecodeProgram = if let Some(package) = &release {
        package.bytecode().clone()
    } else {
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
        BytecodeProgram::compile(&lir)
    };
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
    let engine = config.engine.seed.map_or_else(
        narrava_loom_core::engine::Engine::from_entropy,
        narrava_loom_core::engine::Engine::new,
    );
    let mut state: State = State::with_engine(std::rc::Rc::new(engine));
    let script = match narrava_loom_script::EcmaBinding::load(
        sources,
        &resources,
        mir.i18n(),
        &config.game.default_locale,
        &mut state,
    ) {
        Ok(script) => script,
        Err(error) => return fail_worker_error(requests, error.into_host_error()),
    };
    let identity = match config.identity() {
        Ok(identity) => identity,
        Err(error) => return fail_worker(requests, "tauri_host.game_identity", error.to_string()),
    };
    let services = RuntimeData::new(
        identity,
        mir.i18n().clone(),
        config.game.default_locale.clone(),
        language_packages,
    );
    let session =
        narrava_loom_script::RuntimeSession::with_data(&hir, &bytecode, script, state, services);
    let mut runtime = session;
    let mut audio = crate::audio::AudioOutput::new(resources.as_ref().clone());

    loop {
        let request = match requests.recv() {
            Ok(request) => request,
            Err(_) => break,
        };
        match request {
            WorkerRequest::Execute { command, reply } => {
                let result: WorkerResult = runtime.execute(command).map(|update| {
                    let (update, errors) = audio.consume(update);
                    for error in errors {
                        runtime.record_host_error(&error);
                    }
                    update
                });
                let _notices = runtime.take_notices();
                let _sent = reply.send(result);
            }
            WorkerRequest::Complete { path, reply } => {
                let _sent = reply.send(runtime.console_completions(&path));
            }
            WorkerRequest::Logs(reply) => {
                let _sent = reply.send(runtime.log_records());
            }
            WorkerRequest::Inspect(reply) => {
                let _sent = reply.send(runtime.debug_snapshot());
            }
            WorkerRequest::Languages(reply) => {
                let _sent = reply.send(available_languages.clone());
            }
        }
    }
}

pub(crate) fn fail_worker(requests: Receiver<WorkerRequest>, code: &str, message: String) {
    fail_worker_error(requests, HostErrorDto::new(code, message));
}

fn fail_worker_error(requests: Receiver<WorkerRequest>, error: HostErrorDto) {
    for request in requests {
        let reply: WorkerReply = match request {
            WorkerRequest::Execute { reply, .. } => reply,
            WorkerRequest::Complete { reply, .. } => {
                let _sent = reply.send(Err(error.clone()));
                continue;
            }
            WorkerRequest::Logs(reply) => {
                let _sent = reply.send(vec![HostLogDto {
                    sequence: 1,
                    level: HostLogLevelDto::Error,
                    target: String::from("runtime"),
                    message: error.message.clone(),
                    diagnostic: Some(error.clone()),
                }]);
                continue;
            }
            WorkerRequest::Inspect(reply) => {
                let _sent = reply.send(Err(error.clone()));
                continue;
            }
            WorkerRequest::Languages(reply) => {
                let _sent = reply.send(Vec::new());
                continue;
            }
        };
        let _sent: Result<(), _> = reply.send(Err(error.clone()));
    }
}

/// 构造统一的“Worker 已停止”错误（channel 发送失败时使用）。
pub(crate) fn worker_stopped() -> HostErrorDto {
    HostErrorDto::new("tauri_host.worker_stopped", "Narrava Runtime Worker 已停止")
}

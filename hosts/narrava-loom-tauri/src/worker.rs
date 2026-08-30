//! Runtime Worker 主循环与请求处理。
//!
//! 本模块拥有常驻 Worker 线程的请求协议与事务循环：装载游戏包、编译 Story、
//! 驱动 Engine 事务、处理宏分发与 save/语言/日志请求，并把结果转成 DTO 回传。
//! 宏分发回调复用 `narrava-loom-script::dispatch`。

use std::{
    path::Path,
    sync::{
        Arc,
        mpsc::{Receiver, Sender},
    },
    time::{Duration, Instant},
};

use narrava_loom_core::{
    ProjectConfig, SourceList, bytecode::BytecodeProgram, hir::HirStory, lir::LirProgram,
    mir::MirStory, nar::ValidatedNarPackage, resource::ResourceCatalog, state::State, twee,
};

use narrava_loom_protocol::{
    PendingOperation, RuntimeCommand, RuntimeSessionId, RuntimeUpdate, SaveOperation,
};
use narrava_loom_script::{RuntimeServices, RuntimeSessionDriver};

use crate::{
    HostErrorDto, HostLogDto, HostUpdateDto, package::load_language_packages,
    save_io::process_save_io,
};

pub(crate) type WorkerResult = Result<HostUpdateDto, HostErrorDto>;
pub(crate) type WorkerReply = Sender<WorkerResult>;
pub(crate) type WorkerResponse = Receiver<WorkerResult>;
pub(crate) type InputResult = Result<(), HostErrorDto>;
pub(crate) type CommandResult = Result<(), HostErrorDto>;

pub(crate) enum WorkerRequest {
    /// 启动游戏并渲染起始 Passage。
    Start(WorkerReply),
    History {
        backward: bool,
        reply: WorkerReply,
    },
    /// 按交互身份推进一次导航。
    Activate {
        interaction: String,
        reply: WorkerReply,
    },
    /// 把输入控件值写回 State 并落盘。
    Input {
        interaction: String,
        value: serde_json::Value,
        reply: Sender<InputResult>,
    },
    /// 执行一次存档操作（export/import）。
    Save {
        operation: String,
        target: String,
        reply: Sender<CommandResult>,
    },
    /// 拉取当前日志快照。
    Logs(Sender<Vec<HostLogDto>>),
    /// 拉取可用语言列表。
    Languages(Sender<Vec<String>>),
    /// 切换运行时语言（下一次渲染生效）。
    SelectLanguage {
        locale: String,
        reply: Sender<CommandResult>,
    },
}

struct DelayedReply {
    deadline: Instant,
    operation: u64,
    reply: WorkerReply,
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
    let mut state: State = State::new();
    let script = match narrava_loom_script::EcmaBinding::load(
        sources,
        &resources,
        mir.i18n(),
        &config.game.default_locale,
        &mut state,
    ) {
        Ok(script) => script,
        Err(error) => return fail_worker(requests, error.code.as_str(), error.message),
    };
    let identity = match config.identity() {
        Ok(identity) => identity,
        Err(error) => return fail_worker(requests, "tauri_host.game_identity", error.to_string()),
    };
    let services = RuntimeServices::new(
        identity,
        mir.i18n().clone(),
        config.game.default_locale.clone(),
        language_packages,
    );
    let session = narrava_loom_script::RuntimeSession::with_services(
        &hir, &bytecode, script, state, services,
    );
    let session_id =
        RuntimeSessionId::new("main").expect("内建主 Session ID 必须满足 Protocol 校验");
    let mut runtime: RuntimeSessionDriver<'_> = RuntimeSessionDriver::new(session_id, session);
    let mut logs: Vec<HostLogDto> = vec![HostLogDto {
        level: String::from("info"),
        message: String::from("Runtime Worker 已就绪"),
    }];

    let mut delayed: Option<DelayedReply> = None;
    loop {
        let request = match delayed.as_ref() {
            Some(waiting) => {
                let remaining: Duration =
                    waiting.deadline.saturating_duration_since(Instant::now());
                match requests.recv_timeout(remaining) {
                    Ok(request) => request,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        let waiting = delayed.take().expect("超时必须对应活动 Delay");
                        let result = drive_command(
                            &mut runtime,
                            Path::new(&game_path),
                            RuntimeCommand::Resume {
                                operation: waiting.operation,
                                result: None,
                            },
                        );
                        finish_ready_command(result, waiting.reply, &mut delayed);
                        continue;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            None => match requests.recv() {
                Ok(request) => request,
                Err(_) => break,
            },
        };
        if delayed.is_some() {
            match request {
                WorkerRequest::Logs(reply) => {
                    let _sent = reply.send(logs.clone());
                }
                WorkerRequest::Languages(reply) => {
                    let _sent = reply.send(available_languages.clone());
                }
                request => reject_while_delayed(request),
            }
            continue;
        }
        match request {
            WorkerRequest::Start(reply) => {
                let result =
                    drive_command(&mut runtime, Path::new(&game_path), RuntimeCommand::Start);
                append_runtime_notices(&mut runtime, &mut logs);
                finish_ready_command(result, reply, &mut delayed);
            }
            WorkerRequest::History { backward, reply } => {
                let command = if backward {
                    RuntimeCommand::Back
                } else {
                    RuntimeCommand::Forward
                };
                let result = drive_command(&mut runtime, Path::new(&game_path), command);
                append_runtime_notices(&mut runtime, &mut logs);
                finish_ready_command(result, reply, &mut delayed);
            }
            WorkerRequest::Activate { interaction, reply } => {
                let result = drive_command(
                    &mut runtime,
                    Path::new(&game_path),
                    RuntimeCommand::Activate { interaction },
                );
                append_runtime_notices(&mut runtime, &mut logs);
                finish_ready_command(result, reply, &mut delayed);
            }
            WorkerRequest::Input {
                interaction,
                value,
                reply,
            } => {
                let result: InputResult = drive_command(
                    &mut runtime,
                    Path::new(&game_path),
                    RuntimeCommand::Input { interaction, value },
                )
                .map(|_| ());
                append_runtime_notices(&mut runtime, &mut logs);
                let _sent = reply.send(result);
            }
            WorkerRequest::Save {
                operation,
                target,
                reply,
            } => {
                let operation_kind = match operation.as_str() {
                    "export" => Ok(SaveOperation::Export),
                    "import" => Ok(SaveOperation::Import),
                    _ => Err(HostErrorDto::new(
                        "tauri_host.save_operation",
                        format!("未知 Save 操作：{operation}"),
                    )),
                };
                let result = operation_kind.and_then(|operation| {
                    drive_command(
                        &mut runtime,
                        Path::new(&game_path),
                        RuntimeCommand::Save {
                            operation,
                            target: target.clone(),
                        },
                    )
                    .map(|_| ())
                });
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
                let result: CommandResult = drive_command(
                    &mut runtime,
                    Path::new(&game_path),
                    RuntimeCommand::SelectLanguage {
                        locale: locale.clone(),
                    },
                )
                .map(|_| ());
                if result.is_ok() {
                    logs.push(HostLogDto {
                        level: String::from("info"),
                        message: format!("语言已切换为 {locale}；下一次渲染生效"),
                    });
                }
                let _sent = reply.send(result);
            }
        }
    }
}

fn append_runtime_notices(runtime: &mut RuntimeSessionDriver<'_>, logs: &mut Vec<HostLogDto>) {
    logs.extend(runtime.take_notices().into_iter().map(|notice| HostLogDto {
        level: String::from("error"),
        message: format!("{}：{}", notice.code, notice.message),
    }));
    if logs.len() > 200 {
        logs.drain(..logs.len() - 200);
    }
}

fn drive_command(
    runtime: &mut RuntimeSessionDriver<'_>,
    game_path: &Path,
    mut command: RuntimeCommand,
) -> Result<RuntimeUpdate, HostErrorDto> {
    loop {
        match runtime.execute(command)? {
            update @ RuntimeUpdate::Pending {
                operation: PendingOperation::Delay { .. },
            } => return Ok(update),
            RuntimeUpdate::Pending {
                operation:
                    PendingOperation::Save {
                        operation,
                        direction,
                        target,
                        document,
                    },
            } => {
                let result = match process_save_io(game_path, direction, &target, document) {
                    Ok(document) => narrava_loom_protocol::PendingResult::Save { document },
                    Err(error) => narrava_loom_protocol::PendingResult::Failed { error },
                };
                command = RuntimeCommand::Resume {
                    operation,
                    result: Some(result),
                };
            }
            RuntimeUpdate::Pending {
                operation: PendingOperation::SelectLanguage { operation, .. },
            } => {
                command = RuntimeCommand::Resume {
                    operation,
                    result: Some(narrava_loom_protocol::PendingResult::SelectLanguage),
                };
            }
            update => return Ok(update),
        }
    }
}

fn finish_ready_command(
    result: Result<RuntimeUpdate, HostErrorDto>,
    reply: WorkerReply,
    delayed: &mut Option<DelayedReply>,
) {
    match result {
        Ok(RuntimeUpdate::Pending {
            operation:
                PendingOperation::Delay {
                    operation,
                    milliseconds,
                },
        }) => {
            *delayed = Some(DelayedReply {
                deadline: Instant::now() + Duration::from_millis(milliseconds),
                operation,
                reply,
            });
        }
        result => {
            let _sent = reply.send(ready_update(result));
        }
    }
}

fn reject_while_delayed(request: WorkerRequest) {
    let error = HostErrorDto::new(
        "runtime_session.busy",
        "Runtime 正在等待 PendingOperation 完成",
    );
    match request {
        WorkerRequest::Start(reply)
        | WorkerRequest::History { reply, .. }
        | WorkerRequest::Activate { reply, .. } => {
            let _sent = reply.send(Err(error));
        }
        WorkerRequest::Input { reply, .. }
        | WorkerRequest::Save { reply, .. }
        | WorkerRequest::SelectLanguage { reply, .. } => {
            let _sent = reply.send(Err(error));
        }
        WorkerRequest::Logs(_) | WorkerRequest::Languages(_) => unreachable!(),
    }
}

fn ready_update(result: Result<RuntimeUpdate, HostErrorDto>) -> WorkerResult {
    match result? {
        RuntimeUpdate::Ready { update } => Ok(update),
        RuntimeUpdate::Applied => Err(HostErrorDto::new(
            "tauri_host.update_expected",
            "Runtime 命令没有产生可展示更新",
        )),
        RuntimeUpdate::Pending { .. } => Err(HostErrorDto::new(
            "tauri_host.pending_update",
            "Runtime 返回了未处理的 PendingOperation",
        )),
    }
}

pub(crate) fn fail_worker(requests: Receiver<WorkerRequest>, code: &str, message: String) {
    for request in requests {
        let reply: WorkerReply = match request {
            WorkerRequest::Start(reply)
            | WorkerRequest::History { reply, .. }
            | WorkerRequest::Activate { reply, .. } => reply,
            WorkerRequest::Input { reply, .. } => {
                let _sent = reply.send(Err(HostErrorDto::new(code, message.clone())));
                continue;
            }
            WorkerRequest::Save { reply, .. } | WorkerRequest::SelectLanguage { reply, .. } => {
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

/// 构造统一的“Worker 已停止”错误（channel 发送失败时使用）。
pub(crate) fn worker_stopped() -> HostErrorDto {
    HostErrorDto::new("tauri_host.worker_stopped", "Narrava Runtime Worker 已停止")
}

//! Runtime Worker 主循环与请求处理。
//!
//! 本模块拥有常驻 Worker 线程的请求协议与事务循环：装载游戏包、编译 Story、
//! 驱动 Engine 事务、处理宏分发与 save/语言/日志请求，并把结果转成 DTO 回传。
//! 宏分发回调见 [dispatch](crate::dispatch)。

use std::{
    path::Path,
    sync::{
        Arc,
        mpsc::{Receiver, Sender},
    },
    thread,
};

use narrava_loom_core::{
    ProjectConfig, SourceList,
    bytecode::BytecodeProgram,
    diagnostic::{Diagnostic, DiagnosticSeverity},
    engine::{EngineExecutionLimits, EngineMirContinuation},
    expression::{evaluator::assign_value_with_mut, parse as parse_expression, value::Value},
    hir::HirStory,
    host::{
        HostApi, HostDriveResult, HostInput, HostMirAdvanceRequest, HostMirRequest,
        HostPendingExecutions, HostResumeOutcome, HostUpdate,
    },
    i18n::I18nRuntimeLanguage,
    lir::LirProgram,
    macro_runtime::{MacroHandlerOutcome, MacroInteractions, MacroLogicContext},
    mir::MirStory,
    resource::ResourceCatalog,
    runtime::{BodyExecution, RuntimeExecutionIdentity, execute_logic_body},
    semantic::{InteractionId, RegionId, SemanticOutput, SemanticValue},
    state::{State, StateCheckpoint},
    story::{
        Story,
        special::{BAR_PASSAGE, BAR_STOWED_PASSAGE},
    },
    twee,
};

use narrava_loom_protocol::convert;

use narrava_loom_script::dispatch::{dispatch_macro, macro_value_execution};

use crate::{
    HostErrorDto, HostLogDto, HostUpdateDto,
    package::{load_language_packages, load_release_config_text, load_release_package},
    save_io::{process_save, process_save_operation},
};

pub(crate) type WorkerResult = Result<HostUpdateDto, HostErrorDto>;
pub(crate) type WorkerReply = Sender<WorkerResult>;
pub(crate) type WorkerResponse = Receiver<WorkerResult>;
pub(crate) type InputResult = Result<(), HostErrorDto>;
pub(crate) type CommandResult = Result<(), HostErrorDto>;

pub(crate) enum WorkerRequest {
    /// 启动游戏并渲染起始 Passage。
    Start(WorkerReply),
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

pub(crate) fn run_worker(
    game_path: String,
    requests: Receiver<WorkerRequest>,
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
    state.attach_script_dispatcher(script.clone());
    let mut story: Story<'_, '_> = Story::new(&hir);
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();
    let mut pending: HostPendingExecutions<
        EngineMirContinuation<'_, '_, narrava_loom_script::ScriptPending>,
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
                let mut scheduled: Option<narrava_loom_script::ScriptPending> = None;
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
                // save 失败不撤销已渲染的 Surface：把错误记入日志，保持
                // presented 与 WebView 一致，避免后续点击报 host.unknown_interaction。
                if result.is_ok()
                    && let Err(error) = process_save(
                        Path::new(&game_path),
                        &config,
                        &script,
                        &mut state,
                        &mut story,
                    )
                {
                    logs.push(HostLogDto {
                        level: String::from("error"),
                        message: format!("{}：{}", error.code, error.message),
                    });
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
                let mut scheduled: Option<narrava_loom_script::ScriptPending> = None;
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
                                output: SemanticOutput::default(),
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
                // save 失败不撤销已渲染的 Surface：把错误记入日志，保持
                // presented 与 WebView 一致，避免后续点击报 host.unknown_interaction。
                if result.is_ok()
                    && let Err(error) = process_save(
                        Path::new(&game_path),
                        &config,
                        &script,
                        &mut state,
                        &mut story,
                    )
                {
                    logs.push(HostLogDto {
                        level: String::from("error"),
                        message: format!("{}：{}", error.code, error.message),
                    });
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
                    let binding = previous.surface().input_binding(&id).ok_or_else(|| {
                        HostErrorDto::new(
                            "tauri_host.unknown_input",
                            "输入身份未出现在上一份 Surface 中",
                        )
                    })?;
                    let semantic: SemanticValue = json_to_surface_value(&value)?;
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
                    let value: Value = narrava_loom_script::json_to_value(&value)?;
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
                let selected: Result<Option<I18nRuntimeLanguage>, HostErrorDto> =
                    I18nRuntimeLanguage::select(
                        mir.i18n(),
                        &config.game.default_locale,
                        &locale,
                        language_packages.clone(),
                    )
                    .map_err(|error| {
                        HostErrorDto::new("tauri_host.language_select", error.to_string())
                    });
                let result: CommandResult = selected.and_then(|selected| {
                    script.select_locale(&locale)?;
                    runtime_language = selected;
                    Ok(())
                });
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

/// 将 Engine 的事务生命周期映射为稳定的作者可订阅事件名。
///
/// 事件在对应阶段同步写入 Worker 队列；任何绑定错误都会作为生命周期错误触发
/// Engine 回滚，避免出现“导航已提交但生命周期事实丢失”的半成功状态。
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_passage_event(
    script: &narrava_loom_script::EcmaBinding,
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
pub(crate) fn append_sidebar_regions<'hir, 'source>(
    update: &mut HostUpdate,
    script: &narrava_loom_script::EcmaBinding,
    hir: &'hir HirStory<'source>,
    interactions: &mut MacroInteractions<'hir, 'source>,
    state: &State,
    story: &Story<'hir, 'source>,
    bytecode: &BytecodeProgram,
    language: Option<&I18nRuntimeLanguage>,
    sequence: &mut u64,
) -> Result<(), HostErrorDto> {
    for (name, region) in [
        (BAR_PASSAGE, RegionId::bar()),
        (BAR_STOWED_PASSAGE, RegionId::bar_stowed()),
    ] {
        if !story.has(name) {
            continue;
        }
        let mut view_state: State = state.fork_view();
        let mut view_story: Story<'hir, 'source> = story.fork_view();
        let mut pending: HostPendingExecutions<
            EngineMirContinuation<'hir, 'source, narrava_loom_script::ScriptPending>,
        > = HostPendingExecutions::new();
        let mut scheduled: Option<narrava_loom_script::ScriptPending> = None;
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
        update.append_region(region, rendered.surface().clone());
        *sequence = sequence.saturating_add(1);
    }
    Ok(())
}

/// 驱动 Engine 直到产出 Ready 更新；遇到 Pending 时执行挂起的脚本操作
/// （如 `Host.delay`）后再恢复，循环直到拿到可展示的更新。
///
/// 这些参数分别由 Engine、脚本 Worker 与当前事务持有；合并为长期上下文会扩大
/// 可变借用范围，并让 start/activate 两条路径更难复用。
#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_drive<'hir, 'source>(
    mut result: Result<HostDriveResult, HostErrorDto>,
    script: &narrava_loom_script::EcmaBinding,
    scheduled: &mut Option<narrava_loom_script::ScriptPending>,
    pending: &mut HostPendingExecutions<
        EngineMirContinuation<'hir, 'source, narrava_loom_script::ScriptPending>,
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
                Ok(narrava_loom_script::ScriptMacroOutcome::Complete(value)) => {
                    macro_value_execution(&value)
                        .map(MacroHandlerOutcome::Complete)
                        .map_err(|error| error.to_string())
                }
                Ok(narrava_loom_script::ScriptMacroOutcome::Pending(next)) => {
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

pub(crate) fn fail_worker(requests: Receiver<WorkerRequest>, code: &str, message: String) {
    for request in requests {
        let reply: WorkerReply = match request {
            WorkerRequest::Start(reply) | WorkerRequest::Activate { reply, .. } => reply,
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

pub(crate) fn json_to_surface_value(
    value: &serde_json::Value,
) -> Result<SemanticValue, HostErrorDto> {
    match value {
        serde_json::Value::Null => Ok(SemanticValue::Null),
        serde_json::Value::Bool(value) => Ok(SemanticValue::Boolean(*value)),
        serde_json::Value::Number(value) => value
            .as_f64()
            .filter(|value: &f64| value.is_finite())
            .map(SemanticValue::Number)
            .ok_or_else(|| HostErrorDto::new("tauri_host.input_value", "输入数值超出范围")),
        serde_json::Value::String(value) => Ok(SemanticValue::Text(value.clone())),
        serde_json::Value::Array(values) => values
            .iter()
            .map(json_to_surface_value)
            .collect::<Result<Vec<_>, _>>()
            .map(SemanticValue::List),
        serde_json::Value::Object(values) => values
            .iter()
            .map(|(name, value)| Ok((name.clone(), json_to_surface_value(value)?)))
            .collect::<Result<std::collections::BTreeMap<_, _>, HostErrorDto>>()
            .map(SemanticValue::Map),
    }
}

fn limits() -> EngineExecutionLimits {
    EngineExecutionLimits {
        passages: 8,
        includes: 32,
    }
}

//! TUI Host：编译游戏、驱动 Core Engine 并渲染到终端。
//!
//! 同步单线程驱动：加载开发目录或 `game.nar` 发行包 → 编译 Twee/脚本 →
//! `HostApi` 驱动 Engine 事务 → `TuiRenderer` 渲染 Surface → 终端输入
//! （编号选择导航、输入控件写回）回送 Host。脚本执行与宏分发复用
//! `narrava-loom-script`（Boa + 共享 dispatch）。

use std::{io, path::Path};

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
    lir::LirProgram,
    macro_runtime::{MacroHandlerOutcome, MacroInteractions, MacroLogicContext},
    mir::MirStory,
    nar::{NAR_MAGIC, NarPackage},
    package_zip,
    resource::ResourceCatalog,
    runtime::execute_logic_body,
    runtime::{BodyExecution, RuntimeExecutionIdentity},
    semantic::{InteractionId, RegionId, SemanticOutput, SemanticValue},
    state::{State, StateCheckpoint},
    story::{
        Story,
        special::{BAR_PASSAGE, BAR_STOWED_PASSAGE},
    },
    twee,
};
use narrava_loom_protocol::{HostErrorDto, Surface, SurfaceValue};
use narrava_loom_script::{
    EcmaBinding, ScriptPending,
    dispatch::{dispatch_macro, emit_passage_event, macro_value_execution},
};

use crate::{TuiFrame, TuiRenderer, write_frame};

/// 单次 Engine 事务的执行上限；TUI 与 Web Host 使用同一组预算。
fn limits() -> EngineExecutionLimits {
    EngineExecutionLimits {
        passages: 32,
        includes: 256,
    }
}

/// 驱动 Engine 直到产出 Ready 更新；脚本 Pending（如 `Host.delay`）经恢复后继续。
#[allow(clippy::too_many_arguments)]
fn drive_to_update<'hir, 'source>(
    mut result: Result<HostDriveResult, HostErrorDto>,
    script: &EcmaBinding,
    scheduled: &mut Option<ScriptPending>,
    pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, ScriptPending>>,
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
                "tui_host.pending_without_operation",
                "Core 已暂停，但 Host 没有对应的异步操作",
            )
        })?;
        std::thread::sleep(operation.delay());

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
                    |_phase, _context, _state| Ok::<(), Diagnostic>(()),
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

/// 装载游戏并进入渲染/输入主循环；`game_path` 是开发目录或含 `game.nar` 的发行目录。
pub fn run(game_path: &str) -> Result<(), HostErrorDto> {
    let loaded = load_game(game_path)?;
    let (sources, resources) = match &loaded {
        LoadedGame::Release {
            sources, resources, ..
        } => (sources, resources),
        LoadedGame::Development { sources, resources } => (sources, resources),
    };
    let config = match &loaded {
        LoadedGame::Release { config_toml, .. } => {
            ProjectConfig::parse(Path::new("game.nar/config.toml"), config_toml)
                .map_err(|error| HostErrorDto::new("tui_host.config", error.to_string()))?
        }
        LoadedGame::Development { .. } => ProjectConfig::load(game_path)
            .map_err(|error| HostErrorDto::new("tui_host.config", error.to_string()))?,
    };
    let ast: twee::Story<'_> = twee::Story::build(&sources.items)
        .map_err(|error| HostErrorDto::diagnostic(error.diagnostic()))?;
    let hir: HirStory<'_> =
        HirStory::lower(&ast).map_err(|error| HostErrorDto::diagnostic(error.diagnostic))?;
    let mir: MirStory<'_, '_> = MirStory::lower(&hir)
        .map_err(|error| HostErrorDto::new("tui_host.mir", error.kind.to_string()))?;
    let bytecode: BytecodeProgram = match &loaded {
        LoadedGame::Release { bytecode, .. } => bytecode.as_ref().clone(),
        LoadedGame::Development { .. } => {
            let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).map_err(|error| {
                HostErrorDto::new("tui_host.lir", format!("{:?}", error.kind()))
            })?;
            BytecodeProgram::compile(&lir)
        }
    };
    let mut state: State = State::new();
    let script: std::rc::Rc<EcmaBinding> = EcmaBinding::load(
        sources,
        resources,
        mir.i18n(),
        &config.game.default_locale,
        &mut state,
    )
    .map_err(|error| HostErrorDto::new("tui_host.script", error.to_string()))?;
    state.attach_script_dispatcher(script.clone());
    let mut story: Story<'_, '_> = Story::new(&hir);
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();
    let mut pending: HostPendingExecutions<EngineMirContinuation<'_, '_, ScriptPending>> =
        HostPendingExecutions::new();
    let mut scheduled: Option<ScriptPending> = None;
    let mut sequence: u64 = 1;
    let mut renderer: TuiRenderer = TuiRenderer::default();

    // 启动起始 Passage 并渲染第一帧。
    let params: Value = Value::Null;
    let start = HostApi::start_mir(
        &mut pending,
        &mut state,
        &mut story,
        &bytecode,
        HostMirRequest {
            params: &params,
            identity: RuntimeExecutionIdentity::new(1, sequence),
            limits: limits(),
            language: None,
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
    let update: HostUpdate = drive_to_update(
        start,
        &script,
        &mut scheduled,
        &mut pending,
        &hir,
        &mut interactions,
        &mut state,
        &mut story,
        &bytecode,
    )?;
    sequence = sequence.saturating_add(1);
    let mut update: HostUpdate = finish_update(
        update,
        &script,
        &hir,
        &mut interactions,
        &state,
        &story,
        &bytecode,
        &mut sequence,
    )?;

    let stdin = io::stdin();
    loop {
        let surface: Surface = Surface::from(update.surface());
        let frame: TuiFrame = renderer.render(update.current(), &surface);
        write_frame(&mut io::stdout().lock(), &frame)
            .map_err(|error| HostErrorDto::new("tui_host.write", error.to_string()))?;
        write_help_prompt(&mut io::stdout().lock())
            .map_err(|error| HostErrorDto::new("tui_host.write", error.to_string()))?;

        let mut line = String::new();
        if stdin.read_line(&mut line).is_err() || line.trim().is_empty() {
            continue;
        }
        let command = crate::TuiCommand::parse(line.trim())
            .map_err(|error| HostErrorDto::new("tui_host.command", error.to_string()))?;
        match command
            .resolve(&frame)
            .map_err(|error| HostErrorDto::new("tui_host.command", error.to_string()))?
        {
            crate::TuiOperation::Help => continue,
            crate::TuiOperation::Redraw => continue,
            crate::TuiOperation::Quit => break,
            crate::TuiOperation::Activate { id } => {
                let next: HostUpdate = activate(
                    &update,
                    &id,
                    &script,
                    &hir,
                    &mut interactions,
                    &mut pending,
                    &mut scheduled,
                    &mut state,
                    &mut story,
                    &bytecode,
                    sequence,
                )?;
                sequence = sequence.saturating_add(1);
                update = finish_update(
                    next,
                    &script,
                    &hir,
                    &mut interactions,
                    &state,
                    &story,
                    &bytecode,
                    &mut sequence,
                )?;
            }
            crate::TuiOperation::Input { id, value } => {
                input(&update, &id, value, &mut state)?;
            }
            crate::TuiOperation::Dismiss => continue,
        }
    }
    Ok(())
}

/// 已装载游戏：发行包（`game.nar`）或开发目录。
enum LoadedGame {
    Release {
        sources: SourceList,
        resources: ResourceCatalog,
        config_toml: String,
        bytecode: Box<BytecodeProgram>,
    },
    Development {
        sources: SourceList,
        resources: ResourceCatalog,
    },
}

const PACKAGE_LIMIT: usize = 64 << 20;

/// 装载游戏：优先 `game.nar` 发行包（校验 NAR 魔数与哈希），否则发现开发目录。
fn load_game(game_path: &str) -> Result<LoadedGame, HostErrorDto> {
    let nar_path = Path::new(game_path).join("game.nar");
    if nar_path.exists() {
        let bytes = std::fs::read(&nar_path)
            .map_err(|error| HostErrorDto::new("tui_host.package_read", error.to_string()))?;
        let zip_bytes = bytes.strip_prefix(NAR_MAGIC).ok_or_else(|| {
            HostErrorDto::new("tui_host.package_magic", "game.nar 缺少 NAR 魔数头")
        })?;
        let files = package_zip::decode(zip_bytes, PACKAGE_LIMIT)
            .map_err(|error| HostErrorDto::new("tui_host.package_zip", error))?;
        let package = NarPackage::from_files(files)
            .map_err(|error| HostErrorDto::new("tui_host.package", error.to_string()))?;
        let config_toml = package
            .config_toml()
            .ok_or_else(|| HostErrorDto::new("tui_host.config", "game.nar 缺少 config.toml"))?
            .to_owned();
        let package = package
            .validate()
            .map_err(|error| HostErrorDto::new("tui_host.package", error.to_string()))?;
        return Ok(LoadedGame::Release {
            sources: package.sources().clone(),
            resources: package.resources().clone(),
            config_toml,
            bytecode: Box::new(package.bytecode().clone()),
        });
    }
    let sources = SourceList::discover(game_path)
        .map_err(|error| HostErrorDto::new("tui_host.source", error.to_string()))?;
    let resources = ResourceCatalog::discover(Path::new(game_path))
        .map_err(|error| HostErrorDto::new("tui_host.resource", error.to_string()))?;
    Ok(LoadedGame::Development { sources, resources })
}

/// 激活导航/按钮交互：经 `HostApi` 推进 Engine 事务并驱动到可渲染更新。
#[allow(clippy::too_many_arguments)]
fn activate<'hir, 'source>(
    previous: &HostUpdate,
    interaction: &str,
    script: &EcmaBinding,
    hir: &'hir HirStory<'source>,
    interactions: &mut MacroInteractions<'hir, 'source>,
    pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, ScriptPending>>,
    scheduled: &mut Option<ScriptPending>,
    state: &mut State,
    story: &mut Story<'hir, 'source>,
    bytecode: &BytecodeProgram,
    sequence: u64,
) -> Result<HostUpdate, HostErrorDto> {
    let id: InteractionId = InteractionId::parse(interaction)
        .map_err(|error| HostErrorDto::new("tui_host.interaction", error.to_string()))?;
    let params: Value = Value::Null;
    let request = HostMirAdvanceRequest {
        presented: previous,
        input: HostInput::activate(id.clone()),
        params: &params,
        identity: RuntimeExecutionIdentity::new(1, sequence),
        limits: limits(),
        language: None,
    };
    let result = if interactions.has(&id) {
        let mut next_interactions: MacroInteractions<'_, '_> = MacroInteractions::new();
        let result = HostApi::advance_macro_interaction_mir(
            pending,
            interactions,
            state,
            story,
            bytecode,
            request,
            |body, state, requests, scopes| {
                let mut context = MacroLogicContext::new(state, requests, scopes);
                let control = execute_logic_body(body, &mut context).map_err(|error| {
                    Diagnostic::new(
                        "tui_host.interaction_body",
                        DiagnosticSeverity::Error,
                        &format!("Interaction 正文执行失败：{error:?}"),
                    )
                })?;
                Ok::<BodyExecution, Diagnostic>(BodyExecution {
                    control,
                    output: SemanticOutput::default(),
                })
            },
            |phase, context, _state| emit_passage_event(script, phase, context),
            |invocation, state, requests, scopes| {
                dispatch_macro(
                    script,
                    hir,
                    &mut next_interactions,
                    scheduled,
                    invocation,
                    state,
                    requests,
                    scopes,
                )
            },
        );
        *interactions = next_interactions;
        result
    } else {
        HostApi::advance_mir(
            pending,
            state,
            story,
            bytecode,
            request,
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
    };
    drive_to_update(
        result.map_err(|error| HostErrorDto::diagnostic(error.diagnostic.clone())),
        script,
        scheduled,
        pending,
        hir,
        interactions,
        state,
        story,
        bytecode,
    )
}

/// 为每一份可展示更新补齐作者拥有的特殊区域。
///
/// 启动与后续交互共用这个收尾点，避免 Bar/BarStowed 停留在首帧状态。
#[allow(clippy::too_many_arguments)]
fn finish_update<'hir, 'source>(
    mut update: HostUpdate,
    script: &EcmaBinding,
    hir: &'hir HirStory<'source>,
    interactions: &mut MacroInteractions<'hir, 'source>,
    state: &State,
    story: &Story<'hir, 'source>,
    bytecode: &BytecodeProgram,
    sequence: &mut u64,
) -> Result<HostUpdate, HostErrorDto> {
    append_sidebar(
        &mut update,
        script,
        hir,
        interactions,
        state,
        story,
        bytecode,
        sequence,
    )?;
    Ok(update)
}

/// 用隔离的 State/Story 视图渲染 `Bar`/`BarStowed` 特殊 Passage，并追加到当前更新。
#[allow(clippy::too_many_arguments)]
fn append_sidebar<'hir, 'source>(
    update: &mut HostUpdate,
    script: &EcmaBinding,
    hir: &'hir HirStory<'source>,
    interactions: &mut MacroInteractions<'hir, 'source>,
    state: &State,
    story: &Story<'hir, 'source>,
    bytecode: &BytecodeProgram,
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
            EngineMirContinuation<'hir, 'source, ScriptPending>,
        > = HostPendingExecutions::new();
        let mut scheduled: Option<ScriptPending> = None;
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
                language: None,
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
        let rendered: HostUpdate = drive_to_update(
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

/// 把输入控件值写回 Worker State（checkbox/radiobutton/textbox）。
fn input(
    previous: &HostUpdate,
    interaction: &str,
    value: SurfaceValue,
    state: &mut State,
) -> Result<(), HostErrorDto> {
    let id: InteractionId = InteractionId::parse(interaction)
        .map_err(|error| HostErrorDto::new("tui_host.input", error.to_string()))?;
    let binding = previous
        .surface()
        .input_binding(&id)
        .ok_or_else(|| HostErrorDto::new("tui_host.input", "输入身份未出现在上一份输出中"))?;
    let semantic: SemanticValue = SemanticValue::from(&value);
    if !binding.accepts(&semantic) {
        return Err(HostErrorDto::new(
            "tui_host.input_value",
            "输入值不属于当前控件允许的值集合",
        ));
    }
    let json: serde_json::Value = json_from_surface(&value);
    let core_value: Value = narrava_loom_script::json_to_value(&json)
        .map_err(|error| HostErrorDto::new(&error.code, error.message))?;
    let expression = parse_expression(binding.receiver.as_str())
        .map_err(|error| HostErrorDto::new("tui_host.input_receiver", format!("{error:?}")))?;
    let checkpoint: StateCheckpoint = state.checkpoint();
    if let Err(error) = assign_value_with_mut(&expression, core_value, state) {
        state.restore_checkpoint(checkpoint);
        return Err(HostErrorDto::new(
            "tui_host.input_assignment",
            format!("{error:?}"),
        ));
    }
    Ok(())
}

/// Surface 值 → JSON（输入控件写回用）。
fn json_from_surface(value: &SurfaceValue) -> serde_json::Value {
    match value {
        SurfaceValue::Null => serde_json::Value::Null,
        SurfaceValue::Boolean(value) => serde_json::Value::Bool(*value),
        SurfaceValue::Number(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::Value::String(value.to_string())),
        SurfaceValue::Text(value) => serde_json::Value::String(value.clone()),
        _ => serde_json::Value::Null,
    }
}

fn write_help_prompt(writer: &mut impl io::Write) -> io::Result<()> {
    writeln!(writer, "输入编号选择动作；h 帮助、r 重绘、q 退出")
}

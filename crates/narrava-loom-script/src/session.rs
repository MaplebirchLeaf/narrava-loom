//! Host-neutral Narrava 生命周期编排。

mod state_io;

pub(crate) use state_io::RuntimePlatform;
pub use state_io::RuntimeServices;
use state_io::UnsupportedRuntimePlatform;

use std::rc::Rc;

use narrava_loom_core::{
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
    macro_runtime::{MacroHandlerOutcome, MacroInteractions, MacroLogicContext},
    runtime::{BodyExecution, RuntimeExecutionIdentity, execute_logic_body},
    script::ScriptCallDispatcher,
    semantic::{InteractionId, RegionId, SemanticOutput, SemanticValue},
    state::{State, StateCheckpoint, StateSnapshot},
    story::{
        Story,
        special::{BAR_PASSAGE, BAR_STOWED_PASSAGE},
    },
};
use narrava_loom_protocol::{
    HostErrorDto, PendingOperation, RuntimeCommand, RuntimeUpdate, SaveOperation,
};

use crate::{
    ScriptAdapter, ScriptMacroOutcome, ScriptPending,
    dispatch::{dispatch_macro, emit_passage_event, macro_value_execution},
    json_to_value,
    protocol_adapter::{diagnostic, encode_host_update},
};

const STORY_ID: u64 = 1;
const SPECIAL_STORY_ID: u64 = 2;

fn limits() -> EngineExecutionLimits {
    EngineExecutionLimits {
        passages: 32,
        includes: 256,
    }
}

enum Waiting<'hir, 'source> {
    Main {
        operation: u64,
        execution: narrava_loom_core::host::HostExecutionToken,
    },
    Special(Box<SpecialWaiting<'hir, 'source>>),
    Platform(Box<PlatformWaiting>),
}

enum PlatformAction {
    Save {
        operation: SaveOperation,
        target: String,
    },
    SelectLanguage {
        locale: String,
    },
}

struct PlatformWaiting {
    operation: u64,
    action: PlatformAction,
    after: RuntimeUpdate,
    script_save: bool,
    input_checkpoint: Option<StateCheckpoint>,
}

struct SpecialWaiting<'hir, 'source> {
    operation: u64,
    execution: narrava_loom_core::host::HostExecutionToken,
    update: HostUpdate,
    next_special: usize,
    region: RegionId,
    state: State,
    story: Story<'hir, 'source>,
    pending: HostPendingExecutions<EngineMirContinuation<'hir, 'source, ScriptPending>>,
}

/// 一局游戏的 Host-neutral Runtime 所有权根。
///
/// Host 只能发送拥有型 [`RuntimeCommand`] 并消费 [`RuntimeUpdate`]；Engine
/// continuation、上一份可交互输出、State、Story 与脚本 interaction 均不越过此边界。
pub struct RuntimeSession<'hir, 'source, Adapter: ScriptAdapter + ScriptCallDispatcher + 'static> {
    hir: &'hir HirStory<'source>,
    bytecode: &'hir BytecodeProgram,
    script: Rc<Adapter>,
    state: State,
    story: Story<'hir, 'source>,
    interactions: MacroInteractions<'hir, 'source>,
    pending: HostPendingExecutions<EngineMirContinuation<'hir, 'source, ScriptPending>>,
    scheduled: Option<ScriptPending>,
    waiting: Option<Waiting<'hir, 'source>>,
    presented: Option<Rc<HostUpdate>>,
    language: Option<Rc<I18nRuntimeLanguage>>,
    sequence: u64,
    platform: Box<dyn RuntimePlatform<'hir, 'source> + 'hir>,
    notices: Vec<HostErrorDto>,
    reaction_before: Option<StateSnapshot>,
    reaction_state_checkpoint: Option<StateCheckpoint>,
    reaction_story_snapshot: Option<narrava_loom_core::story::StorySnapshot<'hir, 'source>>,
    reaction_presented_checkpoint: Option<Rc<HostUpdate>>,
    reaction_interactions_checkpoint: Option<MacroInteractions<'hir, 'source>>,
    reaction_checkpoint: Option<Vec<narrava_loom_core::reaction::ReactionRuntimeState>>,
    reaction_prefix: SemanticOutput,
}

impl<'hir, 'source, Adapter: ScriptAdapter + ScriptCallDispatcher + 'static>
    RuntimeSession<'hir, 'source, Adapter>
{
    /// 建立一局 Runtime；传入的 State 应已被脚本 Binding 初始化。
    pub fn new(
        hir: &'hir HirStory<'source>,
        bytecode: &'hir BytecodeProgram,
        script: Rc<Adapter>,
        state: State,
    ) -> Self {
        Self::with_platform(
            hir,
            bytecode,
            script,
            state,
            Box::new(UnsupportedRuntimePlatform),
        )
    }

    /// 建立一局 Runtime，并注入唯一的平台 IO adapter。
    pub(crate) fn with_platform(
        hir: &'hir HirStory<'source>,
        bytecode: &'hir BytecodeProgram,
        script: Rc<Adapter>,
        mut state: State,
        platform: Box<dyn RuntimePlatform<'hir, 'source> + 'hir>,
    ) -> Self {
        state.attach_script_dispatcher(script.clone());
        Self {
            hir,
            bytecode,
            script,
            state,
            story: Story::new(hir),
            interactions: MacroInteractions::new(),
            pending: HostPendingExecutions::new(),
            scheduled: None,
            waiting: None,
            presented: None,
            language: None,
            sequence: 1,
            platform,
            notices: Vec::new(),
            reaction_before: None,
            reaction_state_checkpoint: None,
            reaction_story_snapshot: None,
            reaction_presented_checkpoint: None,
            reaction_interactions_checkpoint: None,
            reaction_checkpoint: None,
            reaction_prefix: SemanticOutput::default(),
        }
    }

    /// 建立带 Save/I18n 数据服务的 Runtime；平台文件 IO 仍通过 PendingOperation 完成。
    pub fn with_services(
        hir: &'hir HirStory<'source>,
        bytecode: &'hir BytecodeProgram,
        script: Rc<Adapter>,
        state: State,
        services: RuntimeServices,
    ) -> Self {
        Self::with_platform(hir, bytecode, script, state, Box::new(services))
    }

    /// 执行一条平台无关命令；Pending 必须以返回的 operation ID 恢复或取消。
    pub fn execute(&mut self, command: RuntimeCommand) -> Result<RuntimeUpdate, HostErrorDto> {
        let cancels_execution = matches!(&command, RuntimeCommand::Cancel { .. });
        let processes_script_save: bool = matches!(
            &command,
            RuntimeCommand::Start
                | RuntimeCommand::Back
                | RuntimeCommand::Forward
                | RuntimeCommand::Activate { .. }
                | RuntimeCommand::Input { .. }
                | RuntimeCommand::Resume { .. }
        );
        let mut input_checkpoint: Option<StateCheckpoint> =
            matches!(&command, RuntimeCommand::Input { .. }).then(|| self.state.checkpoint());
        if processes_script_save && self.reaction_before.is_none() {
            self.reaction_before = Some(self.state.snapshot());
            self.reaction_state_checkpoint = Some(self.state.checkpoint());
            self.reaction_story_snapshot = Some(self.story.snapshot());
            self.reaction_presented_checkpoint = self.presented.clone();
            self.reaction_interactions_checkpoint = Some(self.interactions.clone());
            self.reaction_checkpoint = Some(self.script.reaction_state());
        }
        let result: Result<RuntimeUpdate, HostErrorDto> = match command {
            RuntimeCommand::Start => self.start(),
            RuntimeCommand::Back => self.history(true),
            RuntimeCommand::Forward => self.history(false),
            RuntimeCommand::Activate { interaction } => self.activate(&interaction),
            RuntimeCommand::Input { interaction, value } => self.input(&interaction, value),
            RuntimeCommand::Save { operation, target } => {
                self.begin_save(operation, target, RuntimeUpdate::Applied, false, None)
            }
            RuntimeCommand::SelectLanguage { locale } => {
                self.begin_language(locale, RuntimeUpdate::Applied)
            }
            RuntimeCommand::Resume { operation, result } => self.resume(operation, result),
            RuntimeCommand::Cancel { operation } => self.cancel(operation),
        };
        let mut update: RuntimeUpdate = match result {
            Ok(update) => update,
            Err(error) => {
                self.rollback_reaction_state();
                return Err(error);
            }
        };
        if cancels_execution {
            self.rollback_reaction_state();
            return Ok(update);
        }
        if processes_script_save && !matches!(update, RuntimeUpdate::Pending { .. }) {
            update = match self.settle_reactions(update) {
                Ok(update) => update,
                Err(error) => {
                    self.rollback_reaction_state();
                    return Err(error);
                }
            };
        }
        if processes_script_save && !matches!(update, RuntimeUpdate::Pending { .. }) {
            match self.process_script_save(update.clone(), &mut input_checkpoint) {
                Ok(Some(pending)) => return Ok(pending),
                Ok(None) => {}
                Err(error) => {
                    if let Some(checkpoint) = input_checkpoint {
                        self.state.restore_checkpoint(checkpoint);
                        return Err(error);
                    }
                    self.notices.push(error);
                }
            }
        }
        if processes_script_save && !matches!(update, RuntimeUpdate::Pending { .. }) {
            match self.process_script_language(update.clone()) {
                Ok(Some(pending)) => return Ok(pending),
                Ok(None) => {}
                Err(error) => self.notices.push(error),
            }
        }
        Ok(update)
    }

    fn rollback_reaction_state(&mut self) {
        if let Some(checkpoint) = self.reaction_checkpoint.take() {
            let _ignored = self.script.restore_reaction_state(&checkpoint);
        }
        self.reaction_before = None;
        if let Some(checkpoint) = self.reaction_state_checkpoint.take() {
            self.state.restore_checkpoint(checkpoint);
        }
        if let Some(snapshot) = self.reaction_story_snapshot.take() {
            let _restored = self.story.restore(snapshot);
        }
        self.presented = self.reaction_presented_checkpoint.take();
        if let Some(interactions) = self.reaction_interactions_checkpoint.take() {
            self.interactions = interactions;
        }
        let _ignored = self.script.sync_variables(&self.state);
        self.reaction_prefix = SemanticOutput::default();
    }

    /// 取走命令成功后产生的非阻塞平台提示，例如导航完成后的自动存档失败。
    pub fn take_notices(&mut self) -> Vec<HostErrorDto> {
        std::mem::take(&mut self.notices)
    }

    fn ensure_idle(&self) -> Result<(), HostErrorDto> {
        if self.waiting.is_some() {
            return Err(HostErrorDto::new(
                "runtime_session.pending",
                "Runtime 正等待 Host 恢复或取消挂起操作",
            ));
        }
        Ok(())
    }

    fn identity(&mut self, story: u64) -> RuntimeExecutionIdentity {
        let identity: RuntimeExecutionIdentity =
            RuntimeExecutionIdentity::new(story, self.sequence);
        self.sequence = self.sequence.saturating_add(1);
        identity
    }

    fn start(&mut self) -> Result<RuntimeUpdate, HostErrorDto> {
        self.ensure_idle()?;
        if self.presented.is_some() {
            return Err(HostErrorDto::new(
                "runtime_session.already_started",
                "当前 RuntimeSession 已经启动",
            ));
        }
        let params: Value = Value::Null;
        let identity: RuntimeExecutionIdentity = self.identity(STORY_ID);
        let language: Option<Rc<I18nRuntimeLanguage>> = self.language.clone();
        let result = HostApi::start_mir_with_reaction(
            &mut self.pending,
            &mut self.state,
            &mut self.story,
            self.bytecode,
            HostMirRequest {
                params: &params,
                identity,
                limits: limits(),
                language: language.as_deref(),
            },
            |_passage, _state, _requests, _limits| {
                Ok::<BodyExecution, Diagnostic>(BodyExecution::default())
            },
            |phase, context, _state| emit_passage_event(self.script.as_ref(), phase, context),
            |passage, state, requests| {
                crate::reaction_runtime::apply_lifecycle_reactions(
                    self.script.as_ref(),
                    self.hir,
                    passage,
                    state,
                    requests,
                )
            },
            |invocation, state, requests, scopes| {
                dispatch_macro(
                    self.script.as_ref(),
                    self.hir,
                    &mut self.interactions,
                    &mut self.scheduled,
                    invocation,
                    state,
                    requests,
                    scopes,
                )
            },
        )
        .map_err(|error| diagnostic(error.diagnostic.clone()));
        self.drive_main(result)
    }

    fn activate(&mut self, interaction: &str) -> Result<RuntimeUpdate, HostErrorDto> {
        self.ensure_idle()?;
        let previous: Rc<HostUpdate> = self.presented.clone().ok_or_else(|| {
            HostErrorDto::new("runtime_session.not_started", "必须先启动 Runtime")
        })?;
        let id: InteractionId = InteractionId::parse(interaction)
            .map_err(|error| HostErrorDto::new("runtime_session.interaction", error.to_string()))?;
        let params: Value = Value::Null;
        let identity: RuntimeExecutionIdentity = self.identity(STORY_ID);
        let language: Option<Rc<I18nRuntimeLanguage>> = self.language.clone();
        let request = HostMirAdvanceRequest {
            presented: previous.as_ref(),
            input: HostInput::activate(id.clone()),
            params: &params,
            identity,
            limits: limits(),
            language: language.as_deref(),
        };
        let result = if self.interactions.has(&id) {
            let mut next_interactions: MacroInteractions<'hir, 'source> = MacroInteractions::new();
            let result = HostApi::advance_macro_interaction_mir_with_reaction(
                &mut self.pending,
                &mut self.interactions,
                &mut self.state,
                &mut self.story,
                self.bytecode,
                request,
                |body, state, requests, scopes| {
                    let mut context = MacroLogicContext::new(state, requests, scopes);
                    let control = execute_logic_body(body, &mut context).map_err(|error| {
                        Diagnostic::new(
                            "runtime_session.interaction_body",
                            DiagnosticSeverity::Error,
                            &format!("Interaction 正文执行失败：{error:?}"),
                        )
                    })?;
                    Ok::<BodyExecution, Diagnostic>(BodyExecution {
                        control,
                        output: SemanticOutput::default(),
                    })
                },
                |phase, context, _state| emit_passage_event(self.script.as_ref(), phase, context),
                |passage, state, requests| {
                    crate::reaction_runtime::apply_lifecycle_reactions(
                        self.script.as_ref(),
                        self.hir,
                        passage,
                        state,
                        requests,
                    )
                },
                |invocation, state, requests, scopes| {
                    dispatch_macro(
                        self.script.as_ref(),
                        self.hir,
                        &mut next_interactions,
                        &mut self.scheduled,
                        invocation,
                        state,
                        requests,
                        scopes,
                    )
                },
            );
            if result.is_ok() {
                self.interactions = next_interactions;
            }
            result
        } else {
            HostApi::advance_mir_with_reaction(
                &mut self.pending,
                &mut self.state,
                &mut self.story,
                self.bytecode,
                request,
                |phase, context, _state| emit_passage_event(self.script.as_ref(), phase, context),
                |passage, state, requests| {
                    crate::reaction_runtime::apply_lifecycle_reactions(
                        self.script.as_ref(),
                        self.hir,
                        passage,
                        state,
                        requests,
                    )
                },
                |invocation, state, requests, scopes| {
                    dispatch_macro(
                        self.script.as_ref(),
                        self.hir,
                        &mut self.interactions,
                        &mut self.scheduled,
                        invocation,
                        state,
                        requests,
                        scopes,
                    )
                },
            )
        }
        .map_err(|error| diagnostic(error.diagnostic.clone()));
        self.drive_main(result)
    }

    fn history(&mut self, backward: bool) -> Result<RuntimeUpdate, HostErrorDto> {
        self.ensure_idle()?;
        if self.presented.is_none() {
            return Err(HostErrorDto::new(
                "runtime_session.not_started",
                "必须先启动 Runtime",
            ));
        }
        if (backward && !self.story.can_back()) || (!backward && !self.story.can_forward()) {
            return Err(HostErrorDto::new(
                "runtime_session.history_unavailable",
                "Story 历史中没有该方向的条目",
            ));
        }
        let params = Value::Null;
        let identity = self.identity(STORY_ID);
        let language = self.language.clone();
        let result = HostApi::history_mir_with_reaction(
            &mut self.pending,
            &mut self.state,
            &mut self.story,
            self.bytecode,
            backward,
            HostMirRequest {
                params: &params,
                identity,
                limits: limits(),
                language: language.as_deref(),
            },
            |phase, context, _state| emit_passage_event(self.script.as_ref(), phase, context),
            |passage, state, requests| {
                crate::reaction_runtime::apply_lifecycle_reactions(
                    self.script.as_ref(),
                    self.hir,
                    passage,
                    state,
                    requests,
                )
            },
            |invocation, state, requests, scopes| {
                dispatch_macro(
                    self.script.as_ref(),
                    self.hir,
                    &mut self.interactions,
                    &mut self.scheduled,
                    invocation,
                    state,
                    requests,
                    scopes,
                )
            },
        )
        .map_err(|error| diagnostic(error.diagnostic.clone()));
        self.drive_main(result)
    }

    fn input(
        &mut self,
        interaction: &str,
        value: serde_json::Value,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        self.ensure_idle()?;
        let previous: &HostUpdate = self.presented.as_deref().ok_or_else(|| {
            HostErrorDto::new("runtime_session.not_started", "必须先启动 Runtime")
        })?;
        let id: InteractionId = InteractionId::parse(interaction).map_err(|error| {
            HostErrorDto::new("runtime_session.input_interaction", error.to_string())
        })?;
        let binding = previous.surface().input_binding(&id).ok_or_else(|| {
            HostErrorDto::new(
                "runtime_session.unknown_input",
                "输入身份未出现在上一份 Surface 中",
            )
        })?;
        let semantic: SemanticValue = json_to_semantic(&value)?;
        if !binding.accepts(&semantic) {
            return Err(HostErrorDto::new(
                "runtime_session.input_value",
                "输入值不属于当前控件允许的值集合",
            ));
        }
        let expression = parse_expression(binding.receiver.as_str()).map_err(|error| {
            HostErrorDto::new("runtime_session.input_receiver", format!("{error:?}"))
        })?;
        let core_value: Value =
            json_to_value(&value).map_err(|error| HostErrorDto::new(&error.code, error.message))?;
        let checkpoint: StateCheckpoint = self.state.checkpoint();
        if let Err(error) = assign_value_with_mut(&expression, core_value, &mut self.state) {
            self.state.restore_checkpoint(checkpoint);
            return Err(HostErrorDto::new(
                "runtime_session.input_assignment",
                format!("{error:?}"),
            ));
        }
        Ok(RuntimeUpdate::Applied)
    }

    /// 在命令边界统一处理作者 Event 与持久 State 变化。Setter、Event.emit 与
    /// Reaction cond 都只收集事实，不允许重入 Engine；所有叙事效果在这里顺序提交。
    fn settle_reactions(&mut self, update: RuntimeUpdate) -> Result<RuntimeUpdate, HostErrorDto> {
        let mut before = self
            .reaction_before
            .take()
            .unwrap_or_else(|| self.state.snapshot());
        let mut output = std::mem::take(&mut self.reaction_prefix);
        let mut goto: Option<String> = None;
        let mut settled = false;

        for _round in 0..256 {
            let mut reactions = self
                .script
                .resolve_queued_event_reactions(self.story.current(), &mut self.state)
                .map_err(|error| HostErrorDto::new(&error.code, error.message))?;
            reactions.extend(
                self.script
                    .resolve_state_reactions(self.story.current(), &before, &mut self.state)
                    .map_err(|error| HostErrorDto::new(&error.code, error.message))?,
            );
            if reactions.is_empty() {
                settled = true;
                break;
            }
            // 下一轮必须从“本轮效果执行前”继续比较，才能观察效果自身写入的 State。
            let effect_before = self.state.snapshot();
            let (reaction_output, reaction_goto) = self.apply_reaction_effects(reactions)?;
            output.append(reaction_output);
            if let Some(target) = reaction_goto
                && goto.replace(target).is_some()
            {
                return Err(HostErrorDto::new(
                    "reaction.multiple_goto",
                    "同一 Reaction 安全点只能发起一次导航",
                ));
            }
            before = effect_before;
        }
        if !settled {
            return Err(HostErrorDto::new(
                "reaction.execution_limit",
                "Reaction 安全点超过 256 轮执行上限",
            ));
        }

        if let Some(target) = goto {
            self.reaction_before = Some(before);
            // 与普通 goto 一致，被替代 Passage 的输出（含特殊区域与交互）不进入目标页。
            // 仅保留同一 Reaction 安全点在 goto 前明确产生的效果输出。
            self.reaction_prefix = output;
            let params = Value::Null;
            let identity = self.identity(STORY_ID);
            let language = self.language.clone();
            let result = HostApi::navigate_mir_with_reaction(
                &mut self.pending,
                &mut self.state,
                &mut self.story,
                self.bytecode,
                &target,
                HostMirRequest {
                    params: &params,
                    identity,
                    limits: limits(),
                    language: language.as_deref(),
                },
                |phase, context, _state| emit_passage_event(self.script.as_ref(), phase, context),
                |passage, state, requests| {
                    crate::reaction_runtime::apply_lifecycle_reactions(
                        self.script.as_ref(),
                        self.hir,
                        passage,
                        state,
                        requests,
                    )
                },
                |invocation, state, requests, scopes| {
                    dispatch_macro(
                        self.script.as_ref(),
                        self.hir,
                        &mut self.interactions,
                        &mut self.scheduled,
                        invocation,
                        state,
                        requests,
                        scopes,
                    )
                },
            )
            .map_err(|error| diagnostic(error.diagnostic.clone()));
            let driven = self.drive_main(result)?;
            return if matches!(driven, RuntimeUpdate::Pending { .. }) {
                Ok(driven)
            } else {
                self.settle_reactions(driven)
            };
        }

        if !output.is_empty()
            && let Some(presented) = self.presented.as_ref()
        {
            let mut amended = presented.as_ref().clone();
            amended.append_surface(output);
            let dto = encode_host_update(&amended, self.story.can_back(), self.story.can_forward());
            self.presented = Some(Rc::new(amended));
            self.clear_reaction_checkpoint();
            return Ok(RuntimeUpdate::Ready { update: dto });
        }
        self.clear_reaction_checkpoint();
        Ok(update)
    }

    fn clear_reaction_checkpoint(&mut self) {
        self.reaction_checkpoint = None;
        self.reaction_state_checkpoint = None;
        self.reaction_story_snapshot = None;
        self.reaction_presented_checkpoint = None;
        self.reaction_interactions_checkpoint = None;
    }

    fn apply_reaction_effects(
        &mut self,
        reactions: Vec<narrava_loom_core::reaction::ReactionEffect>,
    ) -> Result<(SemanticOutput, Option<String>), HostErrorDto> {
        let mut requests = narrava_loom_core::story::StoryRuntimeRequests::new(&self.story);
        let mut output = SemanticOutput::default();
        for effect in reactions {
            let execution = crate::reaction_runtime::apply_effect(
                self.hir,
                &effect,
                &mut self.state,
                &mut requests,
            )
            .map_err(diagnostic)?;
            output.append(execution.output);
        }
        let target = requests
            .take_goto()
            .map(|request| request.passage().name.to_owned());
        Ok((output, target))
    }

    fn drive_main(
        &mut self,
        result: Result<HostDriveResult, HostErrorDto>,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        match result? {
            HostDriveResult::Ready(update) => self.finish_specials(update, 0),
            HostDriveResult::Pending { execution } => {
                let pending: ScriptPending = self.take_scheduled()?;
                let operation: PendingOperation = protocol_pending(&pending);
                self.waiting = Some(Waiting::Main {
                    operation: pending.id(),
                    execution,
                });
                Ok(RuntimeUpdate::Pending { operation })
            }
        }
    }

    fn resume(
        &mut self,
        operation: u64,
        _result: Option<narrava_loom_protocol::PendingResult>,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let waiting: Waiting<'hir, 'source> = self.take_waiting(operation)?;
        match waiting {
            Waiting::Main { execution, .. } => {
                let result = self.resume_drive(execution)?;
                self.drive_main(result)
            }
            Waiting::Special(waiting) => self.resume_special(*waiting),
            Waiting::Platform(waiting) => self.resume_platform(*waiting, _result),
        }
    }

    fn cancel(&mut self, operation: u64) -> Result<RuntimeUpdate, HostErrorDto> {
        let waiting: Waiting<'hir, 'source> = self.take_waiting(operation)?;
        match waiting {
            Waiting::Main { execution, .. } => {
                HostApi::cancel_pending(
                    &mut self.pending,
                    &mut self.state,
                    &mut self.story,
                    execution,
                )
                .map_err(|error| diagnostic(error.diagnostic.clone()))?;
            }
            Waiting::Special(mut waiting) => {
                HostApi::cancel_pending(
                    &mut waiting.pending,
                    &mut waiting.state,
                    &mut waiting.story,
                    waiting.execution,
                )
                .map_err(|error| diagnostic(error.diagnostic.clone()))?;
            }
            Waiting::Platform(waiting) => {
                if waiting.script_save {
                    let error =
                        HostErrorDto::new("runtime_session.platform_cancelled", "平台操作已取消");
                    self.finish_script_save(&waiting.action, Err(error.clone()))?;
                    self.notices.push(error);
                    return Ok(waiting.after);
                }
            }
        }
        Ok(RuntimeUpdate::Applied)
    }

    fn resume_drive(
        &mut self,
        execution: narrava_loom_core::host::HostExecutionToken,
    ) -> Result<Result<HostDriveResult, HostErrorDto>, HostErrorDto> {
        let resumed = HostApi::resume_pending(
            &mut self.pending,
            &mut self.state,
            &mut self.story,
            self.bytecode,
            execution,
            |handle, state, _requests, _scopes| {
                resume_script(self.script.as_ref(), handle, state, &mut self.scheduled)
            },
        )
        .map_err(diagnostic)?;
        Ok(self.continue_resumed(resumed))
    }

    fn continue_resumed(
        &mut self,
        resumed: HostResumeOutcome<'hir, 'source>,
    ) -> Result<HostDriveResult, HostErrorDto> {
        match resumed {
            HostResumeOutcome::Pending { execution } => Ok(HostDriveResult::Pending { execution }),
            HostResumeOutcome::Continue(resumed) => {
                let stable = HostApi::continue_resumed(
                    *resumed,
                    &mut self.state,
                    &mut self.story,
                    self.bytecode,
                )
                .map_err(diagnostic)?;
                HostApi::drive_stable_with_reaction(
                    stable,
                    &mut self.pending,
                    &mut self.state,
                    &mut self.story,
                    self.bytecode,
                    |phase, context, _state| {
                        emit_passage_event(self.script.as_ref(), phase, context)
                    },
                    |passage, state, requests| {
                        crate::reaction_runtime::apply_lifecycle_reactions(
                            self.script.as_ref(),
                            self.hir,
                            passage,
                            state,
                            requests,
                        )
                    },
                    |invocation, state, requests, scopes| {
                        dispatch_macro(
                            self.script.as_ref(),
                            self.hir,
                            &mut self.interactions,
                            &mut self.scheduled,
                            invocation,
                            state,
                            requests,
                            scopes,
                        )
                    },
                )
                .map_err(|error| diagnostic(error.diagnostic.clone()))
            }
        }
    }

    fn finish_specials(
        &mut self,
        mut update: HostUpdate,
        mut next_special: usize,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let specials: [(&str, RegionId); 2] = [
            (BAR_PASSAGE, RegionId::bar()),
            (BAR_STOWED_PASSAGE, RegionId::bar_stowed()),
        ];
        while let Some((name, region)) = specials.get(next_special).cloned() {
            next_special += 1;
            if !self.story.has(name) {
                continue;
            }
            let mut view_state: State = self.state.fork_view();
            let mut view_story: Story<'hir, 'source> = self.story.fork_view();
            let mut pending = HostPendingExecutions::new();
            let params: Value = Value::Null;
            let identity: RuntimeExecutionIdentity = self.identity(SPECIAL_STORY_ID);
            let language: Option<Rc<I18nRuntimeLanguage>> = self.language.clone();
            let result = HostApi::render_special_mir(
                &mut pending,
                &mut view_state,
                &mut view_story,
                self.bytecode,
                name,
                HostMirRequest {
                    params: &params,
                    identity,
                    limits: limits(),
                    language: language.as_deref(),
                },
                |_phase, _context, _state| Ok::<(), Diagnostic>(()),
                |invocation, state, requests, scopes| {
                    dispatch_macro(
                        self.script.as_ref(),
                        self.hir,
                        &mut self.interactions,
                        &mut self.scheduled,
                        invocation,
                        state,
                        requests,
                        scopes,
                    )
                },
            )
            .map_err(|error| diagnostic(error.diagnostic.clone()))?;
            match result {
                HostDriveResult::Ready(rendered) => {
                    update.append_region(region, rendered.surface().clone())
                }
                HostDriveResult::Pending { execution } => {
                    let script_pending: ScriptPending = self.take_scheduled()?;
                    let operation: PendingOperation = protocol_pending(&script_pending);
                    self.waiting = Some(Waiting::Special(Box::new(SpecialWaiting {
                        operation: script_pending.id(),
                        execution,
                        update,
                        next_special,
                        region,
                        state: view_state,
                        story: view_story,
                        pending,
                    })));
                    return Ok(RuntimeUpdate::Pending { operation });
                }
            }
        }
        let dto = encode_host_update(&update, self.story.can_back(), self.story.can_forward());
        self.presented = Some(Rc::new(update));
        Ok(RuntimeUpdate::Ready { update: dto })
    }

    fn resume_special(
        &mut self,
        mut waiting: SpecialWaiting<'hir, 'source>,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let resumed = HostApi::resume_pending(
            &mut waiting.pending,
            &mut waiting.state,
            &mut waiting.story,
            self.bytecode,
            waiting.execution,
            |handle, state, _requests, _scopes| {
                resume_script(self.script.as_ref(), handle, state, &mut self.scheduled)
            },
        )
        .map_err(diagnostic)?;
        let result = match resumed {
            HostResumeOutcome::Pending { execution } => HostDriveResult::Pending { execution },
            HostResumeOutcome::Continue(resumed) => {
                let stable = HostApi::continue_resumed(
                    *resumed,
                    &mut waiting.state,
                    &mut waiting.story,
                    self.bytecode,
                )
                .map_err(diagnostic)?;
                HostApi::drive_stable(
                    stable,
                    &mut waiting.pending,
                    &mut waiting.state,
                    &mut waiting.story,
                    self.bytecode,
                    |_phase, _context, _state| Ok::<(), Diagnostic>(()),
                    |invocation, state, requests, scopes| {
                        dispatch_macro(
                            self.script.as_ref(),
                            self.hir,
                            &mut self.interactions,
                            &mut self.scheduled,
                            invocation,
                            state,
                            requests,
                            scopes,
                        )
                    },
                )
                .map_err(|error| diagnostic(error.diagnostic.clone()))?
            }
        };
        match result {
            HostDriveResult::Ready(rendered) => {
                waiting
                    .update
                    .append_region(waiting.region, rendered.surface().clone());
                self.finish_specials(waiting.update, waiting.next_special)
            }
            HostDriveResult::Pending { execution } => {
                let script_pending: ScriptPending = self.take_scheduled()?;
                let operation: PendingOperation = protocol_pending(&script_pending);
                waiting.operation = script_pending.id();
                waiting.execution = execution;
                self.waiting = Some(Waiting::Special(Box::new(waiting)));
                Ok(RuntimeUpdate::Pending { operation })
            }
        }
    }

    fn take_scheduled(&mut self) -> Result<ScriptPending, HostErrorDto> {
        self.scheduled.take().ok_or_else(|| {
            HostErrorDto::new(
                "runtime_session.pending_without_operation",
                "Engine 已暂停，但 Script adapter 没有登记挂起操作",
            )
        })
    }

    fn take_waiting(&mut self, operation: u64) -> Result<Waiting<'hir, 'source>, HostErrorDto> {
        let waiting: Waiting<'hir, 'source> = self.waiting.take().ok_or_else(|| {
            HostErrorDto::new(
                "runtime_session.unknown_operation",
                "Runtime 没有等待中的操作",
            )
        })?;
        let expected: u64 = match &waiting {
            Waiting::Main { operation, .. } => *operation,
            Waiting::Special(waiting) => waiting.operation,
            Waiting::Platform(waiting) => waiting.operation,
        };
        if expected != operation {
            self.waiting = Some(waiting);
            return Err(HostErrorDto::new(
                "runtime_session.operation_mismatch",
                "operation ID 与当前挂起操作不匹配",
            ));
        }
        Ok(waiting)
    }
}

fn resume_script(
    script: &impl ScriptAdapter,
    handle: ScriptPending,
    state: &mut State,
    scheduled: &mut Option<ScriptPending>,
) -> Result<
    MacroHandlerOutcome<narrava_loom_core::runtime::RuntimeMacroExecution, ScriptPending>,
    String,
> {
    match script.resume_macro(handle, state) {
        Ok(ScriptMacroOutcome::Complete(value)) => macro_value_execution(&value)
            .map(MacroHandlerOutcome::Complete)
            .map_err(|error| error.to_string()),
        Ok(ScriptMacroOutcome::Pending(next)) => {
            *scheduled = Some(next.clone());
            Ok(MacroHandlerOutcome::Pending(next))
        }
        Err(error) => Err(error.to_string()),
    }
}

fn protocol_pending(pending: &ScriptPending) -> PendingOperation {
    PendingOperation::Delay {
        operation: pending.id(),
        milliseconds: pending.milliseconds(),
    }
}

fn json_to_semantic(value: &serde_json::Value) -> Result<SemanticValue, HostErrorDto> {
    match value {
        serde_json::Value::Null => Ok(SemanticValue::Null),
        serde_json::Value::Bool(value) => Ok(SemanticValue::Boolean(*value)),
        serde_json::Value::Number(value) => value
            .as_f64()
            .filter(|value| value.is_finite())
            .map(SemanticValue::Number)
            .ok_or_else(|| {
                HostErrorDto::new("runtime_session.input_number", "输入数字必须是有限值")
            }),
        serde_json::Value::String(value) => Ok(SemanticValue::Text(value.clone())),
        _ => Err(HostErrorDto::new(
            "runtime_session.input_value",
            "输入值必须是 null、布尔、数字或字符串",
        )),
    }
}

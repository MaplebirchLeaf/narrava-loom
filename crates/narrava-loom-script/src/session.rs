//! Host-neutral Narrava 生命周期编排。

mod actions;
mod console;
mod inputs;
mod inspect;
mod passage;
mod reactions;
mod specials;
mod state_io;

pub use state_io::RuntimeData;

use std::rc::Rc;

use narrava_loom_core::{
    bytecode::BytecodeProgram,
    diagnostic::{Diagnostic, DiagnosticSeverity},
    engine::{EngineExecutionLimits, EngineMirContinuation},
    expression::{evaluator::assign_value_with_mut, value::Value},
    hir::HirStory,
    host::{
        HostApi, HostDriveResult, HostInput, HostMirAdvanceRequest, HostMirRequest,
        HostPendingExecutions, HostResumeOutcome, HostUpdate,
    },
    i18n::I18nRuntimeLanguage,
    macro_runtime::{MacroHandlerOutcome, MacroInteractions, MacroLogicContext},
    runtime::{BodyExecution, RuntimeExecutionIdentity, execute_logic_body},
    semantic::{InteractionId, RegionId, SemanticOutput, SemanticValue},
    state::{State, StateCheckpoint, StateSnapshot},
    story::{
        Story,
        special::{BAR_PASSAGE, BAR_STOWED_PASSAGE, FOOTER_PASSAGE, HEADER_PASSAGE},
    },
};
use narrava_loom_protocol::{
    HostErrorDto, PendingOperation, RuntimeCommand, RuntimeUpdate, SaveOperation,
};

use crate::{
    EcmaBinding, ScriptError, ScriptMacroOutcome, ScriptPending,
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

enum Pending<'hir, 'source> {
    Console(ScriptPending),
    Action(Box<actions::PendingAction>),
    Main {
        operation: u64,
        execution: narrava_loom_core::host::HostExecutionToken,
    },
    Special(Box<SpecialExecution<'hir, 'source>>),
    Host(Box<HostOperation<'hir, 'source>>),
}

enum HostAction {
    Save {
        operation: SaveOperation,
        target: String,
    },
    SelectLanguage {
        locale: String,
    },
}

/// 输入或开发命令触发的保存仍属于原始事务，失败时恢复完整运行状态。
type InputCheckpoint<'hir, 'source> = RuntimeTransaction<'hir, 'source>;

struct HostOperation<'hir, 'source> {
    operation: u64,
    action: HostAction,
    after: RuntimeUpdate,
    script_save: bool,
    input_checkpoint: Option<InputCheckpoint<'hir, 'source>>,
}

struct SpecialExecution<'hir, 'source> {
    operation: u64,
    execution: narrava_loom_core::host::HostExecutionToken,
    update: HostUpdate,
    next_special: usize,
    region: RegionId,
    state: State,
    story: Story<'hir, 'source>,
    continuations: HostPendingExecutions<EngineMirContinuation<'hir, 'source, ScriptPending>>,
}

/// 从命令进入到 Reaction 安全点完成的一次事务；Pending 期间整体保留。
struct RuntimeTransaction<'hir, 'source> {
    /// 输入与开发命令需把检查点保留到平台存档完成；Pending 期间沿用。
    input: bool,
    before: Rc<StateSnapshot>,
    state: StateCheckpoint,
    story: narrava_loom_core::story::StorySnapshot<'hir, 'source>,
    presented: Option<Rc<HostUpdate>>,
    interactions: MacroInteractions<'hir, 'source>,
    reactions: Vec<narrava_loom_core::reaction::ReactionRuntimeState>,
    output: SemanticOutput,
}

/// 一局游戏的 Host-neutral Runtime 所有权根。
///
/// Host 只能发送拥有型 [`RuntimeCommand`] 并消费 [`RuntimeUpdate`]；Engine
/// continuation、上一份可交互输出、State、Story 与脚本 interaction 均不越过此边界。
pub struct RuntimeSession<'hir, 'source> {
    hir: &'hir HirStory<'source>,
    bytecode: &'hir BytecodeProgram,
    script: Rc<EcmaBinding>,
    state: State,
    story: Story<'hir, 'source>,
    interactions: MacroInteractions<'hir, 'source>,
    continuations: HostPendingExecutions<EngineMirContinuation<'hir, 'source, ScriptPending>>,
    pending: Option<Pending<'hir, 'source>>,
    presented: Option<Rc<HostUpdate>>,
    language: Option<Rc<I18nRuntimeLanguage>>,
    sequence: u64,
    initial_state: State,
    initial_reactions: Vec<narrava_loom_core::reaction::ReactionRuntimeState>,
    console_sequence: u64,
    console_result: Option<narrava_loom_protocol::HostDebugEvaluationDto>,
    data: Option<RuntimeData>,
    notices: Vec<HostErrorDto>,
    transaction: Option<RuntimeTransaction<'hir, 'source>>,
}

impl<'hir, 'source> RuntimeSession<'hir, 'source> {
    /// 建立一局 Runtime；传入的 State 应已被脚本 Binding 初始化。
    pub fn new(
        hir: &'hir HirStory<'source>,
        bytecode: &'hir BytecodeProgram,
        script: Rc<EcmaBinding>,
        state: State,
    ) -> Self {
        Self::create(hir, bytecode, script, state, None)
    }

    /// 在唯一构造入口附着 Script dispatcher 与可选数据。
    fn create(
        hir: &'hir HirStory<'source>,
        bytecode: &'hir BytecodeProgram,
        script: Rc<EcmaBinding>,
        mut state: State,
        data: Option<RuntimeData>,
    ) -> Self {
        state.attach_script_dispatcher(script.clone());
        let initial_state: State = state.fork_view();
        let initial_reactions = script.reaction_state();
        Self {
            hir,
            bytecode,
            script,
            state,
            story: Story::new(hir),
            interactions: MacroInteractions::new(),
            continuations: HostPendingExecutions::new(),
            pending: None,
            presented: None,
            language: None,
            sequence: 1,
            initial_state,
            initial_reactions,
            console_sequence: 0,
            console_result: None,
            data,
            notices: Vec::new(),
            transaction: None,
        }
    }

    /// 建立带 Save/I18n 数据的 Runtime；平台文件 IO 仍通过 PendingOperation 完成。
    pub fn with_data(
        hir: &'hir HirStory<'source>,
        bytecode: &'hir BytecodeProgram,
        script: Rc<EcmaBinding>,
        state: State,
        data: RuntimeData,
    ) -> Self {
        Self::create(hir, bytecode, script, state, Some(data))
    }

    /// 执行一条平台无关命令；Pending 必须以返回的 operation ID 恢复或取消。
    pub fn execute(&mut self, command: RuntimeCommand) -> Result<RuntimeUpdate, HostErrorDto> {
        let previous_notices: usize = self.notices.len();
        let result: Result<RuntimeUpdate, HostErrorDto> = self.execute_command(command);
        if let Err(error) = &result {
            self.record_host_error(error);
        }
        for notice in self.notices.iter().skip(previous_notices) {
            self.record_host_error(notice);
        }
        result
    }

    fn execute_command(&mut self, command: RuntimeCommand) -> Result<RuntimeUpdate, HostErrorDto> {
        // 错误 operation ID 不能清除仍在等待恢复的事务音频。
        match &command {
            RuntimeCommand::Resume { operation, .. } | RuntimeCommand::Cancel { operation } => {
                self.check_operation(*operation)?
            }
            _ => self.ensure_idle()?,
        }
        self.script
            .audio_control("beginAudio", serde_json::json!([]))
            .map_err(crate::ScriptError::into_host_error)?;
        let cancel: bool = matches!(command, RuntimeCommand::Cancel { .. });
        let result = self.execute_inner(command);
        if !matches!(result, Ok(RuntimeUpdate::Pending { .. })) {
            self.script.set_refresh(None);
            self.state.end_random_replay();
        }
        if result.is_err() {
            self.script.cancel_console();
            self.console_result = None;
        }
        if result.is_err() || cancel {
            let _discarded = self
                .script
                .audio_control("rollbackAudio", serde_json::json!([]));
            return result;
        }
        let update = result?;
        if matches!(update, RuntimeUpdate::Pending { .. }) {
            return Ok(update);
        }
        let effects = self
            .script
            .take_audio()
            .map_err(crate::ScriptError::into_host_error)?;
        if effects.is_empty() {
            return Ok(update);
        }
        match update {
            RuntimeUpdate::Ready { update } => Ok(RuntimeUpdate::Audio {
                effects,
                update: Some(update),
            }),
            RuntimeUpdate::Applied => Ok(RuntimeUpdate::Audio {
                effects,
                update: None,
            }),
            _ => unreachable!("音频只附着最终完成结果"),
        }
    }

    fn execute_inner(&mut self, command: RuntimeCommand) -> Result<RuntimeUpdate, HostErrorDto> {
        let cancels_execution = matches!(&command, RuntimeCommand::Cancel { .. });
        let executes_story: bool = matches!(
            &command,
            RuntimeCommand::Start
                | RuntimeCommand::DebugScript { .. }
                | RuntimeCommand::Back
                | RuntimeCommand::Forward
                | RuntimeCommand::Activate { .. }
                | RuntimeCommand::Input { .. }
                | RuntimeCommand::Resume { .. }
        );
        let is_input: bool = matches!(
            &command,
            RuntimeCommand::Input { .. } | RuntimeCommand::DebugScript { .. }
        );
        if executes_story {
            self.begin_transaction();
            self.transaction
                .as_mut()
                .expect("执行命令必须持有事务")
                .input |= is_input;
        }
        let result: Result<RuntimeUpdate, HostErrorDto> = match command {
            RuntimeCommand::DebugScript { source } => self.execute_console(&source),
            RuntimeCommand::Start => self.start(),
            RuntimeCommand::Back => self.history(true),
            RuntimeCommand::Forward => self.history(false),
            RuntimeCommand::Activate { interaction } => self.activate(&interaction),
            RuntimeCommand::Input { interaction, value } => self.input(&interaction, value),
            RuntimeCommand::Save { operation, target } => {
                self.begin_save(operation, target, RuntimeUpdate::Applied, false)
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
                self.rollback_transaction();
                return Err(error);
            }
        };
        if cancels_execution {
            self.rollback_transaction();
            return Ok(update);
        }
        if executes_story && !matches!(update, RuntimeUpdate::Pending { .. }) {
            update = match self.settle_reactions(update) {
                Ok(update) => update,
                Err(error) => {
                    self.rollback_transaction();
                    return Err(error);
                }
            };
        }
        let mut input_checkpoint: Option<InputCheckpoint<'hir, 'source>> = None;
        if executes_story && !matches!(update, RuntimeUpdate::Pending { .. }) {
            input_checkpoint = self
                .transaction
                .take()
                .filter(|transaction| transaction.input);
        }
        if executes_story && !matches!(update, RuntimeUpdate::Pending { .. }) {
            match self.process_script_save(update.clone(), &mut input_checkpoint) {
                Ok(Some(pending)) => return Ok(pending),
                Ok(None) => {}
                Err(error) => {
                    if let Some(checkpoint) = input_checkpoint {
                        self.transaction = Some(checkpoint);
                        self.rollback_transaction();
                        return Err(error);
                    }
                    self.notices.push(error);
                }
            }
        }
        if executes_story && !matches!(update, RuntimeUpdate::Pending { .. }) {
            match self.process_script_language(update.clone()) {
                Ok(Some(pending)) => return Ok(pending),
                Ok(None) => {}
                Err(error) => self.notices.push(error),
            }
        }
        Ok(update)
    }

    fn begin_transaction(&mut self) {
        if self.transaction.is_none() {
            self.transaction = Some(RuntimeTransaction {
                input: false,
                before: Rc::new(self.state.snapshot()),
                state: self.state.checkpoint(),
                story: self.story.snapshot(),
                presented: self.presented.clone(),
                interactions: self.interactions.clone(),
                reactions: self.script.reaction_state(),
                output: SemanticOutput::default(),
            });
        }
    }

    fn rollback_transaction(&mut self) {
        let Some(transaction) = self.transaction.take() else {
            return;
        };
        let _ignored = self.script.restore_reaction_state(&transaction.reactions);
        self.state.restore_checkpoint(transaction.state);
        self.story
            .restore(transaction.story)
            .expect("同一 Session 的事务必须属于当前 Story");
        self.presented = transaction.presented;
        self.interactions = transaction.interactions;
    }

    /// 取走命令成功后产生的非阻塞平台提示，例如导航完成后的自动存档失败。
    pub fn take_notices(&mut self) -> Vec<HostErrorDto> {
        std::mem::take(&mut self.notices)
    }

    fn ensure_idle(&self) -> Result<(), HostErrorDto> {
        if self.pending.is_some() {
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

    fn drive_main(
        &mut self,
        result: Result<HostDriveResult, HostErrorDto>,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        match result? {
            HostDriveResult::Ready(update) => self.finish_specials(update, 0),
            HostDriveResult::Pending { execution } => {
                let pending: &ScriptPending = &self
                    .continuations
                    .get(execution)
                    .expect("Engine 已登记暂停执行")
                    .runtime()
                    .suspension()
                    .handle;
                let operation: PendingOperation = protocol_pending(pending);
                self.pending = Some(Pending::Main {
                    operation: pending.id(),
                    execution,
                });
                Ok(RuntimeUpdate::Pending { operation })
            }
        }
    }

    fn resume(
        &mut self,
        _operation: u64,
        _result: Option<narrava_loom_protocol::PendingResult>,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let waiting: Pending<'hir, 'source> = self.pending.take().expect("命令入口已确认挂起操作");
        match waiting {
            Pending::Console(pending) => {
                let outcome = self
                    .script
                    .resume_console(pending, &mut self.state)
                    .map_err(ScriptError::into_host_error)?;
                self.finish_console(outcome)
            }
            Pending::Main { execution, .. } => {
                let resumed = HostApi::resume_pending(
                    &mut self.continuations,
                    &mut self.state,
                    &mut self.story,
                    self.bytecode,
                    execution,
                    |handle, state, _requests, _scopes| {
                        resume_script(self.script.as_ref(), handle, state)
                    },
                )
                .map_err(diagnostic)?;
                let result = self.continue_resumed(resumed);
                self.drive_main(result)
            }
            Pending::Action(waiting) => self.resume_action(*waiting),
            Pending::Special(waiting) => self.resume_special(*waiting),
            Pending::Host(waiting) => self.resume_host(*waiting, _result),
        }
    }

    fn cancel(&mut self, _operation: u64) -> Result<RuntimeUpdate, HostErrorDto> {
        let waiting: Pending<'hir, 'source> = self.pending.take().expect("命令入口已确认挂起操作");
        match waiting {
            Pending::Console(_) => {
                self.script.cancel_console();
                self.console_result = None;
            }
            Pending::Main { execution, .. } => {
                HostApi::cancel_pending(
                    &mut self.continuations,
                    &mut self.state,
                    &mut self.story,
                    execution,
                )
                .map_err(|error| diagnostic(error.diagnostic.clone()))?;
            }
            Pending::Action(_) => {}
            Pending::Special(mut waiting) => {
                HostApi::cancel_pending(
                    &mut waiting.continuations,
                    &mut waiting.state,
                    &mut waiting.story,
                    waiting.execution,
                )
                .map_err(|error| diagnostic(error.diagnostic.clone()))?;
            }
            Pending::Host(waiting) => {
                if waiting.script_save {
                    let input: bool = waiting.input_checkpoint.is_some();
                    if let Some(checkpoint) = waiting.input_checkpoint {
                        self.transaction = Some(checkpoint);
                    }
                    let error =
                        HostErrorDto::new("runtime_session.platform_cancelled", "平台操作已取消");
                    self.finish_script_save(&waiting.action, Err(error.clone()))?;
                    self.notices.push(error);
                    return Ok(if input {
                        RuntimeUpdate::Applied
                    } else {
                        waiting.after
                    });
                }
            }
        }
        Ok(RuntimeUpdate::Applied)
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
                    &mut self.continuations,
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

    fn check_operation(&self, operation: u64) -> Result<(), HostErrorDto> {
        let waiting = self.pending.as_ref().ok_or_else(|| {
            HostErrorDto::new(
                "runtime_session.unknown_operation",
                "Runtime 没有等待中的操作",
            )
        })?;
        let expected: u64 = match &waiting {
            Pending::Console(pending) => pending.id(),
            Pending::Main { operation, .. } => *operation,
            Pending::Action(waiting) => waiting.operation(),
            Pending::Special(waiting) => waiting.operation,
            Pending::Host(waiting) => waiting.operation,
        };
        if expected != operation {
            return Err(HostErrorDto::new(
                "runtime_session.operation_mismatch",
                "operation ID 与当前挂起操作不匹配",
            ));
        }
        Ok(())
    }
}

fn resume_script(
    script: &EcmaBinding,
    handle: ScriptPending,
    state: &mut State,
) -> Result<
    MacroHandlerOutcome<narrava_loom_core::runtime::RuntimeMacroExecution, ScriptPending>,
    narrava_loom_core::diagnostic::Diagnostic,
> {
    match script.resume_macro(handle, state) {
        Ok(ScriptMacroOutcome::Complete(value)) => macro_value_execution(&value)
            .map(MacroHandlerOutcome::Complete)
            .map_err(|error| crate::protocol_adapter::core_diagnostic(&error.into_host_error())),
        Ok(ScriptMacroOutcome::Pending(next)) => Ok(MacroHandlerOutcome::Pending(next)),
        Err(error) => Err(crate::protocol_adapter::core_diagnostic(
            &error.into_host_error(),
        )),
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

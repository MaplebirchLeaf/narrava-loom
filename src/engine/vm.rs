//! Bytecode VM 到现有 Engine 导航事务的适配。

use crate::{
    bytecode::{BytecodePassage, BytecodeProgram},
    expression::value::Value,
    hir::{HirMacro, HirPassage},
    i18n::I18nRuntimeLanguage,
    macro_runtime::MacroStoryAccess,
    runtime::{BodyControl, BodyExecution, RuntimeExecutionIdentity, RuntimeMacroExecution},
    state::State,
    story::{Story, StoryRuntimeRequestError, StoryRuntimeRequests},
    vm::{MirExecutionError, MirExecutionFrame, MirStep},
};

use super::{
    Engine, EngineExecutionLimits, EngineMirBeginTransaction, EngineMirProgress, EngineMirVmResume,
    EngineMirVmResumeError, EngineNavigationChain, EngineNavigationError,
    EngineRequestedExecutionError, PassageLifecycleContext, PassageLifecyclePhase,
    begin_mir_transaction,
};
use crate::{state::StateCheckpoint, story::StorySnapshot};

/// Engine 进入第一个 MIR Passage 前的准备错误。
#[derive(Debug, PartialEq)]
pub enum EngineMirBeginExecutionError<LifecycleError> {
    /// 当前编译结果中没有对应名称的 Bytecode Passage。
    MissingMirPassage(String),
    /// Init／Start 生命周期回调失败。
    Lifecycle(LifecycleError),
}

/// Engine 开始 MIR 链时的失败阶段：进入 Passage 的准备错误或 VM 驱动错误。
pub enum EngineMirBeginError<'hir, 'source, LifecycleError> {
    /// 进入入口 Passage、执行 Init／Start 前的准备失败。
    Preparation(
        EngineNavigationError<
            EngineRequestedExecutionError<EngineMirBeginExecutionError<LifecycleError>>,
        >,
    ),
    /// 已进入后，驱动 VM 到首个稳定边界时失败。
    Continue(Box<EngineMirVmResumeError<'hir, 'source>>),
}

/// Engine 开始 MIR 链的入口请求参数。
#[derive(Clone, Copy, Debug)]
pub struct EngineMirBeginRequest<'params> {
    /// 起始 Passage 名称。
    pub name: &'params str,
    /// 交给入口 Passage 生命周期与正文的参数。
    pub params: &'params Value,
    /// 本次执行链的运行时身份，随暂停与恢复校验。
    pub identity: RuntimeExecutionIdentity,
    /// 本次开始事务的控制流预算。
    pub limits: EngineExecutionLimits,
    /// 目标语言必须先由当前编译结果的 I18n 目录校验。
    pub language: Option<&'params I18nRuntimeLanguage>,
}

/// 启动方在 StoryInit 前取得的检查点，使初始化与 Start 共用一次事务。
pub(crate) struct EngineMirBeginCheckpointRequest<'hir, 'source, 'params> {
    pub request: EngineMirBeginRequest<'params>,
    pub state_checkpoint: StateCheckpoint,
    pub story_snapshot: StorySnapshot<'hir, 'source>,
}

/// Engine 驱动 Bytecode Passage 时保留 VM、映射与 include 预算错误。
/// Engine 驱动 Bytecode Passage 时保留 VM、映射与 include 预算错误。
#[derive(Debug, PartialEq)]
pub enum EngineMirExecutionError {
    /// 当前编译结果中没有对应名称的 Bytecode Passage。
    MissingMirPassage(String),
    /// VM 步进或 Macro 完成写入失败。
    Vm(MirExecutionError),
    /// Story 导航请求失败。
    Story(StoryRuntimeRequestError),
    /// 本条执行链展开的 include 超过预算。
    IncludeLimitExceeded { limit: usize },
    /// 遇到动态 Macro，但当前执行入口没有安装 Macro 控制器。
    MacroPending,
}

/// Engine 接通 Macro 控制器后能够区分的 MIR、Macro 与边界错误。
/// Engine 接通 Macro 控制器后能够区分的 MIR、Macro 与边界错误。
#[derive(Debug, PartialEq)]
pub enum EngineMirMacroExecutionError<MacroError> {
    /// VM 或 Story 映射层的错误。
    Mir(EngineMirExecutionError),
    /// Macro 控制器自身报告的错误。
    Macro(MacroError),
    /// Macro 返回了 Engine 同步链不支持的停止信号。
    UnexpectedMacroControl(BodyControl),
}

impl Engine {
    /// 进入首个 Passage、执行 Init／Start，并驱动 LIR 到第一个稳定边界。
    pub fn begin_mir_chain_with_reaction<'hir, 'source, LifecycleError>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        request: EngineMirBeginRequest<'_>,
        lifecycle: impl FnMut(
            PassageLifecyclePhase,
            PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), LifecycleError>,
        reaction: impl FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, LifecycleError>,
    ) -> Result<EngineMirVmResume<'hir, 'source>, EngineMirBeginError<'hir, 'source, LifecycleError>>
    {
        let state_checkpoint: StateCheckpoint = state.checkpoint();
        let story_snapshot: StorySnapshot<'hir, 'source> = story.snapshot();
        Self::begin_mir_chain_from_checkpoint_with_reaction(
            state,
            story,
            mir,
            EngineMirBeginCheckpointRequest {
                request,
                state_checkpoint,
                story_snapshot,
            },
            lifecycle,
            reaction,
        )
    }

    /// 不安装 Reaction Phase 的兼容入口。
    pub fn begin_mir_chain<'hir, 'source, LifecycleError>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        request: EngineMirBeginRequest<'_>,
        lifecycle: impl FnMut(
            PassageLifecyclePhase,
            PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), LifecycleError>,
    ) -> Result<EngineMirVmResume<'hir, 'source>, EngineMirBeginError<'hir, 'source, LifecycleError>>
    {
        Self::begin_mir_chain_with_reaction(
            state,
            story,
            mir,
            request,
            lifecycle,
            |_passage, _state, _requests| Ok(BodyExecution::default()),
        )
    }

    /// 使用调用方在启动准备前取得的检查点进入首个 Passage。
    pub(crate) fn begin_mir_chain_from_checkpoint_with_reaction<'hir, 'source, LifecycleError>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        checkpoint: EngineMirBeginCheckpointRequest<'hir, 'source, '_>,
        lifecycle: impl FnMut(
            PassageLifecyclePhase,
            PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), LifecycleError>,
        reaction: impl FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, LifecycleError>,
    ) -> Result<EngineMirVmResume<'hir, 'source>, EngineMirBeginError<'hir, 'source, LifecycleError>>
    {
        let EngineMirBeginCheckpointRequest {
            request,
            state_checkpoint,
            story_snapshot,
        } = checkpoint;
        let EngineMirBeginRequest {
            name,
            params,
            identity,
            limits,
            language,
        } = request;
        let current = *story.goto(name).map_err(|error| {
            EngineMirBeginError::Preparation(EngineNavigationError::Navigation(error))
        })?;
        story.record_state_snapshot(current.id(), state.snapshot());
        Self::begin_mir_chain_from_entry(
            state,
            story,
            mir,
            current,
            params,
            identity,
            limits,
            language,
            state_checkpoint,
            story_snapshot,
            lifecycle,
            reaction,
        )
    }

    pub(crate) fn begin_mir_chain_from_checkpoint<'hir, 'source, LifecycleError>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        checkpoint: EngineMirBeginCheckpointRequest<'hir, 'source, '_>,
        lifecycle: impl FnMut(
            PassageLifecyclePhase,
            PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), LifecycleError>,
    ) -> Result<EngineMirVmResume<'hir, 'source>, EngineMirBeginError<'hir, 'source, LifecycleError>>
    {
        Self::begin_mir_chain_from_checkpoint_with_reaction(
            state,
            story,
            mir,
            checkpoint,
            lifecycle,
            |_passage, _state, _requests| Ok(BodyExecution::default()),
        )
    }

    /// 从已经由 Story 历史游标选中的条目继续执行，不追加新的历史记录。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin_mir_chain_from_entry<'hir, 'source, LifecycleError>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        current: crate::story::StoryHistoryEntry<'hir, 'source>,
        params: &Value,
        identity: RuntimeExecutionIdentity,
        limits: EngineExecutionLimits,
        language: Option<&I18nRuntimeLanguage>,
        state_checkpoint: StateCheckpoint,
        story_snapshot: StorySnapshot<'hir, 'source>,
        mut lifecycle: impl FnMut(
            PassageLifecyclePhase,
            PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), LifecycleError>,
        mut reaction: impl FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, LifecycleError>,
    ) -> Result<EngineMirVmResume<'hir, 'source>, EngineMirBeginError<'hir, 'source, LifecycleError>>
    {
        if limits.passages == 0 {
            let error = Self::rollback(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                EngineRequestedExecutionError::PassageLimitExceeded {
                    limit: limits.passages,
                },
            );
            return Err(EngineMirBeginError::Preparation(error));
        }
        let Some(passage) = mir.passage(current.passage().name) else {
            let error = Self::rollback(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                EngineRequestedExecutionError::Runtime(
                    EngineMirBeginExecutionError::MissingMirPassage(
                        current.passage().name.to_owned(),
                    ),
                ),
            );
            return Err(EngineMirBeginError::Preparation(error));
        };
        let _removed_temporary: usize = state.temporary_clear();
        let context = PassageLifecycleContext::new(current, params);
        for phase in [PassageLifecyclePhase::Init, PassageLifecyclePhase::Start] {
            if let Err(error) = lifecycle(phase, context, state) {
                let error = Self::rollback(
                    state,
                    story,
                    state_checkpoint,
                    story_snapshot,
                    EngineRequestedExecutionError::Lifecycle {
                        phase,
                        error: EngineMirBeginExecutionError::Lifecycle(error),
                    },
                );
                return Err(EngineMirBeginError::Preparation(error));
            }
        }
        let mut requests = StoryRuntimeRequests::new(story);
        let reaction_execution: BodyExecution =
            match reaction(current.passage(), state, &mut requests) {
                Ok(execution) => execution,
                Err(error) => {
                    let error = Self::rollback(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        EngineRequestedExecutionError::Lifecycle {
                            phase: PassageLifecyclePhase::Start,
                            error: EngineMirBeginExecutionError::Lifecycle(error),
                        },
                    );
                    return Err(EngineMirBeginError::Preparation(error));
                }
            };
        let requests = requests.into_pending();
        let progress = EngineMirProgress::new(
            current,
            vec![current],
            reaction_execution.output,
            params,
            0,
            0,
            limits,
        )
        .with_language(language.cloned());
        begin_mir_transaction(
            EngineMirBeginTransaction {
                identity,
                frame: MirExecutionFrame::new(passage),
                control: reaction_execution.control,
                state_checkpoint,
                story_snapshot,
                requests,
                progress,
            },
            mir,
            state,
            story,
        )
        .map_err(|error| EngineMirBeginError::Continue(Box::new(error)))
    }

    /// 使用现有 Engine 检查点与导航确认流程执行 LIR Passage 链。
    pub fn navigate_mir_chain<'hir, 'source>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        name: &str,
        limits: EngineExecutionLimits,
    ) -> Result<
        EngineNavigationChain<'hir, 'source>,
        EngineNavigationError<EngineRequestedExecutionError<EngineMirExecutionError>>,
    > {
        Self::navigate_chain_with_requests(
            state,
            story,
            name,
            limits,
            |passage, state, requests, limits| {
                execute_mir_passage(mir, passage, state, requests, limits.includes)
            },
        )
    }

    /// 在现有导航事务内，通过注入的 Macro 控制器完成同步动态调用。
    ///
    /// Engine 只协调 VM 暂停和事务；Definition、生命周期与 Binding 仍由回调所有。
    pub fn navigate_mir_chain_with_macros<'hir, 'source, MacroError>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        name: &str,
        limits: EngineExecutionLimits,
        mut execute_macro: impl for<'call> FnMut(
            &HirMacro<'call>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            usize,
        ) -> Result<RuntimeMacroExecution, MacroError>,
    ) -> Result<
        EngineNavigationChain<'hir, 'source>,
        EngineNavigationError<
            EngineRequestedExecutionError<EngineMirMacroExecutionError<MacroError>>,
        >,
    > {
        Self::navigate_chain_with_requests(
            state,
            story,
            name,
            limits,
            |passage, state, requests, limits| {
                execute_mir_passage_with_macros(
                    mir,
                    passage,
                    state,
                    requests,
                    limits.includes,
                    &mut execute_macro,
                )
            },
        )
    }
}

/// 在单个 Passage 内驱动 MIR，并逐个分派动态 Macro 直到 Halt 或导航请求。
fn execute_mir_passage_with_macros<'hir, 'source, MacroError>(
    mir: &BytecodeProgram,
    passage: &HirPassage<'source>,
    state: &mut State,
    requests: &mut StoryRuntimeRequests<'_, 'hir, 'source>,
    include_limit: usize,
    execute_macro: &mut impl for<'call> FnMut(
        &HirMacro<'call>,
        &mut State,
        &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        usize,
    ) -> Result<RuntimeMacroExecution, MacroError>,
) -> Result<BodyExecution, EngineMirMacroExecutionError<MacroError>> {
    let mir_passage: &BytecodePassage = mir.passage(passage.name).ok_or_else(|| {
        EngineMirMacroExecutionError::Mir(EngineMirExecutionError::MissingMirPassage(
            passage.name.to_owned(),
        ))
    })?;
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(mir_passage);
    let mut macro_includes_entered: usize = 0;

    loop {
        let step: MirStep = frame
            .step(mir, state)
            .map_err(EngineMirExecutionError::Vm)
            .map_err(EngineMirMacroExecutionError::Mir)?;
        let includes_entered: usize = frame
            .includes_entered()
            .saturating_add(macro_includes_entered);
        if includes_entered > include_limit {
            return Err(EngineMirMacroExecutionError::Mir(
                EngineMirExecutionError::IncludeLimitExceeded {
                    limit: include_limit,
                },
            ));
        }
        match step {
            MirStep::Running => {}
            MirStep::Halted => {
                return Ok(BodyExecution {
                    control: BodyControl::Continue,
                    output: frame.into_output(),
                });
            }
            MirStep::NavigationPending => {
                let target: String = frame
                    .navigation()
                    .expect("NavigationPending 必须保存 goto 目标")
                    .to_owned();
                requests
                    .goto(&target)
                    .map_err(EngineMirExecutionError::Story)
                    .map_err(EngineMirMacroExecutionError::Mir)?;
                return Ok(BodyExecution {
                    control: BodyControl::StopPassage,
                    output: frame.into_output(),
                });
            }
            MirStep::MacroPending => {
                let owned_call = frame
                    .pending_macro(mir)
                    .expect("MacroPending 必须保存原始 HIR 调用");
                let call: HirMacro<'_> = owned_call.as_hir();
                let remaining_includes: usize = include_limit - includes_entered;
                let completion: RuntimeMacroExecution =
                    execute_macro(&call, state, requests, remaining_includes)
                        .map_err(EngineMirMacroExecutionError::Macro)?;
                macro_includes_entered =
                    macro_includes_entered.saturating_add(completion.includes_entered);
                if frame
                    .includes_entered()
                    .saturating_add(macro_includes_entered)
                    > include_limit
                {
                    return Err(EngineMirMacroExecutionError::Mir(
                        EngineMirExecutionError::IncludeLimitExceeded {
                            limit: include_limit,
                        },
                    ));
                }
                let execution: BodyExecution = completion.execution;
                let control: BodyControl = execution.control;
                frame
                    .complete_macro(mir, execution.output)
                    .map_err(EngineMirExecutionError::Vm)
                    .map_err(EngineMirMacroExecutionError::Mir)?;
                match control {
                    BodyControl::Continue => {}
                    BodyControl::StopPassage => {
                        return Ok(BodyExecution {
                            control,
                            output: frame.into_output(),
                        });
                    }
                    control => {
                        return Err(EngineMirMacroExecutionError::UnexpectedMacroControl(
                            control,
                        ));
                    }
                }
            }
        }
    }
}

/// 未安装 Macro 控制器时执行单个 Passage；遇到动态 Macro 即报错。
fn execute_mir_passage<'hir, 'source>(
    mir: &BytecodeProgram,
    passage: &HirPassage<'source>,
    state: &mut State,
    requests: &mut StoryRuntimeRequests<'_, 'hir, 'source>,
    include_limit: usize,
) -> Result<BodyExecution, EngineMirExecutionError> {
    execute_mir_passage_with_macros(
        mir,
        passage,
        state,
        requests,
        include_limit,
        &mut |_call, _state, _requests, _remaining_includes| {
            Err(EngineMirExecutionError::MacroPending)
        },
    )
    .map_err(|error| match error {
        EngineMirMacroExecutionError::Mir(error) | EngineMirMacroExecutionError::Macro(error) => {
            error
        }
        EngineMirMacroExecutionError::UnexpectedMacroControl(_) => {
            unreachable!("未配置 Macro 控制器时不可能返回控制信号")
        }
    })
}

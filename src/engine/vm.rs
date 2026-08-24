//! Bytecode VM 到现有 Engine 导航事务的适配。

use crate::{
    bytecode::{BytecodePassage, BytecodeProgram},
    expression::value::Value,
    hir::{HirMacro, HirPassage},
    i18n::I18nRuntimeLanguage,
    macro_runtime::MacroStoryAccess,
    presentation::PresentationOutput,
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

#[derive(Debug, PartialEq)]
pub enum EngineMirBeginExecutionError<LifecycleError> {
    MissingMirPassage(String),
    Lifecycle(LifecycleError),
}

pub enum EngineMirBeginError<'hir, 'source, LifecycleError> {
    Preparation(
        EngineNavigationError<
            EngineRequestedExecutionError<EngineMirBeginExecutionError<LifecycleError>>,
        >,
    ),
    Continue(Box<EngineMirVmResumeError<'hir, 'source>>),
}

#[derive(Clone, Copy, Debug)]
pub struct EngineMirBeginRequest<'params> {
    pub name: &'params str,
    pub params: &'params Value,
    pub identity: RuntimeExecutionIdentity,
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
#[derive(Debug, PartialEq)]
pub enum EngineMirExecutionError {
    MissingMirPassage(String),
    Vm(MirExecutionError),
    Story(StoryRuntimeRequestError),
    IncludeLimitExceeded { limit: usize },
    MacroPending,
}

/// Engine 接通 Macro 控制器后能够区分的 MIR、Macro 与边界错误。
#[derive(Debug, PartialEq)]
pub enum EngineMirMacroExecutionError<MacroError> {
    Mir(EngineMirExecutionError),
    Macro(MacroError),
    UnexpectedMacroControl(BodyControl),
}

impl Engine {
    /// 进入首个 Passage、执行 Init／Start，并驱动 LIR 到第一个稳定边界。
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
        let state_checkpoint: StateCheckpoint = state.checkpoint();
        let story_snapshot: StorySnapshot<'hir, 'source> = story.snapshot();
        Self::begin_mir_chain_from_checkpoint(
            state,
            story,
            mir,
            EngineMirBeginCheckpointRequest {
                request,
                state_checkpoint,
                story_snapshot,
            },
            lifecycle,
        )
    }

    /// 使用调用方在启动准备前取得的检查点进入首个 Passage。
    pub(crate) fn begin_mir_chain_from_checkpoint<'hir, 'source, LifecycleError>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        checkpoint: EngineMirBeginCheckpointRequest<'hir, 'source, '_>,
        mut lifecycle: impl FnMut(
            PassageLifecyclePhase,
            PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), LifecycleError>,
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
        let requests = StoryRuntimeRequests::new(story).into_pending();
        let progress = EngineMirProgress::new(
            current,
            vec![current],
            PresentationOutput::default(),
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

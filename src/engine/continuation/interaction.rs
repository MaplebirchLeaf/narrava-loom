//! Interaction 正文暂停、恢复、回滚以及进入目标 Passage 的事务。
//!
//! 本模块拥有点击 Interaction 后产生的独立 Macro 正文事务；失败时必须恢复点击前的
//! State、Story 与待处理 Story 请求，不能污染外层 Passage 事务。

use super::*;

/// 延迟 Interaction 正文异步暂停后的完整 Engine 所有权。
pub struct EngineMacroInteractionContinuation<'hir, 'source, Pending> {
    runtime: RuntimeMacroBodyContinuation<Pending>,
    mir: MirMacroBody<'hir, 'source>,
    interaction: MacroInteraction<'hir, 'source>,
    state_checkpoint: StateCheckpoint,
    story_snapshot: StorySnapshot<'hir, 'source>,
    requests: StoryRuntimePending<'hir, 'source>,
    params: Value,
    limits: EngineExecutionLimits,
}

/// Interaction 异步事务跨恢复阶段共用的检查点与入口约束。
pub struct EngineMacroInteractionTransaction<'hir, 'source> {
    state_checkpoint: StateCheckpoint,
    story_snapshot: StorySnapshot<'hir, 'source>,
    requests: StoryRuntimePending<'hir, 'source>,
    params: Value,
    limits: EngineExecutionLimits,
}

impl<'hir, 'source> EngineMacroInteractionTransaction<'hir, 'source> {
    pub fn new(
        state_checkpoint: StateCheckpoint,
        story_snapshot: StorySnapshot<'hir, 'source>,
        requests: StoryRuntimePending<'hir, 'source>,
        params: &Value,
        limits: EngineExecutionLimits,
    ) -> Self {
        Self {
            state_checkpoint,
            story_snapshot,
            requests,
            params: params.detached_clone(),
            limits,
        }
    }
}

/// 取消异步 Interaction 后返还给 Macro／Binding 所有者的数据。
pub struct EngineMacroInteractionCancelled<'hir, 'source, Pending> {
    pub pending: Pending,
    pub interaction: MacroInteraction<'hir, 'source>,
    /// State 总能恢复；Story 不属于同一编译结果时显式报告恢复失败。
    pub story_error: Option<StorySnapshotError>,
}

pub enum EngineMacroInteractionResume<'hir, 'source, Pending> {
    Pending(EngineMacroInteractionContinuation<'hir, 'source, Pending>),
    Complete(EngineMacroInteractionResumed<'hir, 'source>),
}

pub struct EngineMacroInteractionResumed<'hir, 'source> {
    pub runtime: RuntimeMacroBodyResumed,
    pub mir: MirMacroBody<'hir, 'source>,
    pub interaction: MacroInteraction<'hir, 'source>,
    pub state_checkpoint: StateCheckpoint,
    pub story_snapshot: StorySnapshot<'hir, 'source>,
    pub requests: StoryRuntimePending<'hir, 'source>,
    pub params: Value,
    pub limits: EngineExecutionLimits,
}

pub enum EngineMacroInteractionBoundary<'hir, 'source> {
    Halted(EngineMacroInteractionResumed<'hir, 'source>),
    MacroPending(EngineMacroInteractionResumed<'hir, 'source>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EngineMacroInteractionDriveFailureKind {
    Vm(MirExecutionError),
    UnexpectedControl(BodyControl),
    UnexpectedNavigation,
    UnconsumedIncludes { count: usize },
    UnexpectedGoto,
}

pub struct EngineMacroInteractionDriveError<'hir, 'source> {
    pub kind: EngineMacroInteractionDriveFailureKind,
    pub interaction: MacroInteraction<'hir, 'source>,
    pub story_error: Option<StorySnapshotError>,
}

pub enum EngineMacroInteractionDispatch<'hir, 'source, Pending> {
    Pending(EngineMacroInteractionContinuation<'hir, 'source, Pending>),
    Complete(EngineMacroInteractionResumed<'hir, 'source>),
}

pub enum EngineMacroInteractionDispatchFailureKind<Pending, DispatchError> {
    MissingMacro,
    StoryMismatch,
    Callback(DispatchError),
    InvalidSuspension(RuntimeMacroContinuationError<Pending>),
    Vm(MirExecutionError),
}

pub struct EngineMacroInteractionDispatchError<'hir, 'source, Pending, DispatchError> {
    pub kind: EngineMacroInteractionDispatchFailureKind<Pending, DispatchError>,
    pub interaction: MacroInteraction<'hir, 'source>,
    pub story_error: Option<StorySnapshotError>,
}

pub enum EngineMacroInteractionTargetError<'hir, 'source, LifecycleError> {
    NotHalted(Box<EngineMacroInteractionBoundary<'hir, 'source>>),
    Begin {
        error: Box<EngineMirBeginError<'hir, 'source, LifecycleError>>,
        interaction: MacroInteraction<'hir, 'source>,
    },
}

pub enum EngineMacroInteractionResumeError<'hir, 'source, Pending, ResumeError> {
    Story(Box<EngineMacroInteractionContinuation<'hir, 'source, Pending>>),
    Runtime {
        error: RuntimeMacroContinuationResumeError<ResumeError, Pending>,
        interaction: MacroInteraction<'hir, 'source>,
        story_error: Option<StorySnapshotError>,
    },
}

impl<'hir, 'source, Pending> EngineMacroInteractionContinuation<'hir, 'source, Pending> {
    pub fn new(
        runtime: RuntimeMacroBodyContinuation<Pending>,
        mir: MirMacroBody<'hir, 'source>,
        interaction: MacroInteraction<'hir, 'source>,
        transaction: EngineMacroInteractionTransaction<'hir, 'source>,
    ) -> Self {
        let EngineMacroInteractionTransaction {
            state_checkpoint,
            story_snapshot,
            requests,
            params,
            limits,
        } = transaction;
        Self {
            runtime,
            mir,
            interaction,
            state_checkpoint,
            story_snapshot,
            requests,
            params,
            limits,
        }
    }

    pub fn runtime(&self) -> &RuntimeMacroBodyContinuation<Pending> {
        &self.runtime
    }

    pub fn mir(&self) -> &MirMacroBody<'hir, 'source> {
        &self.mir
    }

    pub fn interaction(&self) -> &MacroInteraction<'hir, 'source> {
        &self.interaction
    }

    /// 取消等待并恢复点击 Interaction 之前的领域状态与动作所有权。
    pub fn rollback(
        self,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
    ) -> EngineMacroInteractionCancelled<'hir, 'source, Pending> {
        let (_frame, suspension) = self.runtime.into_parts();
        state.restore_checkpoint(self.state_checkpoint);
        let story_error: Option<StorySnapshotError> = story.restore(self.story_snapshot).err();
        EngineMacroInteractionCancelled {
            pending: suspension.handle,
            interaction: self.interaction,
            story_error,
        }
    }

    /// 恢复当前异步 Macro；完成时只推进该指令，正文后续驱动由下一边界负责。
    pub fn resume<ResumeError>(
        self,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        resume: impl FnOnce(
            Pending,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            &mut MacroLocalScopes<Value>,
        )
            -> Result<MacroHandlerOutcome<RuntimeMacroExecution, Pending>, ResumeError>,
    ) -> Result<
        EngineMacroInteractionResume<'hir, 'source, Pending>,
        Box<EngineMacroInteractionResumeError<'hir, 'source, Pending, ResumeError>>,
    > {
        let Self {
            runtime,
            mir,
            interaction,
            state_checkpoint,
            story_snapshot,
            requests,
            params,
            limits,
        } = self;
        let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
            match StoryRuntimeRequests::from_pending(story, requests) {
                Ok(requests) => requests,
                Err(error) => {
                    return Err(Box::new(EngineMacroInteractionResumeError::Story(
                        Box::new(Self {
                            runtime,
                            mir,
                            interaction,
                            state_checkpoint,
                            story_snapshot,
                            requests: error.pending,
                            params,
                            limits,
                        }),
                    )));
                }
            };
        let outcome = runtime.resume(&mir, |pending, locals| {
            resume(pending, state, &mut requests, locals)
        });
        let requests: StoryRuntimePending<'hir, 'source> = requests.into_pending();
        match outcome {
            Ok(RuntimeMacroBodyContinuationResume::Pending(runtime)) => {
                Ok(EngineMacroInteractionResume::Pending(Self {
                    runtime,
                    mir,
                    interaction,
                    state_checkpoint,
                    story_snapshot,
                    requests,
                    params,
                    limits,
                }))
            }
            Ok(RuntimeMacroBodyContinuationResume::Complete(runtime)) => Ok(
                EngineMacroInteractionResume::Complete(EngineMacroInteractionResumed {
                    runtime,
                    mir,
                    interaction,
                    state_checkpoint,
                    story_snapshot,
                    requests,
                    params,
                    limits,
                }),
            ),
            Err(error) => {
                state.restore_checkpoint(state_checkpoint);
                let story_error: Option<StorySnapshotError> = story.restore(story_snapshot).err();
                Err(Box::new(EngineMacroInteractionResumeError::Runtime {
                    error,
                    interaction,
                    story_error,
                }))
            }
        }
    }
}

impl<'hir, 'source> EngineMacroInteractionResumed<'hir, 'source> {
    /// 驱动异步 Macro 之后的独立正文，直到 Halt 或下一动态 Macro。
    pub fn continue_vm(
        self,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
    ) -> Result<
        EngineMacroInteractionBoundary<'hir, 'source>,
        Box<EngineMacroInteractionDriveError<'hir, 'source>>,
    > {
        let Self {
            runtime,
            mir,
            interaction,
            state_checkpoint,
            story_snapshot,
            requests,
            params,
            limits,
        } = self;
        let RuntimeMacroBodyResumed {
            identity,
            mut frame,
            control,
            includes_entered,
            mut scopes,
        } = runtime;
        if !matches!(control, BodyControl::Continue | BodyControl::ExitScope) {
            return Err(rollback_macro_interaction_drive(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                interaction,
                EngineMacroInteractionDriveFailureKind::UnexpectedControl(control),
            ));
        }
        if includes_entered != 0 {
            return Err(rollback_macro_interaction_drive(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                interaction,
                EngineMacroInteractionDriveFailureKind::UnconsumedIncludes {
                    count: includes_entered,
                },
            ));
        }
        let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
            match StoryRuntimeRequests::from_pending(story, requests) {
                Ok(requests) => requests,
                Err(_) => {
                    return Err(rollback_macro_interaction_drive(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        interaction,
                        EngineMacroInteractionDriveFailureKind::UnexpectedGoto,
                    ));
                }
            };
        let bytecode: BytecodeMacroBody = BytecodeMacroBody::compile(&mir);
        loop {
            let step: Result<MirStep, MirExecutionError> = {
                let mut context: MacroLogicContext<'_, StoryRuntimeRequests<'_, 'hir, 'source>> =
                    MacroLogicContext::new(state, &mut requests, &mut scopes);
                frame.step_macro(&bytecode, &mut context)
            };
            match step {
                Ok(MirStep::Running) => {}
                Ok(MirStep::Halted) | Ok(MirStep::MacroPending) => {
                    let is_macro: bool = matches!(step, Ok(MirStep::MacroPending));
                    let include_count: usize = requests.pending_include_count();
                    let has_goto: bool = requests.take_goto().is_some();
                    if include_count != 0 || has_goto {
                        let kind = if include_count != 0 {
                            EngineMacroInteractionDriveFailureKind::UnconsumedIncludes {
                                count: include_count,
                            }
                        } else {
                            EngineMacroInteractionDriveFailureKind::UnexpectedGoto
                        };
                        drop(requests);
                        return Err(rollback_macro_interaction_drive(
                            state,
                            story,
                            state_checkpoint,
                            story_snapshot,
                            interaction,
                            kind,
                        ));
                    }
                    let resumed = Self {
                        runtime: RuntimeMacroBodyResumed {
                            identity,
                            frame,
                            control: BodyControl::Continue,
                            includes_entered: 0,
                            scopes,
                        },
                        mir,
                        interaction,
                        state_checkpoint,
                        story_snapshot,
                        requests: requests.into_pending(),
                        params,
                        limits,
                    };
                    return Ok(if is_macro {
                        EngineMacroInteractionBoundary::MacroPending(resumed)
                    } else {
                        EngineMacroInteractionBoundary::Halted(resumed)
                    });
                }
                Ok(MirStep::NavigationPending) => {
                    drop(requests);
                    return Err(rollback_macro_interaction_drive(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        interaction,
                        EngineMacroInteractionDriveFailureKind::UnexpectedNavigation,
                    ));
                }
                Err(error) => {
                    drop(requests);
                    return Err(rollback_macro_interaction_drive(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        interaction,
                        EngineMacroInteractionDriveFailureKind::Vm(error),
                    ));
                }
            }
        }
    }

    /// 分派独立正文当前位置的动态 Macro。
    pub fn dispatch_macro<Pending, DispatchError>(
        self,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        dispatch: impl FnOnce(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
    ) -> Result<
        EngineMacroInteractionDispatch<'hir, 'source, Pending>,
        Box<EngineMacroInteractionDispatchError<'hir, 'source, Pending, DispatchError>>,
    > {
        let Self {
            runtime,
            mir,
            interaction,
            state_checkpoint,
            story_snapshot,
            requests,
            params,
            limits,
        } = self;
        let RuntimeMacroBodyResumed {
            identity,
            mut frame,
            control: _,
            includes_entered,
            scopes,
        } = runtime;
        let bytecode: BytecodeMacroBody = BytecodeMacroBody::compile(&mir);
        let Some(call) = frame.pending_macro_body(&bytecode) else {
            return Err(rollback_macro_interaction_dispatch(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                interaction,
                EngineMacroInteractionDispatchFailureKind::MissingMacro,
            ));
        };
        let capture_names: Vec<&str> = frame
            .pending_macro_body_captures(&bytecode)
            .expect("MacroPending 正文必须能读取捕获名称");
        let captures: CapturedMacroLocals<Value> = scopes.capture(&capture_names);
        let location: crate::mir::MirExecutionPosition = frame.location();
        let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
            match StoryRuntimeRequests::from_pending(story, requests) {
                Ok(requests) => requests,
                Err(_) => {
                    return Err(rollback_macro_interaction_dispatch(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        interaction,
                        EngineMacroInteractionDispatchFailureKind::StoryMismatch,
                    ));
                }
            };
        let outcome = dispatch(
            EngineMirMacroInvocation {
                call,
                identity,
                location,
                captures,
            },
            state,
            &mut requests,
            scopes,
        );
        let requests: StoryRuntimePending<'hir, 'source> = requests.into_pending();
        match outcome {
            Err(failure) => Err(rollback_macro_interaction_dispatch(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                interaction,
                EngineMacroInteractionDispatchFailureKind::Callback(failure.error),
            )),
            Ok(MacroResumeOutcome::Pending(suspension)) => {
                match RuntimeMacroBodyContinuation::new(identity, frame, suspension, &mir) {
                    Ok(runtime) => Ok(EngineMacroInteractionDispatch::Pending(
                        EngineMacroInteractionContinuation::new(
                            runtime,
                            mir,
                            interaction,
                            EngineMacroInteractionTransaction {
                                state_checkpoint,
                                story_snapshot,
                                requests,
                                params,
                                limits,
                            },
                        ),
                    )),
                    Err(error) => Err(rollback_macro_interaction_dispatch(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        interaction,
                        EngineMacroInteractionDispatchFailureKind::InvalidSuspension(error),
                    )),
                }
            }
            Ok(MacroResumeOutcome::Complete { output, scopes }) => {
                let RuntimeMacroExecution {
                    execution,
                    includes_entered: completed_includes,
                } = output;
                if let Err(error) = frame.complete_macro_body(&bytecode, execution.output) {
                    return Err(rollback_macro_interaction_dispatch(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        interaction,
                        EngineMacroInteractionDispatchFailureKind::Vm(error),
                    ));
                }
                Ok(EngineMacroInteractionDispatch::Complete(Self {
                    runtime: RuntimeMacroBodyResumed {
                        identity,
                        frame,
                        control: execution.control,
                        includes_entered: includes_entered + completed_includes,
                        scopes,
                    },
                    mir,
                    interaction,
                    state_checkpoint,
                    story_snapshot,
                    requests,
                    params,
                    limits,
                }))
            }
        }
    }
}

fn rollback_macro_interaction_drive<'hir, 'source>(
    state: &mut State,
    story: &mut Story<'hir, 'source>,
    state_checkpoint: StateCheckpoint,
    story_snapshot: StorySnapshot<'hir, 'source>,
    interaction: MacroInteraction<'hir, 'source>,
    kind: EngineMacroInteractionDriveFailureKind,
) -> Box<EngineMacroInteractionDriveError<'hir, 'source>> {
    state.restore_checkpoint(state_checkpoint);
    let story_error: Option<StorySnapshotError> = story.restore(story_snapshot).err();
    Box::new(EngineMacroInteractionDriveError {
        kind,
        interaction,
        story_error,
    })
}

fn rollback_macro_interaction_dispatch<'hir, 'source, Pending, DispatchError>(
    state: &mut State,
    story: &mut Story<'hir, 'source>,
    state_checkpoint: StateCheckpoint,
    story_snapshot: StorySnapshot<'hir, 'source>,
    interaction: MacroInteraction<'hir, 'source>,
    kind: EngineMacroInteractionDispatchFailureKind<Pending, DispatchError>,
) -> Box<EngineMacroInteractionDispatchError<'hir, 'source, Pending, DispatchError>> {
    state.restore_checkpoint(state_checkpoint);
    let story_error: Option<StorySnapshotError> = story.restore(story_snapshot).err();
    Box::new(EngineMacroInteractionDispatchError {
        kind,
        interaction,
        story_error,
    })
}

impl Engine {
    /// 消费已 Halt 的 Interaction 正文，并用同一事务检查点进入目标 Passage。
    pub fn begin_macro_interaction_target<'hir, 'source, LifecycleError>(
        boundary: EngineMacroInteractionBoundary<'hir, 'source>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        story_mir: &BytecodeProgram,
        lifecycle: impl FnMut(
            PassageLifecyclePhase,
            PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), LifecycleError>,
    ) -> Result<
        EngineMirVmResume<'hir, 'source>,
        Box<EngineMacroInteractionTargetError<'hir, 'source, LifecycleError>>,
    > {
        let EngineMacroInteractionBoundary::Halted(halted) = boundary else {
            return Err(Box::new(EngineMacroInteractionTargetError::NotHalted(
                Box::new(boundary),
            )));
        };
        let EngineMacroInteractionResumed {
            runtime,
            interaction,
            state_checkpoint,
            story_snapshot,
            params,
            limits,
            ..
        } = halted;
        let identity: RuntimeExecutionIdentity = runtime.identity;
        let target: String = interaction.target().to_owned();
        Engine::begin_mir_chain_from_checkpoint(
            state,
            story,
            story_mir,
            EngineMirBeginCheckpointRequest {
                request: EngineMirBeginRequest {
                    name: &target,
                    params: &params,
                    identity,
                    limits,
                    language: None,
                },
                state_checkpoint,
                story_snapshot,
            },
            lifecycle,
        )
        .map_err(|error| {
            Box::new(EngineMacroInteractionTargetError::Begin {
                error: Box::new(error),
                interaction,
            })
        })
    }
}

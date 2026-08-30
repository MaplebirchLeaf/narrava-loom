//! 异步 continuation 的取消、恢复与恢复后边界校验。

use super::*;

impl HostApi {
    /// 取消 Token 对应的异步事务并恢复暂停前的 State 与 Story。
    ///
    /// 返回值只包含 Binding 原先提供的平台 Pending 值；continuation、VM frame
    /// 与 Macro 局部域均在 Core 内消费。
    pub fn cancel_pending<'hir, 'source, Pending>(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        token: HostExecutionToken,
    ) -> Result<HostCancelled<Pending>, Box<HostCancelError<Pending>>> {
        let Some(continuation) = pending.take(token) else {
            return Err(Box::new(HostCancelError {
                diagnostic: host_error(
                    "host.pending.unknown_execution",
                    "Host 没有保存该执行令牌对应的异步事务",
                ),
                pending: None,
            }));
        };
        match continuation.rollback(state, story) {
            Ok(suspension) => Ok(HostCancelled {
                execution: token,
                pending: suspension.handle,
            }),
            Err(error) => Err(Box::new(HostCancelError {
                diagnostic: host_error(
                    "engine.rollback.failed",
                    "异步事务取消后，Story 检查点无法恢复",
                ),
                pending: Some(error.suspension.handle),
            })),
        }
    }

    /// 取消异步 Interaction，恢复事务并把原动作放回同一 ID。
    pub fn cancel_macro_interaction_pending<'hir, 'source, Pending>(
        pending: &mut HostPendingExecutions<HostMacroInteractionPending<'hir, 'source, Pending>>,
        interactions: &mut MacroInteractions<'hir, 'source>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        token: HostExecutionToken,
    ) -> Result<HostCancelled<Pending>, Box<HostCancelError<Pending>>> {
        let Some(owned) = pending.take(token) else {
            return Err(Box::new(HostCancelError {
                diagnostic: host_error(
                    "host.pending.unknown_interaction_execution",
                    "Host 没有保存该执行令牌对应的异步 Interaction",
                ),
                pending: None,
            }));
        };
        let HostMacroInteractionPending {
            interaction,
            continuation,
        } = owned;
        let cancelled = continuation.rollback(state, story);
        if interactions
            .add(interaction, cancelled.interaction)
            .is_err()
        {
            return Err(Box::new(HostCancelError {
                diagnostic: host_error(
                    "host.pending.interaction_restore_conflict",
                    "异步 Interaction 取消后，原 ID 已被其他动作占用",
                ),
                pending: Some(cancelled.pending),
            }));
        }
        if cancelled.story_error.is_some() {
            return Err(Box::new(HostCancelError {
                diagnostic: host_error(
                    "engine.rollback.failed",
                    "异步 Interaction 已取消，但 Story 检查点无法恢复",
                ),
                pending: Some(cancelled.pending),
            }));
        }
        Ok(HostCancelled {
            execution: token,
            pending: cancelled.pending,
        })
    }

    /// 恢复异步 Interaction Handler；再次 Pending 会原子归还 Host 容器。
    pub fn resume_macro_interaction_pending<'hir, 'source, Pending, ResumeError>(
        pending: &mut HostPendingExecutions<HostMacroInteractionPending<'hir, 'source, Pending>>,
        interactions: &mut MacroInteractions<'hir, 'source>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        token: HostExecutionToken,
        resume: impl FnOnce(
            Pending,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            &mut MacroLocalScopes<Value>,
        )
            -> Result<MacroHandlerOutcome<RuntimeMacroExecution, Pending>, ResumeError>,
    ) -> Result<HostMacroInteractionResume<'hir, 'source>, Diagnostic> {
        let Some(owned) = pending.take(token) else {
            return Err(host_error(
                "host.pending.unknown_interaction_execution",
                "Host 没有保存该执行令牌对应的异步 Interaction",
            ));
        };
        let HostMacroInteractionPending {
            interaction,
            continuation,
        } = owned;
        match continuation.resume(state, story, resume) {
            Ok(EngineMacroInteractionResume::Pending(continuation)) => {
                let owned = HostMacroInteractionPending::new(interaction, continuation);
                if pending.add(token, owned).is_err() {
                    panic!("取出后的同一 Interaction Token 必须可以重新保存");
                }
                Ok(HostMacroInteractionResume::Pending { execution: token })
            }
            Ok(EngineMacroInteractionResume::Complete(transaction)) => Ok(
                HostMacroInteractionResume::Continue(Box::new(HostMacroInteractionResumed {
                    execution: token,
                    interaction,
                    transaction,
                })),
            ),
            Err(error) => match *error {
                EngineMacroInteractionResumeError::Story(continuation) => {
                    let owned = HostMacroInteractionPending::new(interaction, *continuation);
                    if pending.add(token, owned).is_err() {
                        panic!("Story 附着失败后必须保留原 Interaction continuation");
                    }
                    Err(host_error(
                        "host.pending.story_mismatch",
                        "异步 Interaction 不能附着到当前 Story",
                    ))
                }
                EngineMacroInteractionResumeError::Runtime {
                    interaction: action,
                    story_error,
                    ..
                } => {
                    let restore_failed: bool = interactions.add(interaction, action).is_err();
                    Err(host_error(
                        if story_error.is_some() || restore_failed {
                            "engine.rollback.failed"
                        } else {
                            "host.pending.interaction_resume_failed"
                        },
                        "异步 Interaction 恢复失败，事务已回滚",
                    ))
                }
            },
        }
    }

    /// 驱动已经恢复的 Interaction 正文，并在正文结束后进入其目标 Passage。
    ///
    /// Binding 只处理最终更新或新的异步等待，不需要识别独立 Macro 正文的 VM 边界。
    pub fn drive_macro_interaction<'hir, 'source, Pending, DispatchError, Lifecycle, Dispatch>(
        resumed: HostMacroInteractionResumed<'hir, 'source>,
        context: HostMacroInteractionDriveContext<'_, 'hir, 'source, Pending>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        mut lifecycle: Lifecycle,
        mut dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
    {
        let HostMacroInteractionResumed {
            execution,
            interaction,
            transaction,
        } = resumed;
        let HostMacroInteractionDriveContext {
            interaction_pending,
            passage_pending,
            interactions,
        } = context;
        let mut transaction: EngineMacroInteractionResumed<'hir, 'source> = transaction;

        loop {
            let boundary: EngineMacroInteractionBoundary<'hir, 'source> =
                match transaction.continue_vm(state, story) {
                    Ok(boundary) => boundary,
                    Err(error) => {
                        let restore_failed: bool =
                            interactions.add(interaction, error.interaction).is_err();
                        return Err(Box::new(HostDriveError {
                            diagnostic: host_error(
                                if error.story_error.is_some() || restore_failed {
                                    "engine.rollback.failed"
                                } else {
                                    "host.interaction.drive_failed"
                                },
                                "Interaction 正文执行失败，事务已回滚",
                            ),
                            pending: None,
                        }));
                    }
                };

            match boundary {
                EngineMacroInteractionBoundary::MacroPending(pending_macro) => {
                    match pending_macro.dispatch_macro(state, story, &mut dispatch) {
                        Ok(EngineMacroInteractionDispatch::Pending(continuation)) => {
                            let owned = HostMacroInteractionPending::new(interaction, continuation);
                            if interaction_pending.add(execution, owned).is_err() {
                                panic!("当前 Interaction Token 必须可以保存新的 continuation");
                            }
                            return Ok(HostDriveResult::Pending { execution });
                        }
                        Ok(EngineMacroInteractionDispatch::Complete(next)) => {
                            transaction = next;
                        }
                        Err(error) => {
                            let EngineMacroInteractionDispatchError {
                                interaction: action,
                                story_error,
                                ..
                            } = *error;
                            let restore_failed: bool =
                                interactions.add(interaction, action).is_err();
                            return Err(Box::new(HostDriveError {
                                diagnostic: host_error(
                                    if story_error.is_some() || restore_failed {
                                        "engine.rollback.failed"
                                    } else {
                                        "host.interaction.dispatch_failed"
                                    },
                                    "Interaction 正文中的 Macro 分派失败，事务已回滚",
                                ),
                                pending: None,
                            }));
                        }
                    }
                }
                halted @ EngineMacroInteractionBoundary::Halted(_) => {
                    let boundary: EngineMirVmResume<'hir, 'source> =
                        match Engine::begin_macro_interaction_target(
                            halted,
                            state,
                            story,
                            mir,
                            &mut lifecycle,
                        ) {
                            Ok(boundary) => boundary,
                            Err(error) => match *error {
                                EngineMacroInteractionTargetError::Begin {
                                    error,
                                    interaction: action,
                                } => {
                                    let diagnostic: Diagnostic =
                                        mir_begin_diagnostic(*error, state, story);
                                    let _restored: Result<(), _> =
                                        interactions.add(interaction, action);
                                    return Err(Box::new(HostDriveError {
                                        diagnostic,
                                        pending: None,
                                    }));
                                }
                                EngineMacroInteractionTargetError::NotHalted(_) => {
                                    unreachable!("Halted 分支必须可以进入 Interaction 目标")
                                }
                            },
                        };
                    return Self::drive_stable_with_reaction(
                        HostStable {
                            execution,
                            boundary,
                        },
                        passage_pending,
                        state,
                        story,
                        mir,
                        lifecycle,
                        no_passage_reaction,
                        dispatch,
                    );
                }
            }
        }
    }

    /// 取出并恢复当前异步 Handler；再次 Pending 会自动归还容器。
    pub fn resume_pending<'hir, 'source, Pending, ResumeError>(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        token: HostExecutionToken,
        resume: impl FnOnce(
            Pending,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            &mut MacroLocalScopes<Value>,
        )
            -> Result<MacroHandlerOutcome<RuntimeMacroExecution, Pending>, ResumeError>,
    ) -> Result<HostResumeOutcome<'hir, 'source>, Diagnostic> {
        let Some(continuation) = pending.take(token) else {
            return Err(host_error(
                "host.pending.unknown_execution",
                "Host 没有保存该执行令牌对应的异步事务",
            ));
        };
        match continuation.resume_macro(mir, state, story, resume) {
            Ok(EngineMirContinuationResume::Pending(continuation)) => {
                if pending.add(token, continuation).is_err() {
                    panic!("取出后的同一 Token 必须可以重新保存");
                }
                Ok(HostResumeOutcome::Pending { execution: token })
            }
            Ok(EngineMirContinuationResume::Complete(transaction)) => {
                Ok(HostResumeOutcome::Continue(Box::new(HostResumed {
                    execution: token,
                    transaction,
                })))
            }
            Err(EngineMirContinuationResumeError::Story(continuation)) => {
                if pending.add(token, *continuation).is_err() {
                    panic!("Story 重新附着失败后必须保留原 continuation");
                }
                Err(host_error(
                    "host.pending.story_mismatch",
                    "异步事务不能附着到当前 Story",
                ))
            }
            Err(EngineMirContinuationResumeError::Runtime(failure)) => {
                let rollback_failed: bool = failure.rollback(state, story).is_err();
                Err(host_error(
                    if rollback_failed {
                        "engine.rollback.failed"
                    } else {
                        "host.pending.resume_failed"
                    },
                    if rollback_failed {
                        "异步恢复失败，且 Story 检查点无法恢复"
                    } else {
                        "异步 Handler 或 VM 恢复失败，事务已回滚"
                    },
                ))
            }
        }
    }

    /// 把已完成当前 Handler 的事务继续驱动到下一个稳定边界。
    pub fn continue_resumed<'hir, 'source>(
        resumed: HostResumed<'hir, 'source>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
    ) -> Result<HostStable<'hir, 'source>, Diagnostic> {
        let HostResumed {
            execution,
            transaction,
        } = resumed;
        match transaction.continue_vm(mir, state, story) {
            Ok(boundary) => Ok(HostStable {
                execution,
                boundary,
            }),
            Err(error) => {
                let (transaction, diagnostic) = mir_resume_failure(error);
                let rollback_failed: bool = transaction.rollback(state, story).is_err();
                if rollback_failed {
                    Err(host_error(
                        "engine.rollback.failed",
                        "异步事务继续失败，且 Story 检查点无法恢复",
                    ))
                } else {
                    Err(diagnostic)
                }
            }
        }
    }
}

//! Host 稳定边界的提交、导航继续、Macro 分派与自动驱动。

use super::*;

impl HostApi {
    pub fn drive_stable<'hir, 'source, Pending, DispatchError, Lifecycle, Dispatch>(
        stable: HostStable<'hir, 'source>,
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        lifecycle: Lifecycle,
        dispatch: Dispatch,
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
        Self::drive_stable_with_reaction(
            stable,
            pending,
            state,
            story,
            mir,
            lifecycle,
            no_passage_reaction,
            dispatch,
        )
    }

    /// 提交 Halted 边界并生成 Renderer 可消费的最终更新。
    pub fn commit_halted<'hir, 'source>(
        stable: HostStable<'hir, 'source>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        lifecycle: impl FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
    ) -> Result<HostUpdate, Box<HostCommitError<'hir, 'source>>> {
        let HostStable {
            execution,
            boundary,
        } = stable;
        match boundary.commit_halted(state, story, lifecycle) {
            Ok(committed) => {
                let current = committed
                    .navigation
                    .entries
                    .last()
                    .expect("成功提交必须包含当前 Passage");
                Ok(HostUpdate::new(
                    current.passage().name,
                    committed.navigation.output,
                ))
            }
            Err(EngineMirCommitError::NotHalted(boundary)) => {
                Err(Box::new(HostCommitError::NotHalted(Box::new(HostStable {
                    execution,
                    boundary: *boundary,
                }))))
            }
            Err(EngineMirCommitError::Failed(failure)) => {
                let diagnostic: Diagnostic = match failure.kind {
                    EngineMirCommitFailureKind::Lifecycle { error, .. } => error,
                    EngineMirCommitFailureKind::StoryMismatch => host_error(
                        "host.pending.story_mismatch",
                        "异步提交不能附着到当前 Story",
                    ),
                    EngineMirCommitFailureKind::UnconsumedIncludes { count } => host_error(
                        "engine.include.unconsumed_requests",
                        &format!("异步提交仍有未消费的 include 请求：{count}"),
                    ),
                    EngineMirCommitFailureKind::UnexpectedGoto => host_error(
                        "engine.goto.unexpected_request",
                        "Halted 边界仍留有未确认的 goto 请求",
                    ),
                };
                if failure.story_rollback.is_some() {
                    Err(Box::new(HostCommitError::Failed(host_error(
                        "engine.rollback.failed",
                        "异步提交失败，且 Story 检查点无法恢复",
                    ))))
                } else {
                    Err(Box::new(HostCommitError::Failed(diagnostic)))
                }
            }
        }
    }

    /// 消费 NavigationPending／PassageStopped，并进入下一 Passage。
    pub fn continue_navigation<'hir, 'source>(
        stable: HostStable<'hir, 'source>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        lifecycle: impl FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        reaction: impl FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, Diagnostic>,
    ) -> Result<HostStable<'hir, 'source>, Box<HostNavigationError<'hir, 'source>>> {
        let HostStable {
            execution,
            boundary,
        } = stable;
        match boundary.continue_navigation(mir, state, story, lifecycle, reaction) {
            Ok(boundary) => Ok(HostStable {
                execution,
                boundary,
            }),
            Err(EngineMirNavigationResumeError::NotNavigation(boundary)) => Err(Box::new(
                HostNavigationError::NotNavigation(Box::new(HostStable {
                    execution,
                    boundary: *boundary,
                })),
            )),
            Err(EngineMirNavigationResumeError::Failed(failure)) => {
                let diagnostic: Diagnostic = match failure.kind {
                    EngineMirNavigationFailureKind::Lifecycle { error, .. } => error,
                    EngineMirNavigationFailureKind::PassageLimitExceeded { limit } => host_error(
                        "engine.execution.passage_limit_exceeded",
                        &format!("单次事务执行的 Passage 数量超过限制：{limit}"),
                    ),
                    EngineMirNavigationFailureKind::MissingMirPassage(name) => host_error(
                        "engine.mir.missing_passage",
                        &format!("MIR 中缺少 Passage：{name}"),
                    ),
                    EngineMirNavigationFailureKind::UnconsumedIncludes { count } => host_error(
                        "engine.include.unconsumed_requests",
                        &format!("导航前仍有未消费的 include 请求：{count}"),
                    ),
                    EngineMirNavigationFailureKind::MissingGoto => host_error(
                        "engine.goto.missing_request",
                        "导航边界没有对应的 goto 请求",
                    ),
                    EngineMirNavigationFailureKind::StoryMismatch => host_error(
                        "host.pending.story_mismatch",
                        "异步导航不能附着到当前 Story",
                    ),
                    EngineMirNavigationFailureKind::Confirmation(error) => {
                        story_navigation_diagnostic(error)
                    }
                };
                let diagnostic = if failure.story_rollback.is_some() {
                    host_error(
                        "engine.rollback.failed",
                        "异步导航失败，且 Story 检查点无法恢复",
                    )
                } else {
                    diagnostic
                };
                Err(Box::new(HostNavigationError::Failed(diagnostic)))
            }
            Err(EngineMirNavigationResumeError::Continue(error)) => {
                let (transaction, diagnostic) = mir_resume_failure(*error);
                let rollback_failed: bool = transaction.rollback(state, story).is_err();
                Err(Box::new(HostNavigationError::Failed(if rollback_failed {
                    host_error(
                        "engine.rollback.failed",
                        "导航目标继续失败，且 Story 检查点无法恢复",
                    )
                } else {
                    diagnostic
                })))
            }
        }
    }

    /// 在 MacroPending 边界重新进入统一 Macro 控制器。
    pub fn dispatch_macro<'hir, 'source, Pending, DispatchError>(
        stable: HostStable<'hir, 'source>,
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        dispatch: impl FnOnce(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
    ) -> Result<HostMacroDispatch<'hir, 'source>, Box<HostMacroDispatchError<'hir, 'source, Pending>>>
    {
        let HostStable {
            execution,
            boundary,
        } = stable;
        match boundary.dispatch_macro(mir, state, story, dispatch) {
            Ok(EngineMirMacroDispatch::Continue(boundary)) => {
                Ok(HostMacroDispatch::Continue(Box::new(HostStable {
                    execution,
                    boundary,
                })))
            }
            Ok(EngineMirMacroDispatch::Pending(continuation)) => {
                if pending.add(execution, continuation).is_err() {
                    panic!("活动 Macro Token 已取出时必须可以保存新 continuation");
                }
                Ok(HostMacroDispatch::Pending { execution })
            }
            Err(EngineMirMacroDispatchError::NotMacro(boundary)) => Err(Box::new(
                HostMacroDispatchError::NotMacro(Box::new(HostStable {
                    execution,
                    boundary: *boundary,
                })),
            )),
            Err(EngineMirMacroDispatchError::InvalidSuspension(invalid)) => {
                let invalid = *invalid;
                state.restore_checkpoint(invalid.state_checkpoint);
                let rollback_failed: bool = story.restore(invalid.story_snapshot).is_err();
                let handle: Pending = match invalid.error {
                    RuntimeMacroContinuationError::IdentityMismatch { parts, .. }
                    | RuntimeMacroContinuationError::ExpectedMacroPending { parts, .. } => {
                        parts.suspension.handle
                    }
                };
                Err(Box::new(HostMacroDispatchError::Failed {
                    diagnostic: host_error(
                        if rollback_failed {
                            "engine.rollback.failed"
                        } else {
                            "host.pending.invalid_suspension"
                        },
                        "Macro 分派返回了与当前执行链不一致的 suspension",
                    ),
                    pending: Some(handle),
                }))
            }
            Err(error) => {
                let transaction: EngineMirResumedTransaction<'hir, 'source> = match error {
                    EngineMirMacroDispatchError::Story(transaction)
                    | EngineMirMacroDispatchError::Callback { transaction, .. }
                    | EngineMirMacroDispatchError::Vm { transaction, .. } => *transaction,
                    EngineMirMacroDispatchError::Continue(error) => match *error {
                        EngineMirVmResumeError::Story(transaction)
                        | EngineMirVmResumeError::StoryRequest { transaction, .. }
                        | EngineMirVmResumeError::Vm { transaction, .. }
                        | EngineMirVmResumeError::IncludeLimitExceeded { transaction, .. }
                        | EngineMirVmResumeError::UnexpectedMacroControl { transaction, .. } => {
                            *transaction
                        }
                    },
                    EngineMirMacroDispatchError::NotMacro(_)
                    | EngineMirMacroDispatchError::InvalidSuspension(_) => unreachable!(),
                };
                let rollback_failed: bool = transaction.rollback(state, story).is_err();
                Err(Box::new(HostMacroDispatchError::Failed {
                    diagnostic: host_error(
                        if rollback_failed {
                            "engine.rollback.failed"
                        } else {
                            "host.pending.dispatch_failed"
                        },
                        "后续 Macro 分派失败，事务已回滚",
                    ),
                    pending: None,
                }))
            }
        }
    }

    /// 从一个稳定边界持续驱动事务，直到产生最终更新或新的异步等待。
    ///
    /// Binding 无需逐个理解 VM 边界；平台异步句柄仍只保存在 `pending` 中。
    #[allow(clippy::too_many_arguments)]
    pub fn drive_stable_with_reaction<
        'hir,
        'source,
        Pending,
        DispatchError,
        Lifecycle,
        Reaction,
        Dispatch,
    >(
        mut stable: HostStable<'hir, 'source>,
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        mut lifecycle: Lifecycle,
        mut reaction: Reaction,
        mut dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Reaction: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, Diagnostic>,
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
        loop {
            stable = match stable.boundary() {
                HostStableBoundary::Halted => {
                    return Self::commit_halted(stable, state, story, &mut lifecycle)
                        .map(HostDriveResult::Ready)
                        .map_err(|error| match *error {
                            HostCommitError::Failed(diagnostic) => Box::new(HostDriveError {
                                diagnostic,
                                pending: None,
                            }),
                            HostCommitError::NotHalted(_) => {
                                unreachable!("Halted 分类必须能由 commit_halted 消费")
                            }
                        });
                }
                HostStableBoundary::NavigationPending | HostStableBoundary::PassageStopped => {
                    Self::continue_navigation(
                        stable,
                        state,
                        story,
                        mir,
                        &mut lifecycle,
                        &mut reaction,
                    )
                    .map_err(|error| match *error {
                        HostNavigationError::Failed(diagnostic) => Box::new(HostDriveError {
                            diagnostic,
                            pending: None,
                        }),
                        HostNavigationError::NotNavigation(_) => {
                            unreachable!("导航分类必须能由 continue_navigation 消费")
                        }
                    })?
                }
                HostStableBoundary::MacroPending => {
                    match Self::dispatch_macro(stable, pending, state, story, mir, &mut dispatch)
                        .map_err(|error| match *error {
                            HostMacroDispatchError::Failed {
                                diagnostic,
                                pending,
                            } => Box::new(HostDriveError {
                                diagnostic,
                                pending,
                            }),
                            HostMacroDispatchError::NotMacro(_) => {
                                unreachable!("Macro 分类必须能由 dispatch_macro 消费")
                            }
                        })? {
                        HostMacroDispatch::Pending { execution } => {
                            return Ok(HostDriveResult::Pending { execution });
                        }
                        HostMacroDispatch::Continue(stable) => *stable,
                    }
                }
            };
        }
    }

    /// 不安装 Reaction Phase 的兼容恢复入口。
    pub fn resume_and_drive<
        'hir,
        'source,
        Pending,
        ResumeError,
        DispatchError,
        Lifecycle,
        Resume,
        Dispatch,
    >(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        execution: HostExecutionToken,
        callbacks: HostResumeCallbacks<Lifecycle, Resume, Dispatch>,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Resume: FnOnce(
            Pending,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            &mut MacroLocalScopes<Value>,
        )
            -> Result<MacroHandlerOutcome<RuntimeMacroExecution, Pending>, ResumeError>,
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
        let HostResumeCallbacks {
            lifecycle,
            resume,
            dispatch,
        } = callbacks;
        Self::resume_and_drive_with_reaction(
            pending,
            state,
            story,
            mir,
            execution,
            HostResumeReactionCallbacks::new(lifecycle, no_passage_reaction, resume, dispatch),
        )
    }

    /// 恢复当前异步 Handler，并自动驱动到可呈现结果或下一次等待。
    pub fn resume_and_drive_with_reaction<
        'hir,
        'source,
        Pending,
        ResumeError,
        DispatchError,
        Lifecycle,
        Resume,
        Reaction,
        Dispatch,
    >(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        execution: HostExecutionToken,
        callbacks: HostResumeReactionCallbacks<Lifecycle, Reaction, Resume, Dispatch>,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Reaction: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, Diagnostic>,
        Resume: FnOnce(
            Pending,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            &mut MacroLocalScopes<Value>,
        )
            -> Result<MacroHandlerOutcome<RuntimeMacroExecution, Pending>, ResumeError>,
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
        let HostResumeReactionCallbacks {
            lifecycle,
            reaction,
            resume,
            dispatch,
        } = callbacks;
        let resumed: HostResumeOutcome<'hir, 'source> = Self::resume_pending(
            pending, state, story, mir, execution, resume,
        )
        .map_err(|diagnostic| {
            Box::new(HostDriveError {
                diagnostic,
                pending: None,
            })
        })?;
        let HostResumeOutcome::Continue(resumed) = resumed else {
            return Ok(HostDriveResult::Pending { execution });
        };
        let stable: HostStable<'hir, 'source> = Self::continue_resumed(*resumed, state, story, mir)
            .map_err(|diagnostic| {
                Box::new(HostDriveError {
                    diagnostic,
                    pending: None,
                })
            })?;
        Self::drive_stable_with_reaction(
            stable, pending, state, story, mir, lifecycle, reaction, dispatch,
        )
    }
}

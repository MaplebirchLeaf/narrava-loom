//! MIR 导航继续与最终提交。
//!
//! 导航链必须在确认目标、执行 Passage 生命周期并通过预算检查后才能提交；任何中途失败
//! 都恢复最初的 State 检查点和 Story 快照。

use super::*;

pub enum EngineMirNavigationResumeError<'hir, 'source, LifecycleError> {
    NotNavigation(Box<EngineMirVmResume<'hir, 'source>>),
    Failed(Box<EngineMirNavigationFailure<LifecycleError>>),
    Continue(Box<EngineMirVmResumeError<'hir, 'source>>),
}

pub struct EngineMirNavigationFailure<LifecycleError> {
    pub kind: EngineMirNavigationFailureKind<LifecycleError>,
    pub story_rollback: Option<StorySnapshotError>,
    pub scopes: MacroLocalScopes<Value>,
}

pub enum EngineMirNavigationFailureKind<LifecycleError> {
    StoryMismatch,
    UnconsumedIncludes {
        count: usize,
    },
    MissingGoto,
    PassageLimitExceeded {
        limit: usize,
    },
    MissingMirPassage(String),
    Lifecycle {
        phase: PassageLifecyclePhase,
        error: LifecycleError,
    },
    Confirmation(StoryNavigationError),
}

pub(super) fn continue_navigation_transaction<'hir, 'source, LifecycleError>(
    transaction: EngineMirResumedTransaction<'hir, 'source>,
    mir: &BytecodeProgram,
    state: &mut State,
    story: &mut Story<'hir, 'source>,
    lifecycle: &mut impl FnMut(
        PassageLifecyclePhase,
        PassageLifecycleContext<'_, 'hir, 'source, '_>,
        &mut State,
    ) -> Result<(), LifecycleError>,
) -> Result<
    EngineMirVmResume<'hir, 'source>,
    EngineMirNavigationResumeError<'hir, 'source, LifecycleError>,
> {
    let EngineMirResumedTransaction {
        mut runtime,
        state_checkpoint,
        story_snapshot,
        requests,
        mut progress,
    } = transaction;
    let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
        match StoryRuntimeRequests::from_pending(story, requests) {
            Ok(requests) => requests,
            Err(_error) => {
                return Err(EngineMirNavigationResumeError::Failed(Box::new(
                    rollback_navigation_failure(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        runtime.scopes,
                        EngineMirNavigationFailureKind::StoryMismatch,
                    ),
                )));
            }
        };
    let include_count: usize = requests.pending_include_count();
    if include_count != 0 {
        drop(requests);
        return Err(EngineMirNavigationResumeError::Failed(Box::new(
            rollback_navigation_failure(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                runtime.scopes,
                EngineMirNavigationFailureKind::UnconsumedIncludes {
                    count: include_count,
                },
            ),
        )));
    }
    let Some(request) = requests.take_goto() else {
        drop(requests);
        return Err(EngineMirNavigationResumeError::Failed(Box::new(
            rollback_navigation_failure(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                runtime.scopes,
                EngineMirNavigationFailureKind::MissingGoto,
            ),
        )));
    };
    let requests: StoryRuntimePending<'hir, 'source> = requests.into_pending();
    let next_executed: usize = progress.executed_passages.saturating_add(1);
    if next_executed >= progress.limits.passages {
        return Err(EngineMirNavigationResumeError::Failed(Box::new(
            rollback_navigation_failure(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                runtime.scopes,
                EngineMirNavigationFailureKind::PassageLimitExceeded {
                    limit: progress.limits.passages,
                },
            ),
        )));
    }

    let end_context: PassageLifecycleContext<'_, 'hir, 'source, '_> =
        PassageLifecycleContext::new(progress.current, &progress.params);
    if let Err(error) = lifecycle(PassageLifecyclePhase::End, end_context, state) {
        return Err(EngineMirNavigationResumeError::Failed(Box::new(
            rollback_navigation_failure(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                runtime.scopes,
                EngineMirNavigationFailureKind::Lifecycle {
                    phase: PassageLifecyclePhase::End,
                    error,
                },
            ),
        )));
    }
    let confirmed: StoryHistoryEntry<'hir, 'source> = match story.confirm_navigation(request) {
        Ok(entry) => *entry,
        Err(error) => {
            return Err(EngineMirNavigationResumeError::Failed(Box::new(
                rollback_navigation_failure(
                    state,
                    story,
                    state_checkpoint,
                    story_snapshot,
                    runtime.scopes,
                    EngineMirNavigationFailureKind::Confirmation(error),
                ),
            )));
        }
    };
    let confirmed_name: &str = confirmed.passage().name;
    let Some(mir_passage): Option<&BytecodePassage> = mir.passage(confirmed_name) else {
        return Err(EngineMirNavigationResumeError::Failed(Box::new(
            rollback_navigation_failure(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                runtime.scopes,
                EngineMirNavigationFailureKind::MissingMirPassage(confirmed_name.to_owned()),
            ),
        )));
    };

    let _removed_temporary: usize = state.temporary_clear();
    let target_params: Value = Value::Undefined;
    let start_context: PassageLifecycleContext<'_, 'hir, 'source, '_> =
        PassageLifecycleContext::new(confirmed, &target_params);
    for phase in [PassageLifecyclePhase::Init, PassageLifecyclePhase::Start] {
        if let Err(error) = lifecycle(phase, start_context, state) {
            return Err(EngineMirNavigationResumeError::Failed(Box::new(
                rollback_navigation_failure(
                    state,
                    story,
                    state_checkpoint,
                    story_snapshot,
                    runtime.scopes,
                    EngineMirNavigationFailureKind::Lifecycle { phase, error },
                ),
            )));
        }
    }

    let previous_frame = std::mem::replace(
        &mut runtime.frame,
        crate::vm::MirExecutionFrame::new(mir_passage),
    );
    progress.output.append(previous_frame.into_output());
    progress.current = confirmed;
    progress.entries.push(confirmed);
    progress.params = target_params;
    progress.executed_passages = next_executed;
    progress.macro_includes_entered = 0;
    runtime.control = BodyControl::Continue;
    runtime.includes_entered = 0;
    EngineMirResumedTransaction {
        runtime,
        state_checkpoint,
        story_snapshot,
        requests,
        progress,
    }
    .continue_vm(mir, state, story)
    .map_err(|error| EngineMirNavigationResumeError::Continue(Box::new(error)))
}

fn rollback_navigation_failure<'hir, 'source, LifecycleError>(
    state: &mut State,
    story: &mut Story<'hir, 'source>,
    state_checkpoint: StateCheckpoint,
    story_snapshot: StorySnapshot<'hir, 'source>,
    scopes: MacroLocalScopes<Value>,
    kind: EngineMirNavigationFailureKind<LifecycleError>,
) -> EngineMirNavigationFailure<LifecycleError> {
    state.restore_checkpoint(state_checkpoint);
    let story_rollback: Option<StorySnapshotError> = story.restore(story_snapshot).err();
    EngineMirNavigationFailure {
        kind,
        story_rollback,
        scopes,
    }
}

pub enum EngineMirCommitError<'hir, 'source, LifecycleError> {
    NotHalted(Box<EngineMirVmResume<'hir, 'source>>),
    Failed(Box<EngineMirCommitFailure<LifecycleError>>),
}

pub struct EngineMirCommitFailure<LifecycleError> {
    pub kind: EngineMirCommitFailureKind<LifecycleError>,
    pub story_rollback: Option<StorySnapshotError>,
    pub scopes: MacroLocalScopes<Value>,
}

pub enum EngineMirCommitFailureKind<LifecycleError> {
    StoryMismatch,
    UnconsumedIncludes {
        count: usize,
    },
    UnexpectedGoto,
    Lifecycle {
        phase: PassageLifecyclePhase,
        error: LifecycleError,
    },
}

pub(super) fn commit_halted_transaction<'hir, 'source, LifecycleError>(
    transaction: EngineMirResumedTransaction<'hir, 'source>,
    state: &mut State,
    story: &mut Story<'hir, 'source>,
    lifecycle: &mut impl FnMut(
        PassageLifecyclePhase,
        PassageLifecycleContext<'_, 'hir, 'source, '_>,
        &mut State,
    ) -> Result<(), LifecycleError>,
) -> Result<EngineMirCommitted<'hir, 'source>, EngineMirCommitFailure<LifecycleError>> {
    let EngineMirResumedTransaction {
        runtime,
        state_checkpoint,
        story_snapshot,
        requests,
        mut progress,
    } = transaction;
    let RuntimeMacroResumed { frame, scopes, .. } = runtime;
    let requests: StoryRuntimeRequests<'_, 'hir, 'source> =
        match StoryRuntimeRequests::from_pending(story, requests) {
            Ok(requests) => requests,
            Err(_error) => {
                return Err(rollback_commit_failure(
                    state,
                    story,
                    state_checkpoint,
                    story_snapshot,
                    scopes,
                    EngineMirCommitFailureKind::StoryMismatch,
                ));
            }
        };
    let include_count: usize = requests.pending_include_count();
    let has_goto: bool = requests.pending_goto().is_some();
    drop(requests);
    if include_count != 0 {
        return Err(rollback_commit_failure(
            state,
            story,
            state_checkpoint,
            story_snapshot,
            scopes,
            EngineMirCommitFailureKind::UnconsumedIncludes {
                count: include_count,
            },
        ));
    }
    if has_goto {
        return Err(rollback_commit_failure(
            state,
            story,
            state_checkpoint,
            story_snapshot,
            scopes,
            EngineMirCommitFailureKind::UnexpectedGoto,
        ));
    }

    let mut passage_output: PresentationOutput = frame.into_output();
    story.record_navigation(passage_output.has_navigation());
    let current: StoryHistoryEntry<'hir, 'source> = progress.current;
    if !current.passage().has_tag("exit") {
        if !passage_output.has_navigation()
            && let Some(target) = story.safe_return_target()
        {
            passage_output.push(PresentationNode::SafeReturn {
                id: InteractionId::from_key(format!("safe-return:{}", target.name)),
                target: target.name.to_owned(),
            });
        }
        let context: PassageLifecycleContext<'_, 'hir, 'source, '_> =
            PassageLifecycleContext::with_output(current, &progress.params, &passage_output);
        for phase in [
            PassageLifecyclePhase::Render,
            PassageLifecyclePhase::Display,
        ] {
            if let Err(error) = lifecycle(phase, context, state) {
                return Err(rollback_commit_failure(
                    state,
                    story,
                    state_checkpoint,
                    story_snapshot,
                    scopes,
                    EngineMirCommitFailureKind::Lifecycle { phase, error },
                ));
            }
        }
    }
    progress.output.append(passage_output);
    Ok(EngineMirCommitted {
        navigation: EngineNavigationChain {
            entries: progress.entries,
            output: progress.output,
        },
        scopes,
    })
}

fn rollback_commit_failure<'hir, 'source, LifecycleError>(
    state: &mut State,
    story: &mut Story<'hir, 'source>,
    state_checkpoint: StateCheckpoint,
    story_snapshot: StorySnapshot<'hir, 'source>,
    scopes: MacroLocalScopes<Value>,
    kind: EngineMirCommitFailureKind<LifecycleError>,
) -> EngineMirCommitFailure<LifecycleError> {
    state.restore_checkpoint(state_checkpoint);
    let story_rollback: Option<StorySnapshotError> = story.restore(story_snapshot).err();
    EngineMirCommitFailure {
        kind,
        story_rollback,
        scopes,
    }
}

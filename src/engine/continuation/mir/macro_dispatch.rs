//! 动态 Macro 分派，把 VM 的 MacroPending 边界转换为完成或可恢复暂停。

use super::*;

/// Engine 交给 Macro 控制器的调用，以及在当前位置创建的显式捕获值。
pub struct EngineMirMacroInvocation<'call> {
    pub call: &'call crate::hir::OwnedHirMacro,
    pub identity: RuntimeExecutionIdentity,
    pub location: crate::mir::MirExecutionPosition,
    pub captures: CapturedMacroLocals<Value>,
}

pub struct EngineMirMacroCallbackFailure<Error> {
    pub error: Error,
    pub scopes: MacroLocalScopes<Value>,
}

pub enum EngineMirMacroDispatch<'hir, 'source, Pending> {
    Continue(EngineMirVmResume<'hir, 'source>),
    Pending(EngineMirContinuation<'hir, 'source, Pending>),
}

pub enum EngineMirMacroDispatchError<'hir, 'source, Pending, DispatchError> {
    NotMacro(Box<EngineMirVmResume<'hir, 'source>>),
    Story(Box<EngineMirResumedTransaction<'hir, 'source>>),
    Callback {
        error: DispatchError,
        transaction: Box<EngineMirResumedTransaction<'hir, 'source>>,
    },
    Vm {
        error: MirExecutionError,
        transaction: Box<EngineMirResumedTransaction<'hir, 'source>>,
    },
    Continue(Box<EngineMirVmResumeError<'hir, 'source>>),
    InvalidSuspension(Box<EngineMirInvalidSuspension<'hir, 'source, Pending>>),
}

pub struct EngineMirInvalidSuspension<'hir, 'source, Pending> {
    pub error: RuntimeMacroContinuationError<Pending>,
    pub state_checkpoint: StateCheckpoint,
    pub story_snapshot: StorySnapshot<'hir, 'source>,
    pub requests: StoryRuntimePending<'hir, 'source>,
    pub progress: EngineMirProgress<'hir, 'source>,
}

pub(super) fn dispatch_macro_transaction<'hir, 'source, Pending, DispatchError>(
    transaction: EngineMirResumedTransaction<'hir, 'source>,
    mir: &BytecodeProgram,
    state: &mut State,
    story: &Story<'hir, 'source>,
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
    EngineMirMacroDispatch<'hir, 'source, Pending>,
    EngineMirMacroDispatchError<'hir, 'source, Pending, DispatchError>,
> {
    let EngineMirResumedTransaction {
        runtime,
        state_checkpoint,
        story_snapshot,
        requests,
        progress,
    } = transaction;
    let RuntimeMacroResumed {
        identity,
        mut frame,
        control: _previous_control,
        includes_entered: _previous_includes,
        scopes,
    } = runtime;
    let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
        match StoryRuntimeRequests::from_pending(story, requests) {
            Ok(requests) => requests,
            Err(error) => {
                return Err(EngineMirMacroDispatchError::Story(Box::new(
                    EngineMirResumedTransaction {
                        runtime: RuntimeMacroResumed {
                            identity,
                            frame,
                            control: BodyControl::Continue,
                            includes_entered: 0,
                            scopes,
                        },
                        state_checkpoint,
                        story_snapshot,
                        requests: error.pending,
                        progress,
                    },
                )));
            }
        };
    let call = frame
        .pending_macro(mir)
        .expect("MacroPending 边界必须保存原始 HIR 调用");
    let capture_names: Vec<&str> = frame
        .pending_macro_captures(mir)
        .expect("MacroPending 边界必须保存捕获名称");
    let captures: CapturedMacroLocals<Value> = scopes.capture(&capture_names);
    let location: crate::mir::MirExecutionPosition = frame.location();
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
        Err(failure) => Err(EngineMirMacroDispatchError::Callback {
            error: failure.error,
            transaction: Box::new(EngineMirResumedTransaction {
                runtime: RuntimeMacroResumed {
                    identity,
                    frame,
                    control: BodyControl::Continue,
                    includes_entered: 0,
                    scopes: failure.scopes,
                },
                state_checkpoint,
                story_snapshot,
                requests,
                progress,
            }),
        }),
        Ok(MacroResumeOutcome::Pending(suspension)) => {
            match RuntimeMacroContinuation::new(identity, frame, suspension, mir) {
                Ok(runtime) => Ok(EngineMirMacroDispatch::Pending(EngineMirContinuation::new(
                    runtime,
                    state_checkpoint,
                    story_snapshot,
                    requests,
                    progress,
                ))),
                Err(error) => Err(EngineMirMacroDispatchError::InvalidSuspension(Box::new(
                    EngineMirInvalidSuspension {
                        error,
                        state_checkpoint,
                        story_snapshot,
                        requests,
                        progress,
                    },
                ))),
            }
        }
        Ok(MacroResumeOutcome::Complete { output, scopes }) => {
            let RuntimeMacroExecution {
                execution,
                includes_entered,
            } = output;
            let control: BodyControl = execution.control;
            if let Err(error) = frame.complete_macro(mir, execution.output) {
                return Err(EngineMirMacroDispatchError::Vm {
                    error,
                    transaction: Box::new(EngineMirResumedTransaction {
                        runtime: RuntimeMacroResumed {
                            identity,
                            frame,
                            control,
                            includes_entered,
                            scopes,
                        },
                        state_checkpoint,
                        story_snapshot,
                        requests,
                        progress,
                    }),
                });
            }
            let transaction = EngineMirResumedTransaction {
                runtime: RuntimeMacroResumed {
                    identity,
                    frame,
                    control,
                    includes_entered,
                    scopes,
                },
                state_checkpoint,
                story_snapshot,
                requests,
                progress,
            };
            transaction
                .continue_vm(mir, state, story)
                .map(EngineMirMacroDispatch::Continue)
                .map_err(|error| EngineMirMacroDispatchError::Continue(Box::new(error)))
        }
    }
}

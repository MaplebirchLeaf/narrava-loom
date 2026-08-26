//! 动态 Macro 分派，把 VM 的 MacroPending 边界转换为完成或可恢复暂停。

use super::*;

/// Engine 交给 Macro 控制器的调用，以及在当前位置创建的显式捕获值。
pub struct EngineMirMacroInvocation<'call> {
    /// 触发暂停的原始 HIR 调用。
    pub call: &'call crate::hir::OwnedHirMacro,
    /// 本次执行链的运行时身份。
    pub identity: RuntimeExecutionIdentity,
    /// 调用在 Bytecode 中的执行位置。
    pub location: crate::mir::MirExecutionPosition,
    /// 按 `capture` 名称显式捕获的局部绑定。
    pub captures: CapturedMacroLocals<Value>,
}

/// Macro 控制器回调失败，同时交还外层作用域以便回滚。
pub struct EngineMirMacroCallbackFailure<Error> {
    /// 控制器报告的错误。
    pub error: Error,
    /// 回调失败时仍归控制器持有的作用域。
    pub scopes: MacroLocalScopes<Value>,
}

/// 一次动态 Macro 分派的结果：已完成并继续 VM，或再次暂停。
pub enum EngineMirMacroDispatch<'hir, 'source, Pending> {
    /// Macro 完成，事务继续到下一个稳定边界。
    Continue(EngineMirVmResume<'hir, 'source>),
    /// Macro 异步暂停，等待后续恢复。
    Pending(EngineMirContinuation<'hir, 'source, Pending>),
}

/// 动态 Macro 分派阶段的失败；除 `NotMacro` 外都保留可回滚事务。
pub enum EngineMirMacroDispatchError<'hir, 'source, Pending, DispatchError> {
    /// 当前边界不是 MacroPending，原样交还。
    NotMacro(Box<EngineMirVmResume<'hir, 'source>>),
    /// Story 无法从待处理请求重新附着。
    Story(Box<EngineMirResumedTransaction<'hir, 'source>>),
    /// 控制器回调失败。
    Callback {
        error: DispatchError,
        transaction: Box<EngineMirResumedTransaction<'hir, 'source>>,
    },
    /// 完成 Macro 写入 VM 失败。
    Vm {
        error: MirExecutionError,
        transaction: Box<EngineMirResumedTransaction<'hir, 'source>>,
    },
    /// Macro 完成后继续 VM 失败，事务保持原样。
    Continue(Box<EngineMirVmResumeError<'hir, 'source>>),
    /// 暂停句柄与执行身份不一致，无法建立 continuation。
    InvalidSuspension(Box<EngineMirInvalidSuspension<'hir, 'source, Pending>>),
}

/// 无法建立 continuation 的暂停状态；事务组件原样交还。
pub struct EngineMirInvalidSuspension<'hir, 'source, Pending> {
    /// 构造失败的具体原因。
    pub error: RuntimeMacroContinuationError<Pending>,
    pub state_checkpoint: StateCheckpoint,
    pub story_snapshot: StorySnapshot<'hir, 'source>,
    pub requests: StoryRuntimePending<'hir, 'source>,
    pub progress: EngineMirProgress<'hir, 'source>,
}

/// 把 VM 的 MacroPending 边界转换为完成或可恢复暂停。
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

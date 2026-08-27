//! Passage/MIR 暂停、Macro 恢复、导航继续、提交与回滚事务。
//!
//! 本模块中的所有类型共同维护一个不变量：State 检查点、Story 快照、待处理请求和
//! VM frame 必须属于同一执行身份，任何失败都通过统一回滚路径恢复外层事务。

use super::*;

/// 暂停发生时已完成的 Passage 导航进度。
#[derive(Debug, PartialEq)]
pub struct EngineMirProgress<'hir, 'source> {
    current: StoryHistoryEntry<'hir, 'source>,
    entries: Vec<StoryHistoryEntry<'hir, 'source>>,
    output: SemanticOutput,
    params: Value,
    executed_passages: usize,
    macro_includes_entered: usize,
    limits: EngineExecutionLimits,
    language: Option<I18nRuntimeLanguage>,
}

impl<'hir, 'source> EngineMirProgress<'hir, 'source> {
    /// 建立暂停进度；`entries` 末尾必须是 `current`。
    pub fn new(
        current: StoryHistoryEntry<'hir, 'source>,
        entries: Vec<StoryHistoryEntry<'hir, 'source>>,
        output: SemanticOutput,
        params: &Value,
        executed_passages: usize,
        macro_includes_entered: usize,
        limits: EngineExecutionLimits,
    ) -> Self {
        debug_assert_eq!(entries.last(), Some(&current));
        Self {
            current,
            entries,
            output,
            // 参数可能含引用对象；暂停状态必须与活动 State 图隔离。
            params: params.detached_clone(),
            executed_passages,
            macro_includes_entered,
            limits,
            language: None,
        }
    }

    /// 附加后续恢复时 VM 步进使用的目标语言。
    pub fn with_language(mut self, language: Option<I18nRuntimeLanguage>) -> Self {
        self.language = language;
        self
    }

    /// 暂停时正在执行的 Passage。
    pub fn current(&self) -> StoryHistoryEntry<'hir, 'source> {
        self.current
    }

    /// 已进入并确认的 Passage 历史链。
    pub fn entries(&self) -> &[StoryHistoryEntry<'hir, 'source>] {
        &self.entries
    }

    /// 链上已累积的有序语义输出。
    pub fn output(&self) -> &SemanticOutput {
        &self.output
    }

    /// 入口 Passage 收到的参数副本。
    pub fn params(&self) -> &Value {
        &self.params
    }

    /// 已执行完的 Passage 数量（用于预算检查）。
    pub fn executed_passages(&self) -> usize {
        self.executed_passages
    }

    /// Macro 调用已展开的 include 数量（用于预算检查）。
    pub fn macro_includes_entered(&self) -> usize {
        self.macro_includes_entered
    }

    /// 本事务的控制流预算。
    pub fn limits(&self) -> EngineExecutionLimits {
        self.limits
    }

    /// 恢复时 VM 步进使用的目标语言。
    pub fn language(&self) -> Option<&I18nRuntimeLanguage> {
        self.language.as_ref()
    }
}

/// Engine 等待异步 Macro 时必须作为整体保存的事务所有权。
///
/// 该类型可由 Core／Binding 显式组装，但恢复路径完成前不能作为 Host Resume 输入。
pub struct EngineMirContinuation<'hir, 'source, Pending> {
    runtime: RuntimeMacroContinuation<Pending>,
    state_checkpoint: StateCheckpoint,
    story_snapshot: StorySnapshot<'hir, 'source>,
    requests: StoryRuntimePending<'hir, 'source>,
    progress: EngineMirProgress<'hir, 'source>,
}

impl<'hir, 'source, Pending> EngineMirContinuation<'hir, 'source, Pending> {
    /// 组合暂停事务的全部所有权组件；恢复完成前不能作为 Host Resume 输入。
    pub fn new(
        runtime: RuntimeMacroContinuation<Pending>,
        state_checkpoint: StateCheckpoint,
        story_snapshot: StorySnapshot<'hir, 'source>,
        requests: StoryRuntimePending<'hir, 'source>,
        progress: EngineMirProgress<'hir, 'source>,
    ) -> Self {
        Self {
            runtime,
            state_checkpoint,
            story_snapshot,
            requests,
            progress,
        }
    }

    /// VM 级 Macro 暂停句柄。
    pub fn runtime(&self) -> &RuntimeMacroContinuation<Pending> {
        &self.runtime
    }

    /// 暂停时尚未确认的 Story 请求。
    pub fn requests(&self) -> &StoryRuntimePending<'hir, 'source> {
        &self.requests
    }

    /// 暂停时的 Passage 导航进度。
    pub fn progress(&self) -> &EngineMirProgress<'hir, 'source> {
        &self.progress
    }

    /// 重新附着 Story 请求并恢复当前异步 Macro；尚不继续后续 VM 指令。
    pub fn resume_macro<ResumeError>(
        self,
        mir: &BytecodeProgram,
        state: &mut State,
        story: &Story<'hir, 'source>,
        resume: impl FnOnce(
            Pending,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            &mut MacroLocalScopes<Value>,
        )
            -> Result<MacroHandlerOutcome<RuntimeMacroExecution, Pending>, ResumeError>,
    ) -> Result<
        EngineMirContinuationResume<'hir, 'source, Pending>,
        EngineMirContinuationResumeError<'hir, 'source, ResumeError, Pending>,
    > {
        let Self {
            runtime,
            state_checkpoint,
            story_snapshot,
            requests,
            progress,
        } = self;
        let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
            match StoryRuntimeRequests::from_pending(story, requests) {
                Ok(requests) => requests,
                Err(error) => {
                    return Err(EngineMirContinuationResumeError::Story(Box::new(Self {
                        runtime,
                        state_checkpoint,
                        story_snapshot,
                        requests: error.pending,
                        progress,
                    })));
                }
            };
        let resumed = runtime.resume(mir, |handle, locals| {
            resume(handle, state, &mut requests, locals)
        });
        let requests: StoryRuntimePending<'hir, 'source> = requests.into_pending();
        match resumed {
            Ok(RuntimeMacroContinuationResume::Pending(runtime)) => {
                Ok(EngineMirContinuationResume::Pending(Self {
                    runtime,
                    state_checkpoint,
                    story_snapshot,
                    requests,
                    progress,
                }))
            }
            Ok(RuntimeMacroContinuationResume::Complete(runtime)) => Ok(
                EngineMirContinuationResume::Complete(EngineMirResumedTransaction {
                    runtime,
                    state_checkpoint,
                    story_snapshot,
                    requests,
                    progress,
                }),
            ),
            Err(error) => Err(EngineMirContinuationResumeError::Runtime(Box::new(
                EngineMirContinuationResumeFailure {
                    error,
                    state_checkpoint,
                    story_snapshot,
                    requests,
                    progress,
                },
            ))),
        }
    }

    /// 取消等待并恢复暂停前的完整 State 与 Story 时间线。
    ///
    /// Macro suspension 被交还给调用者，便于 Binding 释放平台调度句柄。
    pub fn rollback(
        self,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
    ) -> Result<MacroSuspension<Pending>, EngineMirContinuationRollbackError<Pending>> {
        state.restore_checkpoint(self.state_checkpoint);
        let (_frame, suspension): (_, MacroSuspension<Pending>) = self.runtime.into_parts();
        match story.restore(self.story_snapshot) {
            Ok(()) => Ok(suspension),
            Err(story) => Err(EngineMirContinuationRollbackError { story, suspension }),
        }
    }
}

/// 取消暂停时 Story 回滚失败；Macro 调度句柄仍返还给调用者。
pub struct EngineMirContinuationRollbackError<Pending> {
    /// Story 快照恢复失败的详情。
    pub story: StorySnapshotError,
    /// 原样交还的 Macro 暂停句柄，供 Binding 释放调度资源。
    pub suspension: MacroSuspension<Pending>,
}

/// 当前异步 Macro 完成后，等待 Engine 继续驱动 VM 与 Passage 生命周期的事务。
pub struct EngineMirResumedTransaction<'hir, 'source> {
    /// Macro 完成后的 VM 与控制状态。
    pub runtime: RuntimeMacroResumed,
    state_checkpoint: StateCheckpoint,
    story_snapshot: StorySnapshot<'hir, 'source>,
    requests: StoryRuntimePending<'hir, 'source>,
    progress: EngineMirProgress<'hir, 'source>,
}

impl<'hir, 'source> EngineMirResumedTransaction<'hir, 'source> {
    /// 等待确认的 Story 请求。
    pub fn requests(&self) -> &StoryRuntimePending<'hir, 'source> {
        &self.requests
    }

    /// 当前 Passage 导航进度。
    pub fn progress(&self) -> &EngineMirProgress<'hir, 'source> {
        &self.progress
    }

    /// 校验已完成 Macro 的边界结果，并继续 VM 到下一个稳定边界。
    pub fn continue_vm(
        mut self,
        mir: &BytecodeProgram,
        state: &mut State,
        story: &Story<'hir, 'source>,
    ) -> Result<EngineMirVmResume<'hir, 'source>, EngineMirVmResumeError<'hir, 'source>> {
        let control: BodyControl = self.runtime.control;
        if !matches!(control, BodyControl::Continue | BodyControl::StopPassage) {
            return Err(EngineMirVmResumeError::UnexpectedMacroControl {
                control,
                transaction: Box::new(self),
            });
        }
        self.progress.macro_includes_entered = self
            .progress
            .macro_includes_entered
            .saturating_add(self.runtime.includes_entered);
        self.runtime.includes_entered = 0;
        if self.total_includes_entered() > self.progress.limits.includes {
            return Err(EngineMirVmResumeError::IncludeLimitExceeded {
                limit: self.progress.limits.includes,
                transaction: Box::new(self),
            });
        }
        if control == BodyControl::StopPassage {
            return Ok(EngineMirVmResume::PassageStopped(self));
        }

        let Self {
            mut runtime,
            state_checkpoint,
            story_snapshot,
            requests,
            progress,
        } = self;
        let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
            match StoryRuntimeRequests::from_pending(story, requests) {
                Ok(requests) => requests,
                Err(error) => {
                    return Err(EngineMirVmResumeError::Story(Box::new(Self {
                        runtime,
                        state_checkpoint,
                        story_snapshot,
                        requests: error.pending,
                        progress,
                    })));
                }
            };

        loop {
            let result: Result<MirStep, MirExecutionError> = match progress.language() {
                Some(language) => runtime
                    .frame
                    .step_with_runtime_language(mir, language, state),
                None => runtime.frame.step(mir, state),
            };
            let step: MirStep = match result {
                Ok(step) => step,
                Err(error) => {
                    return Err(EngineMirVmResumeError::Vm {
                        error,
                        transaction: Box::new(Self {
                            runtime,
                            state_checkpoint,
                            story_snapshot,
                            requests: requests.into_pending(),
                            progress,
                        }),
                    });
                }
            };
            let total_includes_entered: usize = progress
                .macro_includes_entered
                .saturating_add(runtime.frame.includes_entered());
            if total_includes_entered > progress.limits.includes {
                return Err(EngineMirVmResumeError::IncludeLimitExceeded {
                    limit: progress.limits.includes,
                    transaction: Box::new(Self {
                        runtime,
                        state_checkpoint,
                        story_snapshot,
                        requests: requests.into_pending(),
                        progress,
                    }),
                });
            }
            match step {
                MirStep::Running => {}
                MirStep::Halted => {
                    return Ok(EngineMirVmResume::Halted(Self {
                        runtime,
                        state_checkpoint,
                        story_snapshot,
                        requests: requests.into_pending(),
                        progress,
                    }));
                }
                MirStep::MacroPending => {
                    return Ok(EngineMirVmResume::MacroPending(Self {
                        runtime,
                        state_checkpoint,
                        story_snapshot,
                        requests: requests.into_pending(),
                        progress,
                    }));
                }
                MirStep::NavigationPending => {
                    let target: String = runtime
                        .frame
                        .navigation()
                        .expect("NavigationPending 必须保存 goto 目标")
                        .to_owned();
                    if let Err(error) = requests.goto(&target) {
                        return Err(EngineMirVmResumeError::StoryRequest {
                            error,
                            transaction: Box::new(Self {
                                runtime,
                                state_checkpoint,
                                story_snapshot,
                                requests: requests.into_pending(),
                                progress,
                            }),
                        });
                    }
                    return Ok(EngineMirVmResume::NavigationPending(Self {
                        runtime,
                        state_checkpoint,
                        story_snapshot,
                        requests: requests.into_pending(),
                        progress,
                    }));
                }
            }
        }
    }

    fn total_includes_entered(&self) -> usize {
        self.progress
            .macro_includes_entered
            .saturating_add(self.runtime.frame.includes_entered())
    }

    /// 放弃完成后的续跑，并回滚最初导航事务。
    pub fn rollback(
        self,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
    ) -> Result<MacroLocalScopes<Value>, EngineMirResumedRollbackError> {
        state.restore_checkpoint(self.state_checkpoint);
        let scopes: MacroLocalScopes<Value> = self.runtime.scopes;
        match story.restore(self.story_snapshot) {
            Ok(()) => Ok(scopes),
            Err(story) => Err(EngineMirResumedRollbackError { story, scopes }),
        }
    }
}

/// 放弃完成后的续跑时 Story 回滚失败；外层 Macro scopes 仍返还给调用者。
pub struct EngineMirResumedRollbackError {
    /// Story 快照恢复失败的详情。
    pub story: StorySnapshotError,
    /// 回滚后仍应归还的 Macro 局部作用域。
    pub scopes: MacroLocalScopes<Value>,
}

/// 恢复后的 VM 到达的下一个稳定执行边界。
pub enum EngineMirVmResume<'hir, 'source> {
    /// 当前 Passage 执行完毕，可提交导航链。
    Halted(EngineMirResumedTransaction<'hir, 'source>),
    /// VM 请求了 goto，等待 Engine 确认导航并进入目标 Passage。
    NavigationPending(EngineMirResumedTransaction<'hir, 'source>),
    /// 遇到动态 Macro，等待交给 Macro 控制器。
    MacroPending(EngineMirResumedTransaction<'hir, 'source>),
    /// Macro 请求停止当前 Passage，等待 Engine 处理导航请求。
    PassageStopped(EngineMirResumedTransaction<'hir, 'source>),
}

/// 从首次导航已经建立的事务组件启动 MIR，并运行到第一个稳定边界。
///
/// 该入口与异步恢复共用 `continue_vm()`，因此首次 MacroPending 和恢复后的
/// MacroPending 不会形成两套 VM 驱动规则。调用者仍负责进入 Passage 以及执行
/// Init／Start；传入的检查点必须位于这次导航开始之前。
pub struct EngineMirBeginTransaction<'hir, 'source> {
    /// 本次执行链的运行时身份。
    pub identity: RuntimeExecutionIdentity,
    /// 起始 Bytecode Passage 的 VM 执行帧。
    pub frame: crate::vm::MirExecutionFrame,
    /// 本次导航开始前的 State 检查点。
    pub state_checkpoint: StateCheckpoint,
    /// 本次导航开始前的 Story 快照。
    pub story_snapshot: StorySnapshot<'hir, 'source>,
    /// 起始时尚未确认的 Story 请求。
    pub requests: StoryRuntimePending<'hir, 'source>,
    /// 起始 Passage 的导航进度。
    pub progress: EngineMirProgress<'hir, 'source>,
}

/// 用首次导航已建立的事务组件启动 MIR，并运行到第一个稳定边界。
pub fn begin_mir_transaction<'hir, 'source>(
    transaction: EngineMirBeginTransaction<'hir, 'source>,
    mir: &BytecodeProgram,
    state: &mut State,
    story: &Story<'hir, 'source>,
) -> Result<EngineMirVmResume<'hir, 'source>, EngineMirVmResumeError<'hir, 'source>> {
    let EngineMirBeginTransaction {
        identity,
        frame,
        state_checkpoint,
        story_snapshot,
        requests,
        progress,
    } = transaction;
    EngineMirResumedTransaction {
        runtime: RuntimeMacroResumed {
            identity,
            frame,
            control: BodyControl::Continue,
            includes_entered: 0,
            scopes: MacroLocalScopes::new(),
        },
        state_checkpoint,
        story_snapshot,
        requests,
        progress,
    }
    .continue_vm(mir, state, story)
}

/// 异步恢复链完成后提交给 Host 的导航结果与外层 Macro scopes。
pub struct EngineMirCommitted<'hir, 'source> {
    /// 提交成功的导航链与累积输出。
    pub navigation: EngineNavigationChain<'hir, 'source>,
    /// 提交后仍由外层持有的 Macro 局部作用域。
    pub scopes: MacroLocalScopes<Value>,
}

impl<'hir, 'source> EngineMirVmResume<'hir, 'source> {
    /// 只提交 Halted 边界；其他边界必须先由对应控制器继续处理。
    pub fn commit_halted<LifecycleError>(
        self,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mut lifecycle: impl FnMut(
            PassageLifecyclePhase,
            PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), LifecycleError>,
    ) -> Result<
        EngineMirCommitted<'hir, 'source>,
        EngineMirCommitError<'hir, 'source, LifecycleError>,
    > {
        let Self::Halted(transaction) = self else {
            return Err(EngineMirCommitError::NotHalted(Box::new(self)));
        };
        navigation::commit_halted_transaction(transaction, state, story, &mut lifecycle)
            .map_err(|failure| EngineMirCommitError::Failed(Box::new(failure)))
    }

    /// 处理 NavigationPending／StopPassage，并在目标 Passage 继续同一事务。
    pub fn continue_navigation<LifecycleError>(
        self,
        mir: &BytecodeProgram,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mut lifecycle: impl FnMut(
            PassageLifecyclePhase,
            PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), LifecycleError>,
    ) -> Result<
        EngineMirVmResume<'hir, 'source>,
        EngineMirNavigationResumeError<'hir, 'source, LifecycleError>,
    > {
        let transaction = match self {
            Self::NavigationPending(transaction) | Self::PassageStopped(transaction) => transaction,
            other => {
                return Err(EngineMirNavigationResumeError::NotNavigation(Box::new(
                    other,
                )));
            }
        };
        navigation::continue_navigation_transaction(transaction, mir, state, story, &mut lifecycle)
    }

    /// 把恢复链遇到的下一动态 Macro 交回统一控制器。
    pub fn dispatch_macro<Pending, DispatchError>(
        self,
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
        let Self::MacroPending(transaction) = self else {
            return Err(EngineMirMacroDispatchError::NotMacro(Box::new(self)));
        };
        macro_dispatch::dispatch_macro_transaction(transaction, mir, state, story, dispatch)
    }
}

mod macro_dispatch;
mod navigation;
mod resume;

pub use macro_dispatch::*;
pub use navigation::*;
pub use resume::*;

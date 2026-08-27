//! Host 向 Narrava Core 提交动作并取得语义更新的最小边界。

use std::collections::HashMap;

mod diagnostics;

use diagnostics::*;

use crate::{
    bytecode::BytecodeProgram,
    diagnostic::{Diagnostic, DiagnosticSeverity},
    engine::{
        Engine, EngineExecutionLimits, EngineMacroInteractionBoundary,
        EngineMacroInteractionContinuation, EngineMacroInteractionDispatch,
        EngineMacroInteractionDispatchError, EngineMacroInteractionResume,
        EngineMacroInteractionResumeError, EngineMacroInteractionResumed,
        EngineMacroInteractionTargetError, EngineMirBeginCheckpointRequest, EngineMirBeginError,
        EngineMirBeginExecutionError, EngineMirBeginRequest, EngineMirCommitError,
        EngineMirCommitFailureKind, EngineMirContinuation, EngineMirContinuationResume,
        EngineMirContinuationResumeError, EngineMirMacroCallbackFailure, EngineMirMacroDispatch,
        EngineMirMacroDispatchError, EngineMirMacroInvocation, EngineMirNavigationFailureKind,
        EngineMirNavigationResumeError, EngineMirResumedTransaction, EngineMirVmResume,
        EngineMirVmResumeError, EngineNavigationError, EngineRequestedExecutionError, EngineStart,
        EngineStartError,
    },
    expression::value::Value,
    hir::HirPassage,
    i18n::I18nRuntimeLanguage,
    macro_runtime::{
        MacroHandlerOutcome, MacroInteraction, MacroInteractions, MacroLocalScopes,
        MacroResumeOutcome,
    },
    protocol::{InteractionId, Surface},
    runtime::{
        BodyExecution, RuntimeExecutionIdentity, RuntimeExecutionLocation,
        RuntimeMacroContinuationError, RuntimeMacroExecution,
    },
    state::{State, StateCheckpoint},
    story::{
        Story, StoryNavigationError, StoryRuntimeRequestError, StoryRuntimeRequests, StorySnapshot,
    },
    vm::MirExecutionError,
};

/// Host 交回 Core 的平台无关玩家动作。
///
/// 当前只开放 Passage 导航；新增输入语义时扩展此枚举，不接收平台回调对象。
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HostInput {
    /// 玩家激活上一份 Surface 中的交互（导航或按钮）。
    Activate { interaction: InteractionId },
    /// 恢复 Host 先前保存的异步执行。
    Resume { execution: HostExecutionToken },
    /// 取消等待并回滚其 Engine 事务。
    Cancel { execution: HostExecutionToken },
}

impl HostInput {
    /// 回送 Core 在上一份 Surface 中提供的交互身份。
    pub fn activate(interaction: InteractionId) -> Self {
        Self::Activate { interaction }
    }

    /// 请求恢复 Host 先前保存的异步执行；令牌本身不包含 continuation。
    pub fn resume(execution: HostExecutionToken) -> Self {
        Self::Resume { execution }
    }

    /// 请求取消等待并回滚其 Engine 事务。
    pub fn cancel(execution: HostExecutionToken) -> Self {
        Self::Cancel { execution }
    }
}

/// Host 用于路由异步输入的稳定执行令牌。
///
/// 它只复用 Runtime 执行身份，不携带 State、VM frame、平台句柄或局部作用域。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HostExecutionToken(RuntimeExecutionIdentity);

impl HostExecutionToken {
    /// 从 Engine 执行身份建立令牌。
    pub fn from_identity(identity: RuntimeExecutionIdentity) -> Self {
        Self(identity)
    }

    /// 取回底层执行身份。
    pub fn identity(self) -> RuntimeExecutionIdentity {
        self.0
    }
}

/// Host 启动或进入一条 LIR Passage 链所需的显式输入。
#[derive(Clone, Copy, Debug)]
pub struct HostMirRequest<'params> {
    pub params: &'params Value,
    pub identity: RuntimeExecutionIdentity,
    pub limits: EngineExecutionLimits,
    pub language: Option<&'params I18nRuntimeLanguage>,
}

/// 一次玩家输入及其新执行链参数。
#[derive(Clone, Debug)]
pub struct HostMirAdvanceRequest<'update, 'params> {
    pub presented: &'update HostUpdate,
    pub input: HostInput,
    pub params: &'params Value,
    pub identity: RuntimeExecutionIdentity,
    pub limits: EngineExecutionLimits,
    pub language: Option<&'params I18nRuntimeLanguage>,
}

/// 启动或进入一条 LIR Passage 链所需的内部请求。
struct HostMirEntryRequest<'params> {
    name: &'params str,
    params: &'params Value,
    identity: RuntimeExecutionIdentity,
    limits: EngineExecutionLimits,
    language: Option<&'params I18nRuntimeLanguage>,
}

/// Binding 后端按执行令牌独占保存的待处理所有权值。
///
/// `Pending` 通常是 `EngineMirContinuation`，也可以是 Binding 自己在其外层组合的
/// 调度状态。容器不读取该值，因此不依赖某个平台的异步模型。
#[derive(Debug)]
pub struct HostPendingExecutions<Pending> {
    entries: HashMap<HostExecutionToken, Pending>,
}

/// Host 保存的异步 Interaction；ID 与 Engine continuation 必须一起转移。
pub struct HostMacroInteractionPending<'hir, 'source, Pending> {
    interaction: InteractionId,
    continuation: EngineMacroInteractionContinuation<'hir, 'source, Pending>,
}

impl<'hir, 'source, Pending> HostMacroInteractionPending<'hir, 'source, Pending> {
    /// 把交互身份与 Engine continuation 绑定保存。
    pub fn new(
        interaction: InteractionId,
        continuation: EngineMacroInteractionContinuation<'hir, 'source, Pending>,
    ) -> Self {
        Self {
            interaction,
            continuation,
        }
    }

    /// 待处理交互的身份。
    pub fn interaction(&self) -> &InteractionId {
        &self.interaction
    }

    /// 对应的 Engine continuation。
    pub fn continuation(&self) -> &EngineMacroInteractionContinuation<'hir, 'source, Pending> {
        &self.continuation
    }
}

/// Interaction Handler 恢复后，Host 继续等待或取得可驱动正文事务。
pub enum HostMacroInteractionResume<'hir, 'source> {
    Pending { execution: HostExecutionToken },
    Continue(Box<HostMacroInteractionResumed<'hir, 'source>>),
}

/// 已恢复、Host 可驱动正文事务的 Interaction 状态。
pub struct HostMacroInteractionResumed<'hir, 'source> {
    execution: HostExecutionToken,
    interaction: InteractionId,
    transaction: EngineMacroInteractionResumed<'hir, 'source>,
}

/// Interaction 驱动期间由 Binding 后端持有的可变所有权集合。
pub struct HostMacroInteractionDriveContext<'host, 'hir, 'source, Pending> {
    pub interaction_pending:
        &'host mut HostPendingExecutions<HostMacroInteractionPending<'hir, 'source, Pending>>,
    pub passage_pending:
        &'host mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
    pub interactions: &'host mut MacroInteractions<'hir, 'source>,
}

impl<'host, 'hir, 'source, Pending>
    HostMacroInteractionDriveContext<'host, 'hir, 'source, Pending>
{
    /// 绑定交互、Passage 与互动注册表三份可变所有权。
    pub fn new(
        interaction_pending: &'host mut HostPendingExecutions<
            HostMacroInteractionPending<'hir, 'source, Pending>,
        >,
        passage_pending: &'host mut HostPendingExecutions<
            EngineMirContinuation<'hir, 'source, Pending>,
        >,
        interactions: &'host mut MacroInteractions<'hir, 'source>,
    ) -> Self {
        Self {
            interaction_pending,
            passage_pending,
            interactions,
        }
    }
}

impl HostMacroInteractionResumed<'_, '_> {
    /// 该执行链的令牌。
    pub fn execution(&self) -> HostExecutionToken {
        self.execution
    }

    /// 恢复的交互身份。
    pub fn interaction(&self) -> &InteractionId {
        &self.interaction
    }

    /// 可驱动的事务。
    pub fn transaction(&self) -> &EngineMacroInteractionResumed<'_, '_> {
        &self.transaction
    }
}

impl<Pending> HostPendingExecutions<Pending> {
    /// 建立空集合。
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// 保存一个待处理执行；同一 Token 已存在时拒绝覆盖并返还新值。
    pub fn add(
        &mut self,
        token: HostExecutionToken,
        pending: Pending,
    ) -> Result<(), HostPendingAddError<Pending>> {
        if self.entries.contains_key(&token) {
            return Err(HostPendingAddError { token, pending });
        }
        let previous: Option<Pending> = self.entries.insert(token, pending);
        debug_assert!(previous.is_none());
        Ok(())
    }

    /// 该令牌是否已有保存的执行。
    pub fn has(&self, token: HostExecutionToken) -> bool {
        self.entries.contains_key(&token)
    }

    /// 已保存的执行数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否不含任何保存的执行。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 取走所有权后才能恢复或取消，防止同一 continuation 被并发消费两次。
    pub fn take(&mut self, token: HostExecutionToken) -> Option<Pending> {
        self.entries.remove(&token)
    }
}

impl<Pending> Default for HostPendingExecutions<Pending> {
    fn default() -> Self {
        Self::new()
    }
}

/// 重复 Token 插入失败；新传入的所有权值不会被丢弃。
#[derive(Debug)]
pub struct HostPendingAddError<Pending> {
    pub token: HostExecutionToken,
    pub pending: Pending,
}

/// Binding 可读取但不能通过该接口改写的 State 视图。
///
/// 视图只在借用期间有效；需要跨 FFI 或异步边界保存数据时，由 Binding 显式转换
/// 为自己的传输结构，不能持有 Core 内部引用。
#[derive(Clone, Copy)]
pub struct HostStateView<'state> {
    state: &'state State,
}

impl<'state> HostStateView<'state> {
    /// 借用 State 建立只读视图；仅 crate 内部使用。
    fn new(state: &'state State) -> Self {
        Self { state }
    }

    /// 读取 scripts 通过 State API 导入的普通全局名称。
    pub fn global(&self, name: &str) -> Option<&'state Value> {
        self.state.global_get(name)
    }

    /// 读取启动配置共用的 setup 根对象。
    pub fn setup(&self) -> &'state Value {
        self.state.setup_get()
    }

    /// 读取 Twee `$name` 对应的持久变量。
    pub fn variable(&self, name: &str) -> Option<&'state Value> {
        self.state.variables_get(name)
    }

    /// 读取 Twee `_name` 对应的临时变量。
    pub fn temporary(&self, name: &str) -> Option<&'state Value> {
        self.state.temporary_get(name)
    }
}

/// 一次 Host API 调用后可供宿主消费的最小 Core 更新。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostUpdate {
    current: String,
    surface: Surface,
}

impl HostUpdate {
    /// 用当前 Passage 名与语义输出建立更新；仅 crate 内部使用。
    pub(crate) fn new(current: &str, surface: Surface) -> Self {
        Self {
            current: current.to_owned(),
            surface,
        }
    }

    /// 本次执行结束后的确切 PassageName。
    pub fn current(&self) -> &str {
        &self.current
    }

    /// 本次调用按执行顺序产生的宿主无关语义输出。
    pub fn surface(&self) -> &Surface {
        &self.surface
    }

    /// 把独立渲染的 Host 区域附加到本次正文更新。
    pub fn append_region(&mut self, region: crate::protocol::RegionId, content: Surface) {
        self.surface
            .push(crate::protocol::SurfaceNode::Region { region, content });
    }
}

/// 不保存领域状态的最小 Host API 协调入口。
pub struct HostApi;

mod api_drive;
mod api_entry;
mod api_legacy;
mod api_pending;

/// 取消成功后交还 Binding 释放的平台 Pending 所有权。
#[derive(Debug, PartialEq)]
pub struct HostCancelled<Pending> {
    pub execution: HostExecutionToken,
    pub pending: Pending,
}

/// 取消失败的稳定诊断；已取出的平台 Pending 仍尽可能返还给 Binding。
#[derive(Debug, PartialEq)]
pub struct HostCancelError<Pending> {
    pub diagnostic: Diagnostic,
    pub pending: Option<Pending>,
}

/// 当前 Handler 恢复后，Host 需要等待或继续驱动 Engine。
pub enum HostResumeOutcome<'hir, 'source> {
    Pending { execution: HostExecutionToken },
    Continue(Box<HostResumed<'hir, 'source>>),
}

/// Host 不透明持有的已恢复 Engine 事务；内部状态不提供给平台 Renderer。
pub struct HostResumed<'hir, 'source> {
    execution: HostExecutionToken,
    transaction: EngineMirResumedTransaction<'hir, 'source>,
}

impl<'hir, 'source> HostResumed<'hir, 'source> {
    /// 该事务的执行令牌。
    pub fn execution(&self) -> HostExecutionToken {
        self.execution
    }

    /// 当前执行的运行时位置。
    pub fn location(&self) -> RuntimeExecutionLocation {
        RuntimeExecutionLocation::new(
            self.transaction.runtime.identity,
            self.transaction.runtime.frame.location(),
        )
    }
}

/// Host 可检查的稳定边界类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostStableBoundary {
    /// 执行链正常结束，可提交或导航。
    Halted,
    /// 等待确认 Passage 导航。
    NavigationPending,
    /// 等待 Macro 互动结果。
    MacroPending,
    /// Passage 正文停止，等待下一段执行。
    PassageStopped,
}

/// Host 可检查边界类型，但不能取得内部 Engine 事务。
pub struct HostStable<'hir, 'source> {
    execution: HostExecutionToken,
    boundary: EngineMirVmResume<'hir, 'source>,
}

/// 提交失败：边界不是 Halted，或事务回滚失败。
pub enum HostCommitError<'hir, 'source> {
    NotHalted(Box<HostStable<'hir, 'source>>),
    Failed(Diagnostic),
}

/// 导航请求失败：边界不是 NavigationPending，或导航失败。
pub enum HostNavigationError<'hir, 'source> {
    NotNavigation(Box<HostStable<'hir, 'source>>),
    Failed(Diagnostic),
}

/// Macro 分发结果：等待异步 Handler 或继续驱动稳定事务。
pub enum HostMacroDispatch<'hir, 'source> {
    Pending { execution: HostExecutionToken },
    Continue(Box<HostStable<'hir, 'source>>),
}

/// Macro 分发失败：边界不是 MacroPending，或分发失败并尽可能返还 Pending。
pub enum HostMacroDispatchError<'hir, 'source, Pending> {
    NotMacro(Box<HostStable<'hir, 'source>>),
    Failed {
        diagnostic: Diagnostic,
        pending: Option<Pending>,
    },
}

/// Binding 驱动一次恢复链后只需处理“可呈现”或“等待平台任务”两种状态。
pub enum HostDriveResult {
    Ready(HostUpdate),
    Pending { execution: HostExecutionToken },
}

/// 驱动失败时保留稳定诊断；非法异步结果还会返还平台 Pending 所有权。
#[derive(Debug, PartialEq)]
pub struct HostDriveError<Pending> {
    pub diagnostic: Diagnostic,
    pub pending: Option<Pending>,
}

/// 一次异步恢复所需的三个独立回调边界。
pub struct HostResumeCallbacks<Lifecycle, Resume, Dispatch> {
    lifecycle: Lifecycle,
    resume: Resume,
    dispatch: Dispatch,
}

impl<Lifecycle, Resume, Dispatch> HostResumeCallbacks<Lifecycle, Resume, Dispatch> {
    /// 绑定三个回调边界。
    pub fn new(lifecycle: Lifecycle, resume: Resume, dispatch: Dispatch) -> Self {
        Self {
            lifecycle,
            resume,
            dispatch,
        }
    }
}

impl<'hir, 'source> HostStable<'hir, 'source> {
    /// 该稳定状态对应的执行令牌。
    pub fn execution(&self) -> HostExecutionToken {
        self.execution
    }

    /// 稳定边界的类型。
    pub fn boundary(&self) -> HostStableBoundary {
        match &self.boundary {
            EngineMirVmResume::Halted(_) => HostStableBoundary::Halted,
            EngineMirVmResume::NavigationPending(_) => HostStableBoundary::NavigationPending,
            EngineMirVmResume::MacroPending(_) => HostStableBoundary::MacroPending,
            EngineMirVmResume::PassageStopped(_) => HostStableBoundary::PassageStopped,
        }
    }

    /// 当前执行的运行时位置。
    pub fn location(&self) -> RuntimeExecutionLocation {
        let transaction: &EngineMirResumedTransaction<'_, '_> = match &self.boundary {
            EngineMirVmResume::Halted(transaction)
            | EngineMirVmResume::NavigationPending(transaction)
            | EngineMirVmResume::MacroPending(transaction)
            | EngineMirVmResume::PassageStopped(transaction) => transaction,
        };
        RuntimeExecutionLocation::new(
            transaction.runtime.identity,
            transaction.runtime.frame.location(),
        )
    }
}

//! Host 向 Narrava Core 提交动作并取得语义更新的最小边界。

use std::collections::HashMap;

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
    presentation::{InteractionId, PresentationOutput},
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
    Activate { interaction: InteractionId },
    Resume { execution: HostExecutionToken },
    Cancel { execution: HostExecutionToken },
}

impl HostInput {
    /// 回送 Core 在上一份 Presentation 中提供的交互身份。
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
    pub fn from_identity(identity: RuntimeExecutionIdentity) -> Self {
        Self(identity)
    }

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
    pub fn new(
        interaction: InteractionId,
        continuation: EngineMacroInteractionContinuation<'hir, 'source, Pending>,
    ) -> Self {
        Self {
            interaction,
            continuation,
        }
    }

    pub fn interaction(&self) -> &InteractionId {
        &self.interaction
    }

    pub fn continuation(&self) -> &EngineMacroInteractionContinuation<'hir, 'source, Pending> {
        &self.continuation
    }
}

/// Interaction Handler 恢复后，Host 继续等待或取得可驱动正文事务。
pub enum HostMacroInteractionResume<'hir, 'source> {
    Pending { execution: HostExecutionToken },
    Continue(Box<HostMacroInteractionResumed<'hir, 'source>>),
}

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
    pub fn execution(&self) -> HostExecutionToken {
        self.execution
    }

    pub fn interaction(&self) -> &InteractionId {
        &self.interaction
    }

    pub fn transaction(&self) -> &EngineMacroInteractionResumed<'_, '_> {
        &self.transaction
    }
}

impl<Pending> HostPendingExecutions<Pending> {
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

    pub fn has(&self, token: HostExecutionToken) -> bool {
        self.entries.contains_key(&token)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

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
    presentation: PresentationOutput,
}

impl HostUpdate {
    pub(crate) fn new(current: &str, presentation: PresentationOutput) -> Self {
        Self {
            current: current.to_owned(),
            presentation,
        }
    }

    /// 本次执行结束后的确切 PassageName。
    pub fn current(&self) -> &str {
        &self.current
    }

    /// 本次调用按执行顺序产生的宿主无关语义输出。
    pub fn presentation(&self) -> &PresentationOutput {
        &self.presentation
    }

    /// 把独立渲染的 Host 区域附加到本次正文更新。
    pub fn append_region(
        &mut self,
        region: crate::presentation::PresentationRegion,
        content: PresentationOutput,
    ) {
        self.presentation
            .push(crate::presentation::PresentationNode::Region { region, content });
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
    pub fn execution(&self) -> HostExecutionToken {
        self.execution
    }

    pub fn location(&self) -> RuntimeExecutionLocation {
        RuntimeExecutionLocation::new(
            self.transaction.runtime.identity,
            self.transaction.runtime.frame.location(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostStableBoundary {
    Halted,
    NavigationPending,
    MacroPending,
    PassageStopped,
}

/// Host 可检查边界类型，但不能取得内部 Engine 事务。
pub struct HostStable<'hir, 'source> {
    execution: HostExecutionToken,
    boundary: EngineMirVmResume<'hir, 'source>,
}

pub enum HostCommitError<'hir, 'source> {
    NotHalted(Box<HostStable<'hir, 'source>>),
    Failed(Diagnostic),
}

pub enum HostNavigationError<'hir, 'source> {
    NotNavigation(Box<HostStable<'hir, 'source>>),
    Failed(Diagnostic),
}

pub enum HostMacroDispatch<'hir, 'source> {
    Pending { execution: HostExecutionToken },
    Continue(Box<HostStable<'hir, 'source>>),
}

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
    pub fn new(lifecycle: Lifecycle, resume: Resume, dispatch: Dispatch) -> Self {
        Self {
            lifecycle,
            resume,
            dispatch,
        }
    }
}

impl<'hir, 'source> HostStable<'hir, 'source> {
    pub fn execution(&self) -> HostExecutionToken {
        self.execution
    }

    pub fn boundary(&self) -> HostStableBoundary {
        match &self.boundary {
            EngineMirVmResume::Halted(_) => HostStableBoundary::Halted,
            EngineMirVmResume::NavigationPending(_) => HostStableBoundary::NavigationPending,
            EngineMirVmResume::MacroPending(_) => HostStableBoundary::MacroPending,
            EngineMirVmResume::PassageStopped(_) => HostStableBoundary::PassageStopped,
        }
    }

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

/// Host 边界只暴露稳定诊断，不泄漏 Engine 的事务错误结构。
fn host_error(code: &str, message: &str) -> Diagnostic {
    Diagnostic::new(code, DiagnosticSeverity::Error, message)
}

/// 拆出可回滚的事务和真正的失败原因。
///
/// Host 可以改写“回滚也失败”，但回滚成功时必须保留这份诊断，
/// 不得用第二条泛化的“继续执行失败”覆盖它。
fn mir_resume_failure<'hir, 'source>(
    error: EngineMirVmResumeError<'hir, 'source>,
) -> (EngineMirResumedTransaction<'hir, 'source>, Diagnostic) {
    match error {
        EngineMirVmResumeError::Story(transaction) => (
            *transaction,
            host_error("engine.story.failed", "Story 运行时请求失败"),
        ),
        EngineMirVmResumeError::StoryRequest { error, transaction } => {
            let diagnostic = match error {
                StoryRuntimeRequestError::Navigation(error) => story_navigation_diagnostic(error),
                StoryRuntimeRequestError::GotoAlreadyPending => {
                    host_error("story.goto.already_pending", "已有未消费的 goto 请求")
                }
            };
            (*transaction, diagnostic)
        }
        EngineMirVmResumeError::Vm { error, transaction } => {
            let diagnostic = match error {
                MirExecutionError::Evaluation(error) => error.diagnostic(),
                MirExecutionError::MissingPassage => {
                    host_error("engine.vm.missing_passage", "VM 找不到当前 Passage")
                }
                MirExecutionError::DifferentI18nCatalog => host_error(
                    "engine.vm.different_i18n_catalog",
                    "VM 语言目录与当前编译结果不匹配",
                ),
                MirExecutionError::InvalidInstructionPointer => {
                    host_error("engine.vm.invalid_instruction_pointer", "VM 指令位置无效")
                }
                MirExecutionError::MissingValueSlot(_) => {
                    host_error("engine.vm.missing_value_slot", "VM 值槽不存在")
                }
                MirExecutionError::MissingIteratorSlot(_) => {
                    host_error("engine.vm.missing_iterator_slot", "VM 迭代器槽不存在")
                }
                MirExecutionError::InvalidText(_) => {
                    host_error("engine.vm.invalid_text", "VM 文本指令无效")
                }
                MirExecutionError::ExpectedMacroPending => {
                    host_error("engine.vm.expected_macro_pending", "VM 期待 Macro 暂停边界")
                }
                MirExecutionError::MacroBodyIncludeUnsupported => host_error(
                    "engine.vm.macro_body_include_unsupported",
                    "Macro 正文尚不支持 include",
                ),
            };
            (*transaction, diagnostic)
        }
        EngineMirVmResumeError::IncludeLimitExceeded { limit, transaction } => (
            *transaction,
            host_error(
                "engine.include.limit_exceeded",
                &format!("include 深度超过限制：{limit}"),
            ),
        ),
        EngineMirVmResumeError::UnexpectedMacroControl {
            control,
            transaction,
        } => (
            *transaction,
            host_error(
                "engine.macro.unexpected_control",
                &format!("Macro 返回了当前边界不接受的控制信号：{control:?}"),
            ),
        ),
    }
}

fn mir_begin_diagnostic<'hir, 'source>(
    error: EngineMirBeginError<'hir, 'source, Diagnostic>,
    state: &mut State,
    story: &mut Story<'hir, 'source>,
) -> Diagnostic {
    match error {
        EngineMirBeginError::Preparation(error) => match error {
            EngineNavigationError::Navigation(error) => story_navigation_diagnostic(error),
            EngineNavigationError::Rollback { .. } => host_error(
                "engine.rollback.failed",
                "LIR Passage 启动失败，且 Story 检查点无法恢复",
            ),
            EngineNavigationError::Execution(error) => match error {
                EngineRequestedExecutionError::Runtime(
                    EngineMirBeginExecutionError::MissingMirPassage(name),
                ) => host_error(
                    "engine.mir.missing_passage",
                    &format!("MIR 中缺少 Passage：{name}"),
                ),
                EngineRequestedExecutionError::Runtime(
                    EngineMirBeginExecutionError::Lifecycle(error),
                )
                | EngineRequestedExecutionError::Lifecycle {
                    error: EngineMirBeginExecutionError::Lifecycle(error),
                    ..
                } => error,
                EngineRequestedExecutionError::Lifecycle {
                    error: EngineMirBeginExecutionError::MissingMirPassage(name),
                    ..
                } => host_error(
                    "engine.mir.missing_passage",
                    &format!("MIR 中缺少 Passage：{name}"),
                ),
                EngineRequestedExecutionError::PassageLimitExceeded { limit } => host_error(
                    "engine.execution.passage_limit_exceeded",
                    &format!("单次事务执行的 Passage 数量超过限制：{limit}"),
                ),
                _ => host_error(
                    "engine.mir.begin_failed",
                    "LIR Passage 启动请求不符合 Engine 协议",
                ),
            },
        },
        EngineMirBeginError::Continue(error) => {
            let (transaction, diagnostic) = mir_resume_failure(*error);
            let rollback_failed: bool = transaction.rollback(state, story).is_err();
            if rollback_failed {
                host_error(
                    "engine.rollback.failed",
                    "LIR Passage 启动失败，且 Story 检查点无法恢复",
                )
            } else {
                diagnostic
            }
        }
    }
}

fn story_navigation_diagnostic(error: StoryNavigationError) -> Diagnostic {
    let code: &str = match &error {
        StoryNavigationError::MissingPassage(_) => "story.navigation.missing_passage",
        StoryNavigationError::SpecialPassage(_) => "story.navigation.special_passage",
        StoryNavigationError::DifferentStoryRequest => "story.navigation.different_story",
        StoryNavigationError::HistoryIdExhausted => "story.history.id_exhausted",
    };
    host_error(code, &error.to_string())
}

fn execution_diagnostic(error: EngineRequestedExecutionError<Diagnostic>) -> Diagnostic {
    match error {
        EngineRequestedExecutionError::Runtime(error)
        | EngineRequestedExecutionError::Lifecycle { error, .. } => error,
        EngineRequestedExecutionError::MissingGotoRequest => host_error(
            "engine.goto.missing_request",
            "Runtime 请求跳转，但 Story 中没有待处理的 goto 请求",
        ),
        EngineRequestedExecutionError::UnexpectedGotoRequest => host_error(
            "engine.goto.unexpected_request",
            "Story 留有 goto 请求，但 Runtime 没有请求跳转",
        ),
        EngineRequestedExecutionError::StoryInitGotoUnsupported => host_error(
            "engine.story_init.goto_unsupported",
            "StoryInit 初始化阶段不能请求 Passage 跳转",
        ),
        EngineRequestedExecutionError::UnexpectedControl(_) => host_error(
            "engine.control.unexpected",
            "Passage 顶层返回了当前 Engine 阶段不接受的控制信号",
        ),
        EngineRequestedExecutionError::Confirmation(error) => story_navigation_diagnostic(error),
        EngineRequestedExecutionError::PassageLimitExceeded { limit } => host_error(
            "engine.execution.passage_limit_exceeded",
            &format!("单次事务执行的 Passage 数量超过限制：{limit}"),
        ),
        EngineRequestedExecutionError::UnconsumedIncludeRequests { count } => host_error(
            "engine.include.unconsumed_requests",
            &format!("Runtime 结束时仍有未消费的 include 请求：{count}"),
        ),
    }
}

fn navigation_diagnostic(
    error: EngineNavigationError<EngineRequestedExecutionError<Diagnostic>>,
) -> Diagnostic {
    match error {
        EngineNavigationError::Navigation(error) => story_navigation_diagnostic(error),
        EngineNavigationError::Execution(error) => execution_diagnostic(error),
        EngineNavigationError::Rollback { .. } => host_error(
            "engine.rollback.failed",
            "Passage 事务失败，且 Story 检查点无法恢复",
        ),
    }
}

fn start_diagnostic(error: EngineStartError<Diagnostic>) -> Diagnostic {
    match error {
        EngineStartError::AlreadyStarted { current } => host_error(
            "engine.start.already_started",
            &format!("Story 已经启动，当前位置为：{current}"),
        ),
        EngineStartError::Execution(error) => navigation_diagnostic(error),
        EngineStartError::Rollback { .. } => host_error(
            "engine.rollback.failed",
            "Story 启动失败，且 Story 检查点无法恢复",
        ),
    }
}

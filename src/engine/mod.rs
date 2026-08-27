//! Engine 的跨控制器运行事务。
//!
//! 结果类型与事务辅助在本模块，导航事务在 `navigation`，启动与生命周期
//! 事务在 `lifecycle`。

mod continuation;
mod lifecycle;
mod navigation;
mod vm;

pub use continuation::*;
pub use vm::*;

use crate::{
    expression::value::Value,
    hir::HirPassage,
    runtime::{BodyControl, BodyExecution},
    semantic::{InteractionId, SemanticNode, SemanticOutput},
    state::{State, StateCheckpoint, StateReset},
    story::{
        Story, StoryHistoryEntry, StoryNavigationError, StoryNavigationRequest,
        StoryRuntimeRequests, StorySnapshot, StorySnapshotError,
    },
};

/// 单个导航 Passage 从准备到离开的稳定生命周期阶段。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassageLifecyclePhase {
    Init,
    Start,
    Render,
    Display,
    End,
}

impl PassageLifecyclePhase {
    /// 阶段顺序由 Engine 固定，宿主不能交换 render 与 display。
    pub const ORDERED: [Self; 5] = [
        Self::Init,
        Self::Start,
        Self::Render,
        Self::Display,
        Self::End,
    ];
}

/// Passage 生命周期回调读取的身份与参数。
///
/// 参数保持只读；脚本需要持久化数据时，应显式写入 State。
/// Render／Display 阶段通过 `output()` 取得本跳 Passage 的有序语义输出。
#[derive(Clone, Copy, Debug)]
pub struct PassageLifecycleContext<'params, 'hir, 'source, 'output> {
    entry: StoryHistoryEntry<'hir, 'source>,
    params: &'params Value,
    output: Option<&'output SemanticOutput>,
}

impl<'params, 'hir, 'source, 'output> PassageLifecycleContext<'params, 'hir, 'source, 'output> {
    /// 建立 Init／Start 阶段上下文，此时尚无可读语义输出。
    pub fn new(
        entry: StoryHistoryEntry<'hir, 'source>,
        params: &'params Value,
    ) -> PassageLifecycleContext<'params, 'hir, 'source, 'static> {
        PassageLifecycleContext {
            entry,
            params,
            output: None,
        }
    }

    /// 携带本跳输出建立 Render／Display 阶段上下文。
    pub fn with_output(
        entry: StoryHistoryEntry<'hir, 'source>,
        params: &'params Value,
        output: &'output SemanticOutput,
    ) -> Self {
        PassageLifecycleContext {
            entry,
            params,
            output: Some(output),
        }
    }

    /// 当前 Passage 的历史条目（含 HIR 引用与来源）。
    pub fn entry(&self) -> StoryHistoryEntry<'hir, 'source> {
        self.entry
    }

    /// 本次导航或启动传入的入口参数。
    pub fn params(&self) -> &'params Value {
        self.params
    }

    /// 本跳 Passage 的有序语义输出；仅在携带输出的阶段可用。
    pub fn output(&self) -> Option<&'output SemanticOutput> {
        self.output
    }
}

/// 一次成功导航确认的历史项与执行输出。
#[derive(Debug, PartialEq)]
pub struct EngineNavigation<'hir, 'source, Output> {
    /// 已确认的导航目标历史条目。
    pub entry: StoryHistoryEntry<'hir, 'source>,
    /// 目标 Passage 执行产生的输出。
    pub output: Output,
}

/// 一次 Passage 执行及其可选 goto 请求的确认结果。
#[derive(Debug, PartialEq)]
pub struct EngineRequestedNavigation<'hir, 'source> {
    /// 本次实际进入并执行的 Passage。
    pub entered: StoryHistoryEntry<'hir, 'source>,
    /// 执行停止后确认的至多一个 goto 目标；没有请求时为 `None`。
    pub requested: Option<StoryHistoryEntry<'hir, 'source>>,
    /// 本跳 Passage 执行产生的有序语义输出。
    pub output: SemanticOutput,
}

/// 一条连续执行并确认完成的 Passage 历史链。
#[derive(Debug, PartialEq)]
pub struct EngineNavigationChain<'hir, 'source> {
    /// 按执行顺序确认的 Passage 历史链。
    pub entries: Vec<StoryHistoryEntry<'hir, 'source>>,
    /// 整条链按执行顺序累积的有序语义输出。
    pub output: SemanticOutput,
}

/// 单次 Engine 执行允许消耗的显式控制流预算。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineExecutionLimits {
    /// 单条导航链允许执行的 Passage 数量上限。
    pub passages: usize,
    /// 每条 Passage 允许展开的 include 数量上限。
    pub includes: usize,
}

/// Engine 首次启动后可供宿主检查的导航结果。
#[derive(Debug, PartialEq)]
pub struct EngineStart<'hir, 'source> {
    pub initial: StoryHistoryEntry<'hir, 'source>,
    pub current: StoryHistoryEntry<'hir, 'source>,
    pub entries: Vec<StoryHistoryEntry<'hir, 'source>>,
    /// 启动链按执行顺序累积的有序语义输出。
    pub output: SemanticOutput,
}

/// 新游戏重置范围及重新启动结果。
#[derive(Debug, PartialEq)]
pub struct EngineNewGame<'hir, 'source> {
    pub state: StateReset,
    pub history_removed: usize,
    pub start: EngineStart<'hir, 'source>,
}

/// 可选 StoryInit 是否在本次调用中实际执行。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineStoryInit {
    Missing,
    Executed,
}

/// 新游戏在旧 Passage 结束阶段或重新启动阶段的失败原因。
#[derive(Debug, PartialEq)]
pub enum EngineNewGameFailure<RuntimeError> {
    Lifecycle {
        phase: PassageLifecyclePhase,
        error: RuntimeError,
    },
    Start(EngineStartError<RuntimeError>),
}

/// 新游戏失败；若 Story 外层检查点也无法恢复，则同时保留恢复错误。
#[derive(Debug, PartialEq)]
pub enum EngineNewGameError<RuntimeError> {
    Execution(EngineNewGameFailure<RuntimeError>),
    Rollback {
        execution: EngineNewGameFailure<RuntimeError>,
        story: StorySnapshotError,
    },
}

/// Engine 启动前置条件或首次执行链失败。
#[derive(Debug, PartialEq)]
pub enum EngineStartError<RuntimeError> {
    AlreadyStarted {
        current: String,
    },
    Execution(EngineNavigationError<EngineRequestedExecutionError<RuntimeError>>),
    Rollback {
        execution: EngineNavigationError<EngineRequestedExecutionError<RuntimeError>>,
        story: StorySnapshotError,
    },
}

/// Runtime 控制信号与 pending goto 不一致时的执行阶段错误。
#[derive(Debug, PartialEq)]
pub enum EngineRequestedExecutionError<RuntimeError> {
    Runtime(RuntimeError),
    Lifecycle {
        phase: PassageLifecyclePhase,
        error: RuntimeError,
    },
    MissingGotoRequest,
    UnexpectedGotoRequest,
    StoryInitGotoUnsupported,
    UnexpectedControl(BodyControl),
    Confirmation(StoryNavigationError),
    PassageLimitExceeded {
        limit: usize,
    },
    UnconsumedIncludeRequests {
        count: usize,
    },
}

/// Engine 导航事务的稳定失败阶段。
#[derive(Debug, PartialEq)]
pub enum EngineNavigationError<ExecutionError> {
    Navigation(StoryNavigationError),
    Execution(ExecutionError),
    Rollback {
        execution: ExecutionError,
        story: StorySnapshotError,
    },
}

/// 不持有领域状态的 Engine 协调入口。
pub struct Engine;

impl Engine {
    /// 新游戏失败时恢复 State 检查点与 Story 快照，并包装回滚结果。
    fn rollback_new_game<'hir, 'source, RuntimeError>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        state_checkpoint: StateCheckpoint,
        story_snapshot: StorySnapshot<'hir, 'source>,
        execution: EngineNewGameFailure<RuntimeError>,
    ) -> EngineNewGameError<RuntimeError> {
        state.restore_checkpoint(state_checkpoint);
        match story.restore(story_snapshot) {
            Ok(()) => EngineNewGameError::Execution(execution),
            Err(story) => EngineNewGameError::Rollback { execution, story },
        }
    }

    /// 导航失败时恢复 State 检查点与 Story 快照，并包装回滚结果。
    fn rollback<'hir, 'source, ExecutionError>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        state_checkpoint: StateCheckpoint,
        story_snapshot: StorySnapshot<'hir, 'source>,
        execution: ExecutionError,
    ) -> EngineNavigationError<ExecutionError> {
        state.restore_checkpoint(state_checkpoint);
        match story.restore(story_snapshot) {
            Ok(()) => EngineNavigationError::Execution(execution),
            Err(story) => EngineNavigationError::Rollback { execution, story },
        }
    }
}

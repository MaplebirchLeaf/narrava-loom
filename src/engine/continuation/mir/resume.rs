//! MIR continuation 恢复失败的所有权类型与统一回滚入口。

use super::*;

/// 继续 VM 失败时保留仍可回滚的完整事务。
pub enum EngineMirVmResumeError<'hir, 'source> {
    /// Story 无法从待处理请求重新附着，事务保持原样。
    Story(Box<EngineMirResumedTransaction<'hir, 'source>>),
    /// Story 拒绝导航请求，事务保持原样。
    StoryRequest {
        error: StoryRuntimeRequestError,
        transaction: Box<EngineMirResumedTransaction<'hir, 'source>>,
    },
    /// VM 步进失败，事务保持原样。
    Vm {
        error: MirExecutionError,
        transaction: Box<EngineMirResumedTransaction<'hir, 'source>>,
    },
    /// 展开的 include 超过预算，事务保持原样。
    IncludeLimitExceeded {
        limit: usize,
        transaction: Box<EngineMirResumedTransaction<'hir, 'source>>,
    },
    /// Macro 返回了继续 VM 不支持的停止信号，事务保持原样。
    UnexpectedMacroControl {
        control: BodyControl,
        transaction: Box<EngineMirResumedTransaction<'hir, 'source>>,
    },
}

/// Engine 恢复当前 Macro 后得到完成事务，或携带新句柄再次暂停。
pub enum EngineMirContinuationResume<'hir, 'source, Pending> {
    Complete(EngineMirResumedTransaction<'hir, 'source>),
    Pending(EngineMirContinuation<'hir, 'source, Pending>),
}

/// Engine 恢复前后的所有权错误；失败状态仍足以由上层决定重试或回滚。
pub enum EngineMirContinuationResumeError<'hir, 'source, ResumeError, Pending> {
    Story(Box<EngineMirContinuation<'hir, 'source, Pending>>),
    Runtime(Box<EngineMirContinuationResumeFailure<'hir, 'source, ResumeError, Pending>>),
}

/// Handler 恢复失败时保留原始错误与仍可回滚的事务组件。
pub struct EngineMirContinuationResumeFailure<'hir, 'source, ResumeError, Pending> {
    /// Handler 或 VM 恢复失败的原始错误。
    pub error: RuntimeMacroContinuationResumeError<ResumeError, Pending>,
    pub(super) state_checkpoint: StateCheckpoint,
    pub(super) story_snapshot: StorySnapshot<'hir, 'source>,
    pub(super) requests: StoryRuntimePending<'hir, 'source>,
    pub(super) progress: EngineMirProgress<'hir, 'source>,
}

impl<'hir, 'source, ResumeError, Pending>
    EngineMirContinuationResumeFailure<'hir, 'source, ResumeError, Pending>
{
    /// 失败时尚未确认的 Story 请求。
    pub fn requests(&self) -> &StoryRuntimePending<'hir, 'source> {
        &self.requests
    }

    /// 失败时的 Passage 导航进度。
    pub fn progress(&self) -> &EngineMirProgress<'hir, 'source> {
        &self.progress
    }

    /// Handler／VM 恢复失败后回滚最初导航事务，并返还原始恢复错误。
    pub fn rollback(
        self,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
    ) -> Result<
        RuntimeMacroContinuationResumeError<ResumeError, Pending>,
        EngineMirContinuationFailureRollbackError<ResumeError, Pending>,
    > {
        state.restore_checkpoint(self.state_checkpoint);
        match story.restore(self.story_snapshot) {
            Ok(()) => Ok(self.error),
            Err(story) => Err(EngineMirContinuationFailureRollbackError {
                story,
                error: self.error,
            }),
        }
    }
}

/// 恢复失败后回滚时 Story 恢复失败；原始错误仍返还给调用者。
pub struct EngineMirContinuationFailureRollbackError<ResumeError, Pending> {
    /// Story 快照恢复失败的详情。
    pub story: StorySnapshotError,
    /// 原样交还的恢复错误。
    pub error: RuntimeMacroContinuationResumeError<ResumeError, Pending>,
}

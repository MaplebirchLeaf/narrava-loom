//! MIR continuation 恢复失败的所有权类型与统一回滚入口。

use super::*;

/// 继续 VM 失败时保留仍可回滚的完整事务。
pub enum EngineMirVmResumeError<'hir, 'source> {
    Story(Box<EngineMirResumedTransaction<'hir, 'source>>),
    StoryRequest {
        error: StoryRuntimeRequestError,
        transaction: Box<EngineMirResumedTransaction<'hir, 'source>>,
    },
    Vm {
        error: MirExecutionError,
        transaction: Box<EngineMirResumedTransaction<'hir, 'source>>,
    },
    IncludeLimitExceeded {
        limit: usize,
        transaction: Box<EngineMirResumedTransaction<'hir, 'source>>,
    },
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

pub struct EngineMirContinuationResumeFailure<'hir, 'source, ResumeError, Pending> {
    pub error: RuntimeMacroContinuationResumeError<ResumeError, Pending>,
    pub(super) state_checkpoint: StateCheckpoint,
    pub(super) story_snapshot: StorySnapshot<'hir, 'source>,
    pub(super) requests: StoryRuntimePending<'hir, 'source>,
    pub(super) progress: EngineMirProgress<'hir, 'source>,
}

impl<'hir, 'source, ResumeError, Pending>
    EngineMirContinuationResumeFailure<'hir, 'source, ResumeError, Pending>
{
    pub fn requests(&self) -> &StoryRuntimePending<'hir, 'source> {
        &self.requests
    }

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

pub struct EngineMirContinuationFailureRollbackError<ResumeError, Pending> {
    pub story: StorySnapshotError,
    pub error: RuntimeMacroContinuationResumeError<ResumeError, Pending>,
}

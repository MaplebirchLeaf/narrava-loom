//! Runtime 暂停与恢复所需的稳定身份与 VM 级组合状态。

use crate::{
    bytecode::{BytecodeMacroBody, BytecodeProgram},
    expression::value::Value,
    macro_runtime::{
        MacroHandlerOutcome, MacroLocalScopes, MacroResumeError, MacroResumeOutcome,
        MacroSuspension, resume_macro_suspension,
    },
    mir::{MirExecutionPosition, MirMacroBody},
    vm::{MirExecutionError, MirExecutionFrame},
};

use super::{BodyControl, RuntimeMacroExecution};

/// 一次 Story 运行实例及其内部执行链的联合身份。
///
/// Macro 暂停只借用这一身份；身份本身属于 Runtime，后续也会用于校验
/// VM continuation，避免把其他 Story 或执行链的暂停状态错误恢复到当前事务。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeExecutionIdentity {
    pub story: u64,
    pub chain: u64,
}

impl RuntimeExecutionIdentity {
    /// 组合 Story 与执行链编号，建立联合身份。
    pub fn new(story: u64, chain: u64) -> Self {
        Self { story, chain }
    }
}

/// Runtime 中一条执行链当前所在的 MIR 位置。
///
/// 该类型只回答“哪条执行链执行到哪里”，不包含输出、Macro 暂停、Story 请求
/// 或 Engine 检查点，因此不是完整 continuation。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeExecutionLocation {
    identity: RuntimeExecutionIdentity,
    position: MirExecutionPosition,
}

impl RuntimeExecutionLocation {
    /// 组合执行身份与当前位置。
    pub fn new(identity: RuntimeExecutionIdentity, position: MirExecutionPosition) -> Self {
        Self { identity, position }
    }

    /// 该位置所属的执行身份。
    pub fn identity(self) -> RuntimeExecutionIdentity {
        self.identity
    }

    /// 执行链中的 MIR 位置。
    pub fn position(self) -> MirExecutionPosition {
        self.position
    }
}

/// VM 与异步 Macro 暂停无法组成同一执行链的原因。
#[derive(Debug, PartialEq)]
pub enum RuntimeMacroContinuationError<Pending> {
    IdentityMismatch {
        expected: RuntimeExecutionIdentity,
        parts: Box<RuntimeMacroContinuationParts<Pending>>,
    },
    ExpectedMacroPending {
        identity: RuntimeExecutionIdentity,
        parts: Box<RuntimeMacroContinuationParts<Pending>>,
    },
}

/// continuation 构造失败时原样返还的两个所有权组件。
#[derive(Debug, PartialEq)]
pub struct RuntimeMacroContinuationParts<Pending> {
    pub frame: MirExecutionFrame,
    pub suspension: MacroSuspension<Pending>,
}

/// 停在动态 Macro 位置的一条 VM 执行链及其 Handler 暂停状态。
///
/// 这是 Runtime continuation 的 VM 级组成部分，不包含 Engine 检查点、
/// Passage 生命周期或待确认导航，因此尚不能直接暴露给 Host 恢复。
#[derive(Debug, PartialEq)]
pub struct RuntimeMacroContinuation<Pending> {
    identity: RuntimeExecutionIdentity,
    frame: MirExecutionFrame,
    suspension: MacroSuspension<Pending>,
}

/// 独立容器 Macro 正文停在动态 Macro 时的 Runtime continuation。
#[derive(Debug, PartialEq)]
pub struct RuntimeMacroBodyContinuation<Pending> {
    identity: RuntimeExecutionIdentity,
    frame: MirExecutionFrame,
    suspension: MacroSuspension<Pending>,
}

/// 独立容器 Macro 正文的异步 Macro 完成后交回 VM 与控制状态。
#[derive(Debug, PartialEq)]
pub struct RuntimeMacroBodyResumed {
    /// 暂停所属的执行身份。
    pub identity: RuntimeExecutionIdentity,
    /// 已完成 Macro 写入、等待继续驱动的 VM 帧。
    pub frame: MirExecutionFrame,
    /// Macro 请求的控制信号。
    pub control: BodyControl,
    /// Macro 调用期间展开的 include 数量（用于预算检查）。
    pub includes_entered: usize,
    /// 恢复后仍归外层持有的 Macro 局部作用域。
    pub scopes: MacroLocalScopes<Value>,
}

/// 容器 Macro 正文的 Handler 恢复完成，或携带新句柄再次暂停。
#[derive(Debug, PartialEq)]
pub enum RuntimeMacroBodyContinuationResume<Pending> {
    Complete(RuntimeMacroBodyResumed),
    Pending(RuntimeMacroBodyContinuation<Pending>),
}

/// 异步 Macro 完成后交回 Engine 的 VM 与控制状态。
#[derive(Debug, PartialEq)]
pub struct RuntimeMacroResumed {
    /// 暂停所属的执行身份。
    pub identity: RuntimeExecutionIdentity,
    /// 已完成 Macro 写入、等待继续驱动的 VM 帧。
    pub frame: MirExecutionFrame,
    /// Macro 请求的控制信号。
    pub control: BodyControl,
    /// Macro 调用期间展开的 include 数量（用于预算检查）。
    pub includes_entered: usize,
    /// 恢复后仍归外层持有的 Macro 局部作用域。
    pub scopes: MacroLocalScopes<Value>,
}

/// Handler 恢复完成，或携带新调度句柄再次暂停。
#[derive(Debug, PartialEq)]
pub enum RuntimeMacroContinuationResume<Pending> {
    Complete(RuntimeMacroResumed),
    Pending(RuntimeMacroContinuation<Pending>),
}

/// 恢复失败时保留尚未继续执行的 VM frame。
#[derive(Debug, PartialEq)]
pub enum RuntimeMacroContinuationResumeError<ResumeError, Pending> {
    /// Handler 恢复失败；frame 尚未推进。
    Macro {
        frame: Box<MirExecutionFrame>,
        error: MacroResumeError<ResumeError, Pending>,
    },
    /// 完成输出写入 VM 失败；frame 与作用域一并交还。
    Vm {
        frame: Box<MirExecutionFrame>,
        error: MirExecutionError,
        scopes: MacroLocalScopes<Value>,
    },
}

impl<Pending> RuntimeMacroContinuation<Pending> {
    /// 只允许相同执行身份、且确实停在 InvokeMacro 的状态组合。
    pub fn new(
        identity: RuntimeExecutionIdentity,
        frame: MirExecutionFrame,
        suspension: MacroSuspension<Pending>,
        story: &BytecodeProgram,
    ) -> Result<Self, RuntimeMacroContinuationError<Pending>> {
        if suspension.identity != identity {
            return Err(RuntimeMacroContinuationError::IdentityMismatch {
                expected: identity,
                parts: Box::new(RuntimeMacroContinuationParts { frame, suspension }),
            });
        }
        if frame.pending_macro(story).is_none() {
            return Err(RuntimeMacroContinuationError::ExpectedMacroPending {
                identity,
                parts: Box::new(RuntimeMacroContinuationParts { frame, suspension }),
            });
        }
        Ok(Self {
            identity,
            frame,
            suspension,
        })
    }

    /// 暂停所属的执行身份。
    pub fn identity(&self) -> RuntimeExecutionIdentity {
        self.identity
    }

    /// 暂停位置：身份加当前 VM 位置，不含任何状态。
    pub fn location(&self) -> RuntimeExecutionLocation {
        RuntimeExecutionLocation::new(self.identity, self.frame.location())
    }

    /// 暂停时的 VM 执行帧。
    pub fn frame(&self) -> &MirExecutionFrame {
        &self.frame
    }

    /// 暂停的 Handler 句柄。
    pub fn suspension(&self) -> &MacroSuspension<Pending> {
        &self.suspension
    }

    /// 上层 Engine continuation 建立时一次性取回两个所有权组件。
    pub fn into_parts(self) -> (MirExecutionFrame, MacroSuspension<Pending>) {
        (self.frame, self.suspension)
    }

    /// 恢复 Handler，并把完成输出交回原 VM Macro 指令。
    pub fn resume<ResumeError>(
        self,
        story: &BytecodeProgram,
        resume: impl FnOnce(
            Pending,
            &mut MacroLocalScopes<Value>,
        )
            -> Result<MacroHandlerOutcome<RuntimeMacroExecution, Pending>, ResumeError>,
    ) -> Result<
        RuntimeMacroContinuationResume<Pending>,
        RuntimeMacroContinuationResumeError<ResumeError, Pending>,
    > {
        let Self {
            identity,
            mut frame,
            suspension,
        } = self;
        let outcome: MacroResumeOutcome<RuntimeMacroExecution, Pending> =
            match resume_macro_suspension(identity, suspension, resume) {
                Ok(outcome) => outcome,
                Err(error) => {
                    return Err(RuntimeMacroContinuationResumeError::Macro {
                        frame: Box::new(frame),
                        error,
                    });
                }
            };

        match outcome {
            MacroResumeOutcome::Pending(suspension) => {
                Ok(RuntimeMacroContinuationResume::Pending(Self {
                    identity,
                    frame,
                    suspension,
                }))
            }
            MacroResumeOutcome::Complete { output, scopes } => {
                let RuntimeMacroExecution {
                    execution,
                    includes_entered,
                } = output;
                let control: BodyControl = execution.control;
                if let Err(error) = frame.complete_macro(story, execution.output) {
                    return Err(RuntimeMacroContinuationResumeError::Vm {
                        frame: Box::new(frame),
                        error,
                        scopes,
                    });
                }
                Ok(RuntimeMacroContinuationResume::Complete(
                    RuntimeMacroResumed {
                        identity,
                        frame,
                        control,
                        includes_entered,
                        scopes,
                    },
                ))
            }
        }
    }
}

impl<Pending> RuntimeMacroBodyContinuation<Pending> {
    /// 校验身份一致且 frame 停在正文的动态 Macro 位置后建立 continuation。
    pub fn new(
        identity: RuntimeExecutionIdentity,
        frame: MirExecutionFrame,
        suspension: MacroSuspension<Pending>,
        body: &MirMacroBody<'_, '_>,
    ) -> Result<Self, RuntimeMacroContinuationError<Pending>> {
        if suspension.identity != identity {
            return Err(RuntimeMacroContinuationError::IdentityMismatch {
                expected: identity,
                parts: Box::new(RuntimeMacroContinuationParts { frame, suspension }),
            });
        }
        let bytecode: BytecodeMacroBody = BytecodeMacroBody::compile(body);
        if frame.pending_macro_body(&bytecode).is_none() {
            return Err(RuntimeMacroContinuationError::ExpectedMacroPending {
                identity,
                parts: Box::new(RuntimeMacroContinuationParts { frame, suspension }),
            });
        }
        Ok(Self {
            identity,
            frame,
            suspension,
        })
    }

    /// 暂停所属的执行身份。
    pub fn identity(&self) -> RuntimeExecutionIdentity {
        self.identity
    }

    /// 暂停位置：身份加当前 VM 位置，不含任何状态。
    pub fn location(&self) -> RuntimeExecutionLocation {
        RuntimeExecutionLocation::new(self.identity, self.frame.location())
    }

    /// 暂停时的 VM 执行帧。
    pub fn frame(&self) -> &MirExecutionFrame {
        &self.frame
    }

    /// 暂停的 Handler 句柄。
    pub fn suspension(&self) -> &MacroSuspension<Pending> {
        &self.suspension
    }

    /// 恢复时一次性取回 frame 与暂停句柄两个所有权组件。
    pub fn into_parts(self) -> (MirExecutionFrame, MacroSuspension<Pending>) {
        (self.frame, self.suspension)
    }

    /// 恢复 Handler，并把完成输出写回原容器正文的 Macro 指令。
    pub fn resume<ResumeError>(
        self,
        body: &MirMacroBody<'_, '_>,
        resume: impl FnOnce(
            Pending,
            &mut MacroLocalScopes<Value>,
        )
            -> Result<MacroHandlerOutcome<RuntimeMacroExecution, Pending>, ResumeError>,
    ) -> Result<
        RuntimeMacroBodyContinuationResume<Pending>,
        RuntimeMacroContinuationResumeError<ResumeError, Pending>,
    > {
        let Self {
            identity,
            mut frame,
            suspension,
        } = self;
        let outcome: MacroResumeOutcome<RuntimeMacroExecution, Pending> =
            match resume_macro_suspension(identity, suspension, resume) {
                Ok(outcome) => outcome,
                Err(error) => {
                    return Err(RuntimeMacroContinuationResumeError::Macro {
                        frame: Box::new(frame),
                        error,
                    });
                }
            };

        match outcome {
            MacroResumeOutcome::Pending(suspension) => {
                Ok(RuntimeMacroBodyContinuationResume::Pending(Self {
                    identity,
                    frame,
                    suspension,
                }))
            }
            MacroResumeOutcome::Complete { output, scopes } => {
                let RuntimeMacroExecution {
                    execution,
                    includes_entered,
                } = output;
                let control: BodyControl = execution.control;
                let bytecode: BytecodeMacroBody = BytecodeMacroBody::compile(body);
                if let Err(error) = frame.complete_macro_body(&bytecode, execution.output) {
                    return Err(RuntimeMacroContinuationResumeError::Vm {
                        frame: Box::new(frame),
                        error,
                        scopes,
                    });
                }
                Ok(RuntimeMacroBodyContinuationResume::Complete(
                    RuntimeMacroBodyResumed {
                        identity,
                        frame,
                        control,
                        includes_entered,
                        scopes,
                    },
                ))
            }
        }
    }
}

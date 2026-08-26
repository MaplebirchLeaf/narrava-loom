//! Macro 异步暂停状态、执行身份与恢复生命周期。

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    expression::value::Value,
    runtime::RuntimeExecutionIdentity,
};

use super::super::{MacroHandlerOutcome, MacroLocalScopes, SuspendedMacroScopes};

/// 一次已执行调用的同步结果或异步暂停状态。
#[derive(Debug, PartialEq)]
pub enum MacroCallOutcome<Output, Pending> {
    /// Handler 同步完成并返回输出。
    Complete(Output),
    /// Handler 异步暂停，调度器必须整体保存暂停状态。
    Pending(MacroSuspension<Pending>),
}

/// 调度器必须作为整体保存和恢复的 Macro 暂停状态。
#[derive(Debug, PartialEq)]
pub struct MacroSuspension<Pending> {
    pub identity: RuntimeExecutionIdentity,
    pub handle: Pending,
    pub scopes: SuspendedMacroScopes<Value>,
}

/// Pending 恢复后的完成状态或再次暂停状态。
#[derive(Debug, PartialEq)]
pub enum MacroResumeOutcome<Output, Pending> {
    /// Handler 完成，输出与退出后的外层作用域一并交还。
    Complete {
        output: Output,
        scopes: MacroLocalScopes<Value>,
    },
    /// Handler 再次暂停，作用域链重新转移。
    Pending(MacroSuspension<Pending>),
}

/// 恢复回调失败，同时交还已经退出当前帧的外层作用域。
#[derive(Debug, PartialEq)]
pub struct MacroResumeFailure<Error> {
    pub error: Error,
    pub scopes: MacroLocalScopes<Value>,
}

/// 暂停恢复前的身份错误，原暂停状态保持完整。
#[derive(Debug, PartialEq)]
pub struct MacroResumeIdentityError<Pending> {
    pub expected: RuntimeExecutionIdentity,
    pub suspension: MacroSuspension<Pending>,
}

impl<Pending> MacroResumeIdentityError<Pending> {
    /// 生成稳定 Diagnostic，同时保留原暂停状态的所有权。
    pub fn diagnostic(&self) -> Diagnostic {
        let actual: RuntimeExecutionIdentity = self.suspension.identity;
        Diagnostic::new(
            "macro.resume_identity_mismatch",
            DiagnosticSeverity::Error,
            &format!(
                "Macro 暂停属于 Story {} 的执行链 {}，不能恢复到 Story {} 的执行链 {}",
                actual.story, actual.chain, self.expected.story, self.expected.chain,
            ),
        )
    }
}

/// 暂停恢复失败：执行身份不符，或恢复回调自身失败。
#[derive(Debug, PartialEq)]
pub enum MacroResumeError<Error, Pending> {
    /// 暂停属于其他执行链，原状态保持完整。
    Identity(MacroResumeIdentityError<Pending>),
    /// 恢复回调失败，作用域已退出并交还。
    Resume(MacroResumeFailure<Error>),
}

impl<Error, Pending> MacroResumeError<Error, Pending> {
    /// 读取稳定 Diagnostic，但不消费错误中仍由调度器拥有的暂停状态与作用域。
    pub fn diagnostic(&self, convert_resume: impl FnOnce(&Error) -> Diagnostic) -> Diagnostic {
        match self {
            Self::Identity(error) => error.diagnostic(),
            Self::Resume(failure) => convert_resume(&failure.error),
        }
    }
}

/// 恢复一条暂停的 Macro 执行链。
///
/// Complete 或失败会退出当前暂停调用；再次 Pending 则重新转移整条作用域链。
pub fn resume_macro_suspension<Output, Pending, ResumeError>(
    expected: RuntimeExecutionIdentity,
    suspension: MacroSuspension<Pending>,
    resume: impl FnOnce(
        Pending,
        &mut MacroLocalScopes<Value>,
    ) -> Result<MacroHandlerOutcome<Output, Pending>, ResumeError>,
) -> Result<MacroResumeOutcome<Output, Pending>, MacroResumeError<ResumeError, Pending>> {
    if suspension.identity != expected {
        return Err(MacroResumeError::Identity(MacroResumeIdentityError {
            expected,
            suspension,
        }));
    }
    let MacroSuspension {
        identity,
        handle,
        scopes,
    } = suspension;
    let mut scopes: MacroLocalScopes<Value> = scopes.into_scopes();
    match resume(handle, &mut scopes) {
        Ok(MacroHandlerOutcome::Complete(output)) => {
            let _left: bool = scopes.leave();
            Ok(MacroResumeOutcome::Complete { output, scopes })
        }
        Ok(MacroHandlerOutcome::Pending(handle)) => {
            Ok(MacroResumeOutcome::Pending(MacroSuspension {
                identity,
                handle,
                scopes: scopes
                    .suspend()
                    .expect("恢复中的 Macro 调用必须可以再次暂停"),
            }))
        }
        Err(error) => {
            let _left: bool = scopes.leave();
            Err(MacroResumeError::Resume(MacroResumeFailure {
                error,
                scopes,
            }))
        }
    }
}

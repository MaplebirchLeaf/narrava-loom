//! Native／scripts 自定义 Macro 与 Runtime 之间的调用边界。

use crate::{
    diagnostic::Diagnostic,
    hir::HirBodyNode,
    macro_runtime::{
        MacroHandlerOutcome, MacroInvocation, MacroLifecycleCallbacks, MacroLocalScopes,
        MacroLogicContext, MacroResumeError, MacroResumeOutcome, MacroStoryAccess, MacroSuspension,
        resume_macro_suspension,
    },
};

use super::{BodyExecution, RuntimeExecutionIdentity, RuntimeMacroExecution};

/// Binding 负责把 Native Handler 身份转换为一次实际调用。
///
/// `Story`、State 与 `@` 局部域只通过 `MacroLogicContext` 暴露受控能力；
/// Callback 不取得这些控制器的所有权。
pub trait NativeMacroCallbacks<Native, Story>
where
    Story: MacroStoryAccess + ?Sized,
{
    fn invoke(
        &mut self,
        handler: &Native,
        invocation: MacroInvocation<'_, HirBodyNode<'_>, MacroLogicContext<'_, Story>>,
    ) -> Result<BodyExecution, Diagnostic>;
}

/// Binding 提供的异步 Native／scripts Macro 首次调用边界。
pub trait AsyncNativeMacroCallbacks<Native, Story, Pending>
where
    Story: MacroStoryAccess + ?Sized,
{
    fn invoke(
        &mut self,
        handler: &Native,
        invocation: MacroInvocation<'_, HirBodyNode<'_>, MacroLogicContext<'_, Story>>,
    ) -> Result<MacroHandlerOutcome<BodyExecution, Pending>, Diagnostic>;
}

/// Runtime 保存的平台句柄及其所属 MacroName。
///
/// MacroName 用于恢复完成后执行同一 Definition 的 `after` 钩子；平台句柄仍保持不透明。
#[derive(Debug, PartialEq)]
pub struct RuntimeNativePending<Pending> {
    pub name: String,
    pub handle: Pending,
}

/// Native Handler 恢复或最终 `after` 阶段的明确失败来源。
#[derive(Debug, PartialEq)]
pub enum RuntimeNativeResumeError<HandlerError> {
    Handler(HandlerError),
    Lifecycle(Diagnostic),
}

pub type RuntimeNativeResumeOutcome<Pending> =
    MacroResumeOutcome<RuntimeMacroExecution, RuntimeNativePending<Pending>>;

pub type RuntimeNativeResumeFailure<HandlerError, Pending> =
    MacroResumeError<RuntimeNativeResumeError<HandlerError>, RuntimeNativePending<Pending>>;

/// 恢复 Async Native Macro，并只在最终完成时执行一次 `after`。
///
/// 再次 Pending 会保留原 MacroName 并替换平台句柄；通用恢复器继续负责
/// 调用帧的退出或重新暂停，避免 Native 边界复制作用域生命周期。
pub fn resume_async_native_macro<Pending, HandlerError>(
    expected: RuntimeExecutionIdentity,
    suspension: MacroSuspension<RuntimeNativePending<Pending>>,
    lifecycle: Option<&mut dyn MacroLifecycleCallbacks>,
    resume: impl FnOnce(
        Pending,
        &mut MacroLocalScopes<crate::expression::value::Value>,
    )
        -> Result<MacroHandlerOutcome<RuntimeMacroExecution, Pending>, HandlerError>,
) -> Result<
    RuntimeNativeResumeOutcome<Pending>,
    Box<RuntimeNativeResumeFailure<HandlerError, Pending>>,
> {
    resume_macro_suspension(expected, suspension, |pending, scopes| {
        let RuntimeNativePending { name, handle } = pending;
        let outcome: MacroHandlerOutcome<RuntimeMacroExecution, Pending> =
            resume(handle, scopes).map_err(RuntimeNativeResumeError::Handler)?;

        match outcome {
            MacroHandlerOutcome::Pending(handle) => {
                Ok(MacroHandlerOutcome::Pending(RuntimeNativePending {
                    name,
                    handle,
                }))
            }
            MacroHandlerOutcome::Complete(mut output) => {
                if let Some(callbacks) = lifecycle {
                    let arguments = scopes
                        .args()
                        .expect("Async Native 恢复完成前必须存在调用帧");
                    output.execution.output = callbacks
                        .after(&name, arguments, output.execution.output)
                        .map_err(RuntimeNativeResumeError::Lifecycle)?;
                }
                Ok(MacroHandlerOutcome::Complete(output))
            }
        }
    })
    .map_err(Box::new)
}

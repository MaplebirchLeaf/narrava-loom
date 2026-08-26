//! 同步 Macro 的 before、Handler 与 after 生命周期。

use crate::expression::value::Value;

use super::super::{
    CapturedMacroLocals, MacroDispatchError, MacroExecutionKind, MacroHandlerOutcome,
    MacroInvocation, MacroInvocationBody, MacroLocalScopes, dispatch_macro_handler,
};
use super::PreparedMacroCall;

/// 同步生命周期中能够确定归属的失败阶段。
#[derive(Debug, PartialEq)]
pub enum MacroLifecycleError<HandlerError, Pending> {
    /// 定义是 Async，不能走不保存 continuation 的同步入口。
    AsyncDefinition,
    /// before Hook 失败。
    Before(HandlerError),
    /// Definition 校验或主 Handler 失败。
    Handler(MacroDispatchError<HandlerError, Pending>),
    /// after Hook 失败。
    After(HandlerError),
}

/// 一次调用已经按 MacroName 选出的有序 before／after Hook。
pub struct MacroLifecycleHookSequence<BeforeHooks, AfterHooks> {
    pub before: BeforeHooks,
    pub after: AfterHooks,
}

impl<BeforeHooks, AfterHooks> MacroLifecycleHookSequence<BeforeHooks, AfterHooks> {
    /// 组合指定 Macro 的有序 before 与 after Hook 迭代器。
    pub fn new(before: BeforeHooks, after: AfterHooks) -> Self {
        Self { before, after }
    }
}

/// 生命周期执行期间组合借用外部 Context 与当前 Macro Local 链。
pub struct MacroLifecycleExecutionContext<'runtime, Context> {
    pub context: &'runtime mut Context,
    pub locals: &'runtime mut MacroLocalScopes<Value>,
}

impl<'runtime, Context> MacroLifecycleExecutionContext<'runtime, Context> {
    /// 组合生命周期执行期间的外部 Context 与当前 Macro Local 链。
    pub fn new(
        context: &'runtime mut Context,
        locals: &'runtime mut MacroLocalScopes<Value>,
    ) -> Self {
        Self { context, locals }
    }
}

/// 执行一次不会暂停的完整 Macro 生命周期。
///
/// before 与 Handler 共享当前调用帧中的参数；after 只接收 Handler 返回的独立输出。
/// Async Definition 必须交给能保存 continuation 的入口，不能从这里丢失 Pending。
pub fn execute_prepared_sync_macro_with_lifecycle<
    'hooks,
    Hook,
    Handler,
    Body,
    Context,
    Output,
    Pending,
    HandlerError,
    BeforeHooks,
    AfterHooks,
    Before,
    Invoke,
    After,
>(
    prepared: PreparedMacroCall<'_, Handler>,
    body: MacroInvocationBody<'_, Body>,
    execution: MacroLifecycleExecutionContext<'_, Context>,
    hooks: MacroLifecycleHookSequence<BeforeHooks, AfterHooks>,
    mut before: Before,
    invoke: Invoke,
    mut after: After,
) -> Result<Output, MacroLifecycleError<HandlerError, Pending>>
where
    Hook: 'hooks,
    BeforeHooks: IntoIterator<Item = &'hooks Hook>,
    AfterHooks: IntoIterator<Item = &'hooks Hook>,
    Before:
        FnMut(&'hooks Hook, &mut MacroLocalScopes<Value>, &mut Context) -> Result<(), HandlerError>,
    Invoke: for<'invoke> FnOnce(
        &Handler,
        MacroInvocation<'invoke, Body, Context>,
    ) -> Result<MacroHandlerOutcome<Output, Pending>, HandlerError>,
    After: FnMut(
        &'hooks Hook,
        Output,
        &mut MacroLocalScopes<Value>,
        &mut Context,
    ) -> Result<Output, HandlerError>,
{
    let MacroLifecycleExecutionContext { context, locals } = execution;
    let MacroLifecycleHookSequence {
        before: before_hooks,
        after: after_hooks,
    } = hooks;
    let PreparedMacroCall {
        name,
        raw_arguments,
        arguments,
        definition,
    } = prepared;
    if definition.execution_kind != MacroExecutionKind::Sync {
        return Err(MacroLifecycleError::AsyncDefinition);
    }

    locals.enter_call(arguments);
    for hook in before_hooks {
        if let Err(error) = before(hook, locals, context) {
            let _left: bool = locals.leave();
            return Err(MacroLifecycleError::Before(error));
        }
    }

    let handler_result = {
        let arguments: &[Value] = locals.args().expect("同步 Macro 调用帧必须存在");
        let invocation: MacroInvocation<'_, Body, Context> = MacroInvocation {
            name,
            raw_arguments,
            arguments,
            body,
            captures: CapturedMacroLocals::empty(),
            context,
        };
        dispatch_macro_handler(definition, invocation, invoke)
    };
    let output: Output = match handler_result {
        Ok(MacroHandlerOutcome::Complete(output)) => output,
        Ok(MacroHandlerOutcome::Pending(_)) => {
            unreachable!("Sync Definition 的 Pending 已由 Handler 分派拒绝")
        }
        Err(error) => {
            let _left: bool = locals.leave();
            return Err(MacroLifecycleError::Handler(error));
        }
    };

    let mut output: Output = output;
    for hook in after_hooks {
        output = match after(hook, output, locals, context) {
            Ok(updated) => updated,
            Err(error) => {
                let _left: bool = locals.leave();
                return Err(MacroLifecycleError::After(error));
            }
        };
    }
    let _left: bool = locals.leave();
    Ok(output)
}

//! Macro Handler 的稳定输入与同步／异步输出边界。

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    expression::value::Value,
};

use super::{CapturedMacroLocals, MacroBodyKind, MacroDefinition, MacroExecutionKind};

/// Handler 是否收到正文。
///
/// 使用枚举而不是 `Option`，让调用方直接看出这是 Inline 还是 Container Macro。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroInvocationBody<'source, Body> {
    Inline,
    Container(&'source [Body]),
}

/// 一次 Handler 调用可读取的完整输入。
///
/// Context 由 Runtime 组装并借给 Handler，因此 Handler 不拥有 State、Story 或调度器。
pub struct MacroInvocation<'source, Body, Context> {
    pub name: &'source str,
    pub raw_arguments: &'source str,
    pub arguments: &'source [Value],
    pub body: MacroInvocationBody<'source, Body>,
    /// 延迟执行只能保留词法 `capture` 明确列出的局部绑定。
    pub captures: CapturedMacroLocals<Value>,
    pub context: &'source mut Context,
}

impl<'source, Body, Context> MacroInvocation<'source, Body, Context> {
    /// 建立不接收正文的 Inline Macro 输入。
    pub fn inline(
        name: &'source str,
        raw_arguments: &'source str,
        arguments: &'source [Value],
        context: &'source mut Context,
    ) -> Self {
        Self {
            name,
            raw_arguments,
            arguments,
            body: MacroInvocationBody::Inline,
            captures: CapturedMacroLocals::empty(),
            context,
        }
    }

    /// 建立接收正文的 Container Macro 输入。
    pub fn container(
        name: &'source str,
        raw_arguments: &'source str,
        arguments: &'source [Value],
        body: &'source [Body],
        context: &'source mut Context,
    ) -> Self {
        Self {
            name,
            raw_arguments,
            arguments,
            body: MacroInvocationBody::Container(body),
            captures: CapturedMacroLocals::empty(),
            context,
        }
    }
}

/// Handler 已完成，或把后续执行交给 Runtime 调度器。
///
/// `Pending` 保存的是调度器定义的句柄，不直接绑定 Rust Future 或 JavaScript Promise。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroHandlerOutcome<Output, Pending> {
    Complete(Output),
    Pending(Pending),
}

/// Definition 分派阶段产生的错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroDispatchError<HandlerError, Pending> {
    /// 调用源码是否携带正文，与注册定义不一致。
    BodyKindMismatch {
        expected: MacroBodyKind,
        actual: MacroBodyKind,
    },
    /// 声明为同步的 Handler 返回了暂停句柄。
    UnexpectedPending(Pending),
    /// Handler 自身报告的业务或 Runtime 错误。
    Handler(HandlerError),
}

impl<HandlerError, Pending> MacroDispatchError<HandlerError, Pending> {
    /// 转换为稳定 Diagnostic；Handler 自身错误仍由所属边界负责转换。
    pub fn diagnostic<Convert>(self, name: &str, convert_handler: Convert) -> Diagnostic
    where
        Convert: FnOnce(HandlerError) -> Diagnostic,
    {
        match self {
            Self::BodyKindMismatch { expected, actual } => Diagnostic::new(
                "macro.body_kind_mismatch",
                DiagnosticSeverity::Error,
                &format!(
                    "Macro `{name}` 定义为 {}，但调用提供了 {} 正文",
                    body_kind_name(expected),
                    body_kind_name(actual),
                ),
            ),
            Self::UnexpectedPending(_handle) => Diagnostic::new(
                "macro.unexpected_pending",
                DiagnosticSeverity::Error,
                &format!("Macro `{name}` 声明为 Sync，但 Handler 返回了 Pending"),
            ),
            Self::Handler(error) => convert_handler(error),
        }
    }
}

fn body_kind_name(kind: MacroBodyKind) -> &'static str {
    match kind {
        MacroBodyKind::Inline => "Inline",
        MacroBodyKind::Container => "Container",
    }
}

/// 按 Definition 校验并调用一个 Macro Handler。
///
/// `invoke` 是 Rust 内置 Handler 或 scripts 桥提供的调用适配器。这里不拥有调度器，
/// 因而只验证 Sync／Async 契约，并把合法的 Pending 句柄交还上层。
pub fn dispatch_macro<'source, Handler, Body, Context, Output, Pending, HandlerError, Invoke>(
    definition: &MacroDefinition<Handler>,
    invocation: MacroInvocation<'source, Body, Context>,
    invoke: Invoke,
) -> Result<MacroHandlerOutcome<Output, Pending>, MacroDispatchError<HandlerError, Pending>>
where
    Invoke: FnOnce(
        &Handler,
        MacroInvocation<'source, Body, Context>,
    ) -> Result<MacroHandlerOutcome<Output, Pending>, HandlerError>,
{
    dispatch_macro_handler(definition, invocation, invoke)
}

/// 生命周期分派完成 Hook 后，用该入口执行 Definition 的主 Handler。
pub(crate) fn dispatch_macro_handler<
    'source,
    Handler,
    Body,
    Context,
    Output,
    Pending,
    HandlerError,
    Invoke,
>(
    definition: &MacroDefinition<Handler>,
    invocation: MacroInvocation<'source, Body, Context>,
    invoke: Invoke,
) -> Result<MacroHandlerOutcome<Output, Pending>, MacroDispatchError<HandlerError, Pending>>
where
    Invoke: FnOnce(
        &Handler,
        MacroInvocation<'source, Body, Context>,
    ) -> Result<MacroHandlerOutcome<Output, Pending>, HandlerError>,
{
    let actual_body_kind: MacroBodyKind = match invocation.body {
        MacroInvocationBody::Inline => MacroBodyKind::Inline,
        MacroInvocationBody::Container(_) => MacroBodyKind::Container,
    };
    if definition.body_kind != actual_body_kind {
        return Err(MacroDispatchError::BodyKindMismatch {
            expected: definition.body_kind,
            actual: actual_body_kind,
        });
    }

    let outcome: MacroHandlerOutcome<Output, Pending> =
        invoke(&definition.handler, invocation).map_err(MacroDispatchError::Handler)?;
    match (definition.execution_kind, outcome) {
        (MacroExecutionKind::Sync, MacroHandlerOutcome::Pending(handle)) => {
            Err(MacroDispatchError::UnexpectedPending(handle))
        }
        (_, outcome) => Ok(outcome),
    }
}

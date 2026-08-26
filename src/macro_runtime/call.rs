//! Macro Definition 查询与调用参数准备。

use crate::{
    expression::{Expression, value::Value},
    runtime::RuntimeExecutionIdentity,
};

use super::{
    CapturedMacroLocals, MacroArgumentKind, MacroArgumentListError, MacroArgumentValueError,
    MacroDefinition, MacroDefinitionError, MacroDefinitions, MacroDispatchError,
    MacroHandlerOutcome, MacroInvocation, MacroInvocationBody, MacroLocalScopes, dispatch_macro,
    parse_argument_list, prepare_argument_values,
};

/// 已完成 Definition 查询和参数准备的 Macro 调用。
pub struct PreparedMacroCall<'call, Handler> {
    pub name: &'call str,
    pub raw_arguments: &'call str,
    pub arguments: Vec<Value>,
    pub definition: &'call MacroDefinition<Handler>,
}

/// Macro 调用在进入 Handler 前的准备错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroCallPreparationError<EvaluationError> {
    /// Definition 查询失败。
    Definition(MacroDefinitionError),
    /// 参数列表解析失败。
    ArgumentList(MacroArgumentListError),
    /// 参数求值失败。
    ArgumentValue(MacroArgumentValueError<EvaluationError>),
}

mod diagnostic;
pub use diagnostic::*;

mod suspension;
pub use suspension::*;

mod lifecycle;
pub use lifecycle::*;

/// 查询 Definition，并按其参数契约准备 Handler 输入。
///
/// Raw 参数不进入 Expression Parser；ArgumentList 则完整解析并求值后才返回。
pub fn prepare_macro_call<'call, Handler, EvaluationError>(
    definitions: &'call MacroDefinitions<MacroDefinition<Handler>>,
    name: &'call str,
    raw_arguments: &'call str,
    evaluate: impl FnMut(&Expression<'_>) -> Result<Value, EvaluationError>,
) -> Result<PreparedMacroCall<'call, Handler>, MacroCallPreparationError<EvaluationError>> {
    let definition: &MacroDefinition<Handler> =
        definitions
            .get(name)
            .ok_or(MacroCallPreparationError::Definition(
                MacroDefinitionError::MissingDefinition,
            ))?;
    let arguments: Vec<Value> = match definition.argument_kind {
        MacroArgumentKind::Raw => Vec::new(),
        MacroArgumentKind::ArgumentList => {
            let parsed = parse_argument_list(raw_arguments)
                .map_err(MacroCallPreparationError::ArgumentList)?;
            prepare_argument_values(&parsed, evaluate)
                .map_err(MacroCallPreparationError::ArgumentValue)?
        }
    };

    Ok(PreparedMacroCall {
        name,
        raw_arguments,
        arguments,
        definition,
    })
}

/// 为已准备调用建立局部帧、分派 Handler，并处理帧的完成或暂停生命周期。
pub fn execute_prepared_macro<Handler, Body, Context, Output, Pending, HandlerError, Invoke>(
    prepared: PreparedMacroCall<'_, Handler>,
    identity: RuntimeExecutionIdentity,
    body: MacroInvocationBody<'_, Body>,
    context: &mut Context,
    locals: &mut MacroLocalScopes<Value>,
    invoke: Invoke,
) -> Result<MacroCallOutcome<Output, Pending>, MacroDispatchError<HandlerError, Pending>>
where
    Invoke: for<'invoke> FnOnce(
        &Handler,
        MacroInvocation<'invoke, Body, Context>,
    ) -> Result<MacroHandlerOutcome<Output, Pending>, HandlerError>,
{
    let PreparedMacroCall {
        name,
        raw_arguments,
        arguments,
        definition,
    } = prepared;
    locals.enter_call(arguments);

    let result = {
        let arguments: &[Value] = locals.args().expect("刚建立的调用帧必须存在");
        let invocation = MacroInvocation {
            name,
            raw_arguments,
            arguments,
            body,
            captures: CapturedMacroLocals::empty(),
            context,
        };
        dispatch_macro(definition, invocation, invoke)
    };

    match result {
        Ok(MacroHandlerOutcome::Complete(output)) => {
            let _left: bool = locals.leave();
            Ok(MacroCallOutcome::Complete(output))
        }
        Ok(MacroHandlerOutcome::Pending(handle)) => {
            Ok(MacroCallOutcome::Pending(MacroSuspension {
                identity,
                handle,
                scopes: locals.suspend().expect("活动 Macro 调用必须可以暂停"),
            }))
        }
        Err(error) => {
            let _left: bool = locals.leave();
            Err(error)
        }
    }
}

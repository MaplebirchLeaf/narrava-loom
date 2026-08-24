//! Widget Definition 调用、参数准备与独立执行域。

use crate::{
    expression::{
        evaluator::{EvalError, WritableEvaluationContext, evaluate_with_mut},
        value::Value,
    },
    hir::{HirBodyNode, HirMacro, HirMacroArguments},
    macro_runtime::{
        MacroArgumentListError, MacroArgumentValueError, MacroDefinition, MacroDefinitionError,
        MacroDefinitions, MacroLocalScopes, MacroLogicContext, MacroStoryAccess,
        RuntimeMacroHandler, parse_argument_list, prepare_argument_values,
    },
    twee::MacroSyntaxKind,
};

use super::{BodyControl, LogicNodeError, execute_logic_body};

/// Widget 调用准备与正文执行保持各自原始错误边界。
#[derive(Debug, PartialEq)]
pub enum WidgetMacroError<StoryError> {
    Definition(MacroDefinitionError),
    ArgumentList(MacroArgumentListError),
    ArgumentValue(MacroArgumentValueError<EvalError>),
    InvalidHirArguments,
    NativeHandler,
    /// Widget 调用是 Inline Macro，调用处不能再携带正文。
    ContainerCall,
    Logic(LogicNodeError<StoryError>),
}

/// 已完成 Definition 查询与实参求值、尚未建立调用帧的 Widget 调用。
pub struct PreparedWidget<'hir, 'source> {
    pub body: &'hir [HirBodyNode<'source>],
    pub arguments: Vec<Value>,
}

/// 在独立调用帧中执行 Widget 正文，并消费属于该 Widget 的 exit。
pub fn execute_widget_body<Story>(
    body: &[HirBodyNode<'_>],
    arguments: Vec<Value>,
    state: &mut dyn WritableEvaluationContext,
    story: &mut Story,
    locals: &mut MacroLocalScopes<Value>,
) -> Result<BodyControl, LogicNodeError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    locals.enter_call(arguments);
    let result: Result<BodyControl, LogicNodeError<Story::Error>> = {
        let mut context: MacroLogicContext<'_, Story> =
            MacroLogicContext::new(state, story, locals);
        execute_logic_body(body, &mut context)
    };
    let _left: bool = locals.leave();

    match result? {
        BodyControl::ExitScope => Ok(BodyControl::Continue),
        control => Ok(control),
    }
}

/// 查询 Widget Definition、准备位置实参，再进入独立 Widget 执行域。
pub fn execute_widget_macro<Story, Native>(
    call: &HirMacro<'_>,
    definitions: &MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, Native>>>,
    state: &mut dyn WritableEvaluationContext,
    story: &mut Story,
    locals: &mut MacroLocalScopes<Value>,
) -> Result<BodyControl, WidgetMacroError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    let prepared: PreparedWidget<'_, '_> =
        prepare_widget_macro(call, definitions, state, story, locals)?;
    execute_widget_body(prepared.body, prepared.arguments, state, story, locals)
        .map_err(WidgetMacroError::Logic)
}

/// 查询 Widget Definition 并在调用者作用域中完成实参求值。
pub fn prepare_widget_macro<'hir, 'source, 'call, Story, Native>(
    call: &HirMacro<'call>,
    definitions: &MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'hir, 'source, Native>>>,
    state: &mut dyn WritableEvaluationContext,
    story: &mut Story,
    locals: &mut MacroLocalScopes<Value>,
) -> Result<PreparedWidget<'hir, 'source>, WidgetMacroError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    if call.syntax_kind == MacroSyntaxKind::Container {
        return Err(WidgetMacroError::ContainerCall);
    }

    let definition: &MacroDefinition<RuntimeMacroHandler<'_, '_, Native>> = definitions
        .get(call.name)
        .ok_or(WidgetMacroError::Definition(
            MacroDefinitionError::MissingDefinition,
        ))?;
    let body: &'hir [HirBodyNode<'source>] = match &definition.handler {
        RuntimeMacroHandler::Widget(body) => body,
        RuntimeMacroHandler::Native(_) => return Err(WidgetMacroError::NativeHandler),
    };
    let raw_arguments: &str = match call.arguments {
        HirMacroArguments::None => "",
        HirMacroArguments::Raw(raw) => raw,
        HirMacroArguments::Expression(_) => return Err(WidgetMacroError::InvalidHirArguments),
    };
    let parsed = parse_argument_list(raw_arguments).map_err(WidgetMacroError::ArgumentList)?;
    let arguments: Vec<Value> = {
        let mut context: MacroLogicContext<'_, Story> =
            MacroLogicContext::new(state, story, locals);
        prepare_argument_values(&parsed, |expression| {
            evaluate_with_mut(expression, &mut context)
        })
        .map_err(WidgetMacroError::ArgumentValue)?
    };

    Ok(PreparedWidget { body, arguments })
}

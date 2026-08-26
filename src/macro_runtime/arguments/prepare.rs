//! Macro 参数求值、Interaction 动态文本与调用帧准备。

use crate::expression::{
    Expression, ParseError,
    evaluator::value_to_text,
    parse,
    value::{TextValue, Value},
};
use crate::interpolation::find_interpolation_end;

use super::{InteractionTarget, MacroArgument, MacroArgumentValueError};
use crate::macro_runtime::MacroLocalScopes;

/// 把有序参数准备为调用帧中的 `@args`。
///
/// 求值方式由调用方注入，因此只读、可写、随机数和未来异步入口不在这里重复实现。
pub fn prepare_argument_values<Error>(
    arguments: &[MacroArgument<'_>],
    mut evaluate: impl FnMut(&Expression<'_>) -> Result<Value, Error>,
) -> Result<Vec<Value>, MacroArgumentValueError<Error>> {
    let mut values: Vec<Value> = Vec::with_capacity(arguments.len());

    for argument in arguments {
        let value: Value = match argument {
            MacroArgument::Expression { expression, offset } => {
                evaluate(expression).map_err(|error: Error| {
                    MacroArgumentValueError::Expression {
                        error,
                        offset: *offset,
                    }
                })?
            }
            MacroArgument::InteractionTarget {
                target,
                label_offset,
                target_offset,
            } => interaction_target_value(*target, *label_offset, *target_offset, &mut evaluate)?,
        };
        values.push(value);
    }

    Ok(values)
}

/// 先完整准备参数，再原子地建立调用帧，避免 Handler 观察到半组 `@args`。
pub fn enter_argument_call<Error>(
    locals: &mut MacroLocalScopes<Value>,
    arguments: &[MacroArgument<'_>],
    evaluate: impl FnMut(&Expression<'_>) -> Result<Value, Error>,
) -> Result<(), MacroArgumentValueError<Error>> {
    let values: Vec<Value> = prepare_argument_values(arguments, evaluate)?;
    locals.enter_call(values);
    Ok(())
}

/// scripts Handler 通过稳定的 `label`、`target` 字段读取交互目标。
fn interaction_target_value<Error>(
    target: InteractionTarget<'_>,
    label_offset: usize,
    target_offset: usize,
    evaluate: &mut impl FnMut(&Expression<'_>) -> Result<Value, Error>,
) -> Result<Value, MacroArgumentValueError<Error>> {
    let label: TextValue = evaluate_interaction_text(target.label, label_offset, evaluate)?;
    let target_value: TextValue =
        evaluate_interaction_text(target.target, target_offset, evaluate)?;
    Ok(Value::object(vec![
        (String::from("label"), Value::String(label)),
        (String::from("target"), Value::String(target_value)),
    ]))
}

/// 整段变量引用与 `${expression}` 共用标准 Expression 求值和文本转换。
fn evaluate_interaction_text<Error>(
    source: &str,
    offset: usize,
    evaluate: &mut impl FnMut(&Expression<'_>) -> Result<Value, Error>,
) -> Result<TextValue, MacroArgumentValueError<Error>> {
    if source.starts_with(['$', '_', '@']) && !source.contains("${") {
        return evaluate_interaction_expression(source, offset, evaluate);
    }

    let mut units: Vec<u16> = Vec::new();
    let mut cursor: usize = 0;
    while let Some(relative_start) = source[cursor..].find("${") {
        let start: usize = cursor + relative_start;
        units.extend(source[cursor..start].encode_utf16());
        let expression_start: usize = start + 2;
        let expression_end: usize = find_interpolation_end(source, expression_start).ok_or(
            MacroArgumentValueError::UnclosedInteraction {
                offset: offset + start,
            },
        )?;
        let value: TextValue = evaluate_interaction_expression(
            &source[expression_start..expression_end],
            offset + expression_start,
            evaluate,
        )?;
        units.extend_from_slice(value.as_units());
        cursor = expression_end + 1;
    }
    units.extend(source[cursor..].encode_utf16());
    Ok(TextValue::from_units(units))
}

/// 把交互文本中的整段引用或 `${...}` 内容按标准 Expression 求值并转文本。
fn evaluate_interaction_expression<Error>(
    source: &str,
    offset: usize,
    evaluate: &mut impl FnMut(&Expression<'_>) -> Result<Value, Error>,
) -> Result<TextValue, MacroArgumentValueError<Error>> {
    let expression: Expression<'_> = parse(source)
        .map_err(|error: ParseError| MacroArgumentValueError::InteractionParse { error, offset })?;
    let value: Value = evaluate(&expression)
        .map_err(|error: Error| MacroArgumentValueError::InteractionEvaluation { error, offset })?;
    value_to_text(&value).ok_or(MacroArgumentValueError::InvalidInteractionText { offset })
}

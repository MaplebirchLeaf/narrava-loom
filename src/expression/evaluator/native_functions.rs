//! Expression 内置函数的参数检查与受控求值。

use super::{EvalError, EvaluationSession, RandomSource, to_number, to_string};
use crate::expression::{
    Expression, Span,
    value::{NativeFunction, TextValue, Value},
};

pub(super) fn call_native_function(
    function: NativeFunction,
    arguments: Vec<Value>,
    argument_nodes: &[Expression<'_>],
    call_span: Span,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let expected_range: std::ops::RangeInclusive<usize> = function.argument_range();
    if !expected_range.contains(&arguments.len()) {
        return Err(EvalError::InvalidArgumentCount(call_span));
    }

    match function {
        NativeFunction::ObjectAssign => {
            evaluate_object_assign(&arguments, argument_nodes, call_span, session)
        }
        NativeFunction::ObjectHasOwn => evaluate_object_has_own(&arguments, argument_nodes),
        NativeFunction::Either | NativeFunction::Random => {
            evaluate_random_function(function, arguments, call_span, session)
        }
        NativeFunction::Boolean | NativeFunction::Number | NativeFunction::String => {
            evaluate_conversion_function(function, &arguments[0], argument_nodes[0].span)
        }
        NativeFunction::Abs
        | NativeFunction::Ceil
        | NativeFunction::Floor
        | NativeFunction::Round => {
            evaluate_numeric_function(function, &arguments[0], argument_nodes[0].span)
        }
        NativeFunction::Clamp | NativeFunction::Max | NativeFunction::Min => {
            evaluate_range_numeric_function(function, &arguments, argument_nodes)
        }
        NativeFunction::Defined => Ok(Value::Boolean(!matches!(
            arguments.first(),
            Some(Value::Undefined)
        ))),
        NativeFunction::Empty => {
            let empty: bool = match &arguments[0] {
                Value::Undefined | Value::Null => true,
                Value::String(value) => value.is_empty(),
                Value::Array(values) => values.is_empty(),
                Value::Object(properties) => properties.is_empty(),
                Value::Boolean(_)
                | Value::Callable(_)
                | Value::ScriptCallable(_)
                | Value::Namespace(_)
                | Value::Number(_) => false,
            };
            Ok(Value::Boolean(empty))
        }
        NativeFunction::Entries | NativeFunction::Keys | NativeFunction::Values => {
            evaluate_collection_function(function, &arguments[0], argument_nodes[0].span)
        }
    }
}

fn evaluate_object_assign(
    arguments: &[Value],
    argument_nodes: &[Expression<'_>],
    call_span: Span,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let Value::Object(target) = &arguments[0] else {
        return Err(EvalError::InvalidObjectTarget(argument_nodes[0].span));
    };
    let mut sources: Vec<Vec<(String, Value)>> = Vec::with_capacity(arguments.len() - 1);
    for (source, node) in arguments[1..].iter().zip(&argument_nodes[1..]) {
        let Value::Object(properties) = source else {
            return Err(EvalError::InvalidObjectTarget(node.span));
        };
        sources.push(properties.snapshot());
    }

    session.context.authorize_reference_write(call_span)?;
    for source in sources {
        target.with_mut(|properties: &mut Vec<(String, Value)>| {
            for (name, value) in source {
                if let Some((_, stored)) = properties
                    .iter_mut()
                    .find(|(stored_name, _): &&mut (String, Value)| stored_name == &name)
                {
                    *stored = value;
                } else {
                    properties.push((name, value));
                }
            }
        });
    }
    Ok(arguments[0].clone())
}

fn evaluate_object_has_own(
    arguments: &[Value],
    argument_nodes: &[Expression<'_>],
) -> Result<Value, EvalError> {
    let Value::Object(properties) = &arguments[0] else {
        return Err(EvalError::InvalidObjectTarget(argument_nodes[0].span));
    };
    let key: TextValue = to_string(&arguments[1])
        .ok_or(EvalError::InvalidStringConversion(argument_nodes[1].span))?;
    let key: String = key
        .to_unicode_string()
        .ok_or(EvalError::InvalidStringConversion(argument_nodes[1].span))?;
    Ok(Value::Boolean(properties.with(
        |values: &Vec<(String, Value)>| {
            values
                .iter()
                .any(|(name, _value): &(String, Value)| name == &key)
        },
    )))
}

fn evaluate_random_function(
    function: NativeFunction,
    arguments: Vec<Value>,
    call_span: Span,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let random: &mut dyn RandomSource = session
        .random
        .as_deref_mut()
        .ok_or(EvalError::MissingRandomSource(call_span))?;
    let unit: f64 = random.next_unit();
    if !unit.is_finite() || !(0.0..1.0).contains(&unit) {
        return Err(EvalError::InvalidRandomValue(call_span));
    }

    match function {
        NativeFunction::Random => Ok(Value::Number(unit)),
        NativeFunction::Either => {
            let index: usize = (unit * arguments.len() as f64).floor() as usize;
            Ok(arguments[index].clone())
        }
        _ => unreachable!("随机函数分派只接收 random 或 either"),
    }
}

fn evaluate_conversion_function(
    function: NativeFunction,
    argument: &Value,
    argument_span: Span,
) -> Result<Value, EvalError> {
    match function {
        NativeFunction::Boolean => Ok(Value::Boolean(argument.is_truthy())),
        NativeFunction::Number => Ok(Value::Number(
            to_number(argument).ok_or(EvalError::InvalidNumericConversion(argument_span))?,
        )),
        NativeFunction::String => Ok(Value::String(
            to_string(argument).ok_or(EvalError::InvalidStringConversion(argument_span))?,
        )),
        NativeFunction::Abs
        | NativeFunction::Ceil
        | NativeFunction::Clamp
        | NativeFunction::Defined
        | NativeFunction::Empty
        | NativeFunction::Either
        | NativeFunction::Entries
        | NativeFunction::Floor
        | NativeFunction::Keys
        | NativeFunction::Max
        | NativeFunction::Min
        | NativeFunction::ObjectAssign
        | NativeFunction::ObjectHasOwn
        | NativeFunction::Random
        | NativeFunction::Round
        | NativeFunction::Values => unreachable!("转换函数分派只接收转换函数"),
    }
}

fn evaluate_numeric_function(
    function: NativeFunction,
    argument: &Value,
    argument_span: Span,
) -> Result<Value, EvalError> {
    let Value::Number(number) = argument else {
        return Err(EvalError::InvalidNumericArgument(argument_span));
    };
    let result: f64 = match function {
        NativeFunction::Abs => number.abs(),
        NativeFunction::Ceil => number.ceil(),
        NativeFunction::Floor => number.floor(),
        NativeFunction::Round => round_web(*number),
        NativeFunction::Defined
        | NativeFunction::Boolean
        | NativeFunction::Empty
        | NativeFunction::Either
        | NativeFunction::Entries
        | NativeFunction::Keys
        | NativeFunction::Clamp
        | NativeFunction::Max
        | NativeFunction::Min
        | NativeFunction::Number
        | NativeFunction::ObjectAssign
        | NativeFunction::ObjectHasOwn
        | NativeFunction::Random
        | NativeFunction::String
        | NativeFunction::Values => unreachable!("数值函数分派只接收数值函数"),
    };
    Ok(Value::Number(result))
}

fn evaluate_range_numeric_function(
    function: NativeFunction,
    arguments: &[Value],
    argument_nodes: &[Expression<'_>],
) -> Result<Value, EvalError> {
    let numbers: Result<Vec<f64>, EvalError> = arguments
        .iter()
        .zip(argument_nodes)
        .map(
            |(argument, node): (&Value, &Expression<'_>)| match argument {
                Value::Number(number) => Ok(*number),
                _ => Err(EvalError::InvalidNumericArgument(node.span)),
            },
        )
        .collect();
    let numbers: Vec<f64> = numbers?;

    let result: f64 = match function {
        NativeFunction::Min => numbers
            .into_iter()
            .reduce(web_min)
            .expect("min 至少接收一个参数"),
        NativeFunction::Max => numbers
            .into_iter()
            .reduce(web_max)
            .expect("max 至少接收一个参数"),
        NativeFunction::Clamp => {
            let value: f64 = numbers[0];
            let lower: f64 = numbers[1];
            let upper: f64 = numbers[2];
            if lower > upper {
                return Err(EvalError::InvalidRange(Span {
                    start: argument_nodes[1].span.start,
                    end: argument_nodes[2].span.end,
                }));
            }
            web_max(lower, web_min(value, upper))
        }
        NativeFunction::Abs
        | NativeFunction::Boolean
        | NativeFunction::Ceil
        | NativeFunction::Defined
        | NativeFunction::Empty
        | NativeFunction::Either
        | NativeFunction::Entries
        | NativeFunction::Floor
        | NativeFunction::Keys
        | NativeFunction::Number
        | NativeFunction::ObjectAssign
        | NativeFunction::ObjectHasOwn
        | NativeFunction::Random
        | NativeFunction::Round
        | NativeFunction::String
        | NativeFunction::Values => unreachable!("范围函数分派只接收 min、max 或 clamp"),
    };
    Ok(Value::Number(result))
}

fn web_min(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        return f64::NAN;
    }
    if left == 0.0 && right == 0.0 {
        return if left.is_sign_negative() || right.is_sign_negative() {
            -0.0
        } else {
            0.0
        };
    }
    left.min(right)
}

fn web_max(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        return f64::NAN;
    }
    if left == 0.0 && right == 0.0 {
        return if left.is_sign_positive() || right.is_sign_positive() {
            0.0
        } else {
            -0.0
        };
    }
    left.max(right)
}

/// Web round 在半数位置向正无穷取整，并保留负数区间产生的负零。
fn round_web(number: f64) -> f64 {
    if !number.is_finite() || number == 0.0 {
        return number;
    }

    let lower: f64 = number.floor();
    let rounded: f64 = if number - lower < 0.5 {
        lower
    } else {
        lower + 1.0
    };
    if rounded == 0.0 && number.is_sign_negative() {
        -0.0
    } else {
        rounded
    }
}

fn evaluate_collection_function(
    function: NativeFunction,
    collection: &Value,
    collection_span: Span,
) -> Result<Value, EvalError> {
    let entries: Vec<(String, Value)> = match collection {
        Value::Object(properties) => properties.snapshot(),
        Value::Array(values) => values.with(|items: &Vec<Value>| {
            items
                .iter()
                .enumerate()
                .map(|(index, value): (usize, &Value)| (index.to_string(), value.clone()))
                .collect()
        }),
        _ => return Err(EvalError::InvalidCollectionTarget(collection_span)),
    };

    let values: Vec<Value> = match function {
        NativeFunction::Keys => entries
            .into_iter()
            .map(|(key, _value): (String, Value)| Value::string(key))
            .collect(),
        NativeFunction::Values => entries
            .into_iter()
            .map(|(_key, value): (String, Value)| value)
            .collect(),
        NativeFunction::Entries => entries
            .into_iter()
            .map(|(key, value): (String, Value)| Value::array(vec![Value::string(key), value]))
            .collect(),
        NativeFunction::Abs
        | NativeFunction::Boolean
        | NativeFunction::Ceil
        | NativeFunction::Clamp
        | NativeFunction::Defined
        | NativeFunction::Empty
        | NativeFunction::Either
        | NativeFunction::Floor
        | NativeFunction::Max
        | NativeFunction::Min
        | NativeFunction::Number
        | NativeFunction::ObjectAssign
        | NativeFunction::ObjectHasOwn
        | NativeFunction::Random
        | NativeFunction::Round
        | NativeFunction::String => {
            unreachable!("集合函数分派只接收 keys、values 或 entries")
        }
    };
    Ok(Value::array(values))
}

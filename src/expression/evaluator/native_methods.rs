//! Array 与 String 原生方法的参数检查和受控求值。

use super::{EvalError, EvaluationSession, strict_equal, to_number, to_string, to_uint32};
use crate::expression::{
    Expression, Span,
    value::{NativeMethod, TextValue, Value},
};

/// 按方法签名检查参数数量，再分派到具体方法求值。
pub(super) fn call_native_method(
    receiver: Value,
    method: NativeMethod,
    arguments: Vec<Value>,
    argument_nodes: &[Expression<'_>],
    call_span: Span,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let expected_range: std::ops::RangeInclusive<usize> = method.argument_range();
    if !expected_range.contains(&arguments.len()) {
        return Err(EvalError::InvalidArgumentCount(call_span));
    }

    match method {
        NativeMethod::ArrayAt => {
            let argument: &Value = &arguments[0];
            let argument_span: Span = argument_nodes[0].span;
            evaluate_array_at(receiver, argument, argument_span)
        }
        NativeMethod::ArrayConcat => {
            let Value::Array(items) = receiver else {
                unreachable!("Array.concat 必须绑定 Array 接收者")
            };
            let mut values: Vec<Value> = items.snapshot();
            for argument in arguments {
                match argument {
                    Value::Array(array) => values.extend(array.snapshot()),
                    value => values.push(value),
                }
            }
            Ok(Value::array(values))
        }
        NativeMethod::ArrayIncludes => {
            let argument: &Value = &arguments[0];
            let Value::Array(items) = receiver else {
                unreachable!("Array.includes 必须绑定 Array 接收者")
            };
            Ok(Value::Boolean(items.with(|values: &Vec<Value>| {
                values
                    .iter()
                    .any(|item: &Value| strict_equal(item, argument))
            })))
        }
        NativeMethod::ArrayIndexOf => evaluate_array_index_of(receiver, &arguments, argument_nodes),
        NativeMethod::ArrayJoin => {
            evaluate_array_join(receiver, &arguments, argument_nodes, call_span)
        }
        NativeMethod::ArrayPop => {
            let Value::Array(items) = receiver else {
                unreachable!("Array.pop 必须绑定 Array 接收者")
            };
            session.context.authorize_reference_write(call_span)?;
            Ok(items.with_mut(|values: &mut Vec<Value>| values.pop().unwrap_or(Value::Undefined)))
        }
        NativeMethod::ArrayPush => {
            let Value::Array(items) = receiver else {
                unreachable!("Array.push 必须绑定 Array 接收者")
            };
            session.context.authorize_reference_write(call_span)?;
            let length: usize = items.with_mut(|values: &mut Vec<Value>| {
                values.extend(arguments);
                values.len()
            });
            Ok(Value::Number(length as f64))
        }
        NativeMethod::ArrayShift => {
            let Value::Array(items) = receiver else {
                unreachable!("Array.shift 必须绑定 Array 接收者")
            };
            session.context.authorize_reference_write(call_span)?;
            Ok(items.with_mut(|values: &mut Vec<Value>| {
                if values.is_empty() {
                    Value::Undefined
                } else {
                    values.remove(0)
                }
            }))
        }
        NativeMethod::ArraySlice => evaluate_array_slice(receiver, &arguments, argument_nodes),
        NativeMethod::ArraySplice => {
            evaluate_array_splice(receiver, arguments, argument_nodes, call_span, session)
        }
        NativeMethod::ArrayUnshift => {
            let Value::Array(items) = receiver else {
                unreachable!("Array.unshift 必须绑定 Array 接收者")
            };
            session.context.authorize_reference_write(call_span)?;
            let length: usize = items.with_mut(|values: &mut Vec<Value>| {
                drop(values.splice(0..0, arguments));
                values.len()
            });
            Ok(Value::Number(length as f64))
        }
        NativeMethod::StringIncludes
        | NativeMethod::StringStartsWith
        | NativeMethod::StringEndsWith => {
            let argument: &Value = &arguments[0];
            let argument_span: Span = argument_nodes[0].span;
            evaluate_string_search(receiver, method, argument, argument_span)
        }
        NativeMethod::StringSlice => evaluate_string_slice(receiver, &arguments, argument_nodes),
        NativeMethod::StringSplit => evaluate_string_split(receiver, &arguments, argument_nodes),
        NativeMethod::StringTrim => {
            let Value::String(text) = receiver else {
                unreachable!("String.trim 必须绑定 String 接收者")
            };
            Ok(Value::String(trim_web_text(&text)))
        }
        NativeMethod::StringToLowerCase => {
            let Value::String(text) = receiver else {
                unreachable!("String.toLowerCase 必须绑定 String 接收者")
            };
            Ok(Value::String(text.to_lowercase()))
        }
        NativeMethod::StringToUpperCase => {
            let Value::String(text) = receiver else {
                unreachable!("String.toUpperCase 必须绑定 String 接收者")
            };
            Ok(Value::String(text.to_uppercase()))
        }
    }
}

/// String.slice 与 String.length 使用同一套 UTF-16 码元边界。
fn evaluate_string_slice(
    receiver: Value,
    arguments: &[Value],
    argument_nodes: &[Expression<'_>],
) -> Result<Value, EvalError> {
    let Value::String(text) = receiver else {
        unreachable!("String.slice 必须绑定 String 接收者")
    };
    let length: usize = text.len();
    let start: usize = match arguments.first() {
        Some(value) => relative_index(value, argument_nodes[0].span, length)?,
        None => 0,
    };
    let end: usize = match arguments.get(1) {
        Some(value) => relative_index(value, argument_nodes[1].span, length)?,
        None => length,
    };
    Ok(Value::String(text.slice_units(start, end)))
}

/// String.split 不接受 RegExp；普通分隔符和 limit 均按 Web 标量规则转换。
fn evaluate_string_split(
    receiver: Value,
    arguments: &[Value],
    argument_nodes: &[Expression<'_>],
) -> Result<Value, EvalError> {
    let Value::String(text) = receiver else {
        unreachable!("String.split 必须绑定 String 接收者")
    };
    let limit: usize = match arguments.get(1) {
        Some(value) => {
            let number: f64 = to_number(value)
                .ok_or(EvalError::InvalidNumericConversion(argument_nodes[1].span))?;
            to_uint32(number) as usize
        }
        None => u32::MAX as usize,
    };
    if limit == 0 {
        return Ok(Value::array(Vec::new()));
    }

    let separator: Option<TextValue> = match arguments.first() {
        None | Some(Value::Undefined) => None,
        Some(value) => Some(
            to_string(value).ok_or(EvalError::InvalidStringConversion(argument_nodes[0].span))?,
        ),
    };
    let Some(separator) = separator else {
        return Ok(Value::array(vec![Value::String(text)]));
    };
    let parts: Vec<Value> = split_text(&text, &separator, limit)
        .into_iter()
        .map(Value::String)
        .collect();
    Ok(Value::array(parts))
}

/// 按 UTF-16 码元序列查找分隔符并切分；空分隔符按单码元切分。
fn split_text(text: &TextValue, separator: &TextValue, limit: usize) -> Vec<TextValue> {
    if separator.is_empty() {
        return text
            .as_units()
            .iter()
            .take(limit)
            .map(|unit: &u16| TextValue::from_units(vec![*unit]))
            .collect();
    }

    let text_units: &[u16] = text.as_units();
    let separator_units: &[u16] = separator.as_units();
    let mut parts: Vec<TextValue> = Vec::new();
    let mut start: usize = 0;
    while parts.len() < limit {
        let found: Option<usize> = text_units[start..]
            .windows(separator_units.len())
            .position(|window: &[u16]| window == separator_units)
            .map(|offset: usize| start + offset);
        let Some(end) = found else {
            parts.push(text.slice_units(start, text.len()));
            break;
        };
        parts.push(text.slice_units(start, end));
        start = end + separator.len();
    }
    parts
}

/// Array.join 使用受控标量转换；嵌套数组始终以逗号递归连接。
fn evaluate_array_join(
    receiver: Value,
    arguments: &[Value],
    argument_nodes: &[Expression<'_>],
    call_span: Span,
) -> Result<Value, EvalError> {
    let Value::Array(items) = receiver else {
        unreachable!("Array.join 必须绑定 Array 接收者")
    };
    let separator: TextValue = match arguments.first() {
        None | Some(Value::Undefined) => TextValue::from(","),
        Some(value) => {
            to_string(value).ok_or(EvalError::InvalidStringConversion(argument_nodes[0].span))?
        }
    };
    let joined: TextValue =
        items.with(|values: &Vec<Value>| join_array(values, &separator, call_span))?;
    Ok(Value::String(joined))
}

/// 递归连接数组元素；嵌套数组固定以逗号分隔，函数与对象拒绝转换。
fn join_array(
    items: &[Value],
    separator: &TextValue,
    error_span: Span,
) -> Result<TextValue, EvalError> {
    let mut joined: TextValue = TextValue::from("");
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            joined.append(separator);
        }
        let part: TextValue = match item {
            Value::Undefined | Value::Null => TextValue::from(""),
            Value::Array(values) => values
                .with(|items: &Vec<Value>| join_array(items, &TextValue::from(","), error_span))?,
            Value::Callable(_) | Value::Object(_) => {
                return Err(EvalError::InvalidStringConversion(error_span));
            }
            value => to_string(value).expect("Boolean、Number 与 String 必须可执行受控字符串转换"),
        };
        joined.append(&part);
    }
    Ok(joined)
}

/// Array.indexOf 从可选起点开始执行严格标量搜索。
fn evaluate_array_index_of(
    receiver: Value,
    arguments: &[Value],
    argument_nodes: &[Expression<'_>],
) -> Result<Value, EvalError> {
    let Value::Array(items) = receiver else {
        unreachable!("Array.indexOf 必须绑定 Array 接收者")
    };
    let search: &Value = &arguments[0];
    let start: usize = match arguments.get(1) {
        Some(value) => search_start_index(value, argument_nodes[1].span, items.len())?,
        None => 0,
    };
    let found: Option<usize> = items.with(|values: &Vec<Value>| {
        values
            .iter()
            .enumerate()
            .skip(start)
            .find_map(|(index, item): (usize, &Value)| strict_equal(item, search).then_some(index))
    });
    Ok(Value::Number(
        found.map_or(-1.0, |index: usize| index as f64),
    ))
}

/// indexOf 的负起点相对末尾计算，正无穷直接从数组末尾开始。
fn search_start_index(value: &Value, span: Span, length: usize) -> Result<usize, EvalError> {
    let integer: f64 = to_integer(value, span)?;
    if integer == f64::NEG_INFINITY {
        return Ok(0);
    }
    if integer == f64::INFINITY {
        return Ok(length);
    }

    let length_number: f64 = length as f64;
    let start: f64 = if integer < 0.0 {
        (length_number + integer).max(0.0)
    } else {
        integer.min(length_number)
    };
    Ok(start as usize)
}

/// Array.slice 允许省略起止边界，并把负边界换算为相对数组末尾的位置。
fn evaluate_array_slice(
    receiver: Value,
    arguments: &[Value],
    argument_nodes: &[Expression<'_>],
) -> Result<Value, EvalError> {
    let Value::Array(items) = receiver else {
        unreachable!("Array.slice 必须绑定 Array 接收者")
    };
    let length: usize = items.len();
    let start: usize = match arguments.first() {
        Some(value) => relative_index(value, argument_nodes[0].span, length)?,
        None => 0,
    };
    let end: usize = match arguments.get(1) {
        Some(value) => relative_index(value, argument_nodes[1].span, length)?,
        None => length,
    };
    let count: usize = end.saturating_sub(start);
    let values: Vec<Value> =
        items.with(|values: &Vec<Value>| values.iter().skip(start).take(count).cloned().collect());
    Ok(Value::array(values))
}

/// Array.splice 先计算完整区间，授权后再一次性替换共享数组。
fn evaluate_array_splice(
    receiver: Value,
    arguments: Vec<Value>,
    argument_nodes: &[Expression<'_>],
    call_span: Span,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let Value::Array(items) = receiver else {
        unreachable!("Array.splice 必须绑定 Array 接收者")
    };
    let length: usize = items.len();
    let start: usize = match arguments.first() {
        Some(value) => relative_index(value, argument_nodes[0].span, length)?,
        None => 0,
    };
    let delete_count: usize = match arguments.get(1) {
        Some(value) => {
            let count: f64 = to_integer(value, argument_nodes[1].span)?;
            if count <= 0.0 || count == f64::NEG_INFINITY {
                0
            } else if count == f64::INFINITY {
                length - start
            } else {
                (count as usize).min(length - start)
            }
        }
        None if arguments.is_empty() => 0,
        None => length - start,
    };
    let replacements: Vec<Value> = arguments.into_iter().skip(2).collect();

    session.context.authorize_reference_write(call_span)?;
    let removed: Vec<Value> = items.with_mut(|values: &mut Vec<Value>| {
        values
            .splice(start..start + delete_count, replacements)
            .collect()
    });
    Ok(Value::array(removed))
}

/// 把 Web 风格数值边界限制到 `0..=length`。
fn relative_index(value: &Value, span: Span, length: usize) -> Result<usize, EvalError> {
    let integer: f64 = to_integer(value, span)?;
    if integer == f64::NEG_INFINITY {
        return Ok(0);
    }
    if integer == f64::INFINITY {
        return Ok(length);
    }

    let length_number: f64 = length as f64;
    let relative: f64 = if integer < 0.0 {
        (length_number + integer).max(0.0)
    } else {
        integer.min(length_number)
    };
    Ok(relative as usize)
}

/// Array.at 使用 Web 的整数化和负索引规则，越界不视为错误。
fn evaluate_array_at(
    receiver: Value,
    argument: &Value,
    argument_span: Span,
) -> Result<Value, EvalError> {
    let Value::Array(items) = receiver else {
        unreachable!("Array.at 必须绑定 Array 接收者")
    };
    let index: f64 = to_integer(argument, argument_span)?;
    if !index.is_finite() {
        return Ok(Value::Undefined);
    }

    let length: f64 = items.len() as f64;
    let offset: f64 = if index >= 0.0 { index } else { length + index };
    if offset < 0.0 || offset >= length {
        return Ok(Value::Undefined);
    }

    Ok(items.with(|values: &Vec<Value>| values[offset as usize].clone()))
}

/// Web 的 ToIntegerOrInfinity：NaN 变为零，其余数值向零截断。
fn to_integer(value: &Value, span: Span) -> Result<f64, EvalError> {
    let number: f64 = to_number(value).ok_or(EvalError::InvalidNumericConversion(span))?;
    Ok(if number.is_nan() { 0.0 } else { number.trunc() })
}

/// 对齐 ECMAScript trim 使用的 WhiteSpace 与 LineTerminator 集合。
fn is_web_whitespace(unit: u16) -> bool {
    matches!(
        unit,
        0x0009 | 0x000A | 0x000B | 0x000C | 0x000D | 0x0020 | 0x00A0 | 0x1680 | 0x2000
            ..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000 | 0xFEFF
    )
}

/// 移除文本两端属于 Web 空白集合的码元。
fn trim_web_text(text: &TextValue) -> TextValue {
    let units: &[u16] = text.as_units();
    let start: usize = units
        .iter()
        .position(|unit: &u16| !is_web_whitespace(*unit))
        .unwrap_or(units.len());
    let end: usize = units
        .iter()
        .rposition(|unit: &u16| !is_web_whitespace(*unit))
        .map_or(start, |index: usize| index + 1);
    text.slice_units(start, end)
}

/// 三种字符串查找共享相同的接收者与参数类型约束。
fn evaluate_string_search(
    receiver: Value,
    method: NativeMethod,
    argument: &Value,
    argument_span: Span,
) -> Result<Value, EvalError> {
    let Value::String(text) = receiver else {
        unreachable!("String 原生方法必须绑定 String 接收者")
    };
    let Value::String(search) = argument else {
        return Err(EvalError::InvalidStringConversion(argument_span));
    };
    let found: bool = match method {
        NativeMethod::StringIncludes => text.contains(search),
        NativeMethod::StringStartsWith => text.starts_with(search),
        NativeMethod::StringEndsWith => text.ends_with(search),
        NativeMethod::ArrayAt
        | NativeMethod::ArrayConcat
        | NativeMethod::ArrayIncludes
        | NativeMethod::ArrayIndexOf
        | NativeMethod::ArrayJoin
        | NativeMethod::ArrayPop
        | NativeMethod::ArrayPush
        | NativeMethod::ArrayShift
        | NativeMethod::ArraySlice
        | NativeMethod::ArraySplice
        | NativeMethod::ArrayUnshift
        | NativeMethod::StringSlice
        | NativeMethod::StringSplit
        | NativeMethod::StringToLowerCase
        | NativeMethod::StringToUpperCase
        | NativeMethod::StringTrim => {
            unreachable!("非查找方法不能进入 String 查找分派")
        }
    };
    Ok(Value::Boolean(found))
}

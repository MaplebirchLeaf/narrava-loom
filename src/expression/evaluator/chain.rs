//! 链式读取、成员访问与原生调用解析。

use super::session::evaluate_in;
use super::*;

/// 链式求值的中间结果；可选链命中空值时短路，不再继续求值。
pub(crate) enum ChainValue {
    Ready(Value),
    ShortCircuited,
}

/// 把全局函数名解析为原生函数身份；未登记的名称返回 `None`。
pub(crate) fn native_function(name: &str) -> Option<NativeFunction> {
    Some(match name {
        "abs" => NativeFunction::Abs,
        "boolean" => NativeFunction::Boolean,
        "ceil" => NativeFunction::Ceil,
        "clamp" => NativeFunction::Clamp,
        "clone" => NativeFunction::Clone,
        "defined" => NativeFunction::Defined,
        "empty" => NativeFunction::Empty,
        "entries" => NativeFunction::Entries,
        "either" => NativeFunction::Either,
        "floor" => NativeFunction::Floor,
        "keys" => NativeFunction::Keys,
        "max" => NativeFunction::Max,
        "min" => NativeFunction::Min,
        "number" => NativeFunction::Number,
        "random" => NativeFunction::Random,
        "round" => NativeFunction::Round,
        "string" => NativeFunction::String,
        "values" => NativeFunction::Values,
        _ => return None,
    })
}

/// 把全局名解析为原生命名空间；目前只登记 Object。
pub(crate) fn native_namespace(name: &str) -> Option<NativeNamespace> {
    match name {
        "Object" => Some(NativeNamespace::Object),
        _ => None,
    }
}

impl ChainValue {
    /// 解包为最终值；可选链短路时返回 undefined。
    pub(crate) fn into_value(self) -> Value {
        match self {
            Self::Ready(value) => value,
            Self::ShortCircuited => Value::Undefined,
        }
    }
}

/// 连续的成员和索引共用一次可选链传播；Group 会经普通求值结束传播。
pub(crate) fn evaluate_chain(
    expression: &Expression<'_>,
    session: &mut EvaluationSession<'_>,
) -> Result<ChainValue, EvalError> {
    match &expression.kind {
        ExpressionKind::Call { callee, arguments } => match evaluate_chain(callee, session)? {
            ChainValue::Ready(callable) => Ok(ChainValue::Ready(call_value(
                callable,
                callee.span,
                arguments,
                expression.span,
                session,
            )?)),
            ChainValue::ShortCircuited => Ok(ChainValue::ShortCircuited),
        },
        ExpressionKind::Member {
            target,
            property,
            property_span,
        } => match evaluate_chain(target, session)? {
            ChainValue::Ready(target) => Ok(ChainValue::Ready(read_member(
                target,
                property,
                *property_span,
            )?)),
            ChainValue::ShortCircuited => Ok(ChainValue::ShortCircuited),
        },
        ExpressionKind::Index { target, index } => match evaluate_chain(target, session)? {
            ChainValue::Ready(target_value) => Ok(ChainValue::Ready(read_index(
                target_value,
                index,
                target.span,
                session,
            )?)),
            ChainValue::ShortCircuited => Ok(ChainValue::ShortCircuited),
        },
        ExpressionKind::OptionalMember {
            target,
            property,
            property_span,
        } => match evaluate_chain(target, session)? {
            ChainValue::Ready(target) if target.is_nullish() => Ok(ChainValue::ShortCircuited),
            ChainValue::Ready(target) => Ok(ChainValue::Ready(read_member(
                target,
                property,
                *property_span,
            )?)),
            ChainValue::ShortCircuited => Ok(ChainValue::ShortCircuited),
        },
        ExpressionKind::OptionalIndex { target, index } => match evaluate_chain(target, session)? {
            ChainValue::Ready(target_value) if target_value.is_nullish() => {
                Ok(ChainValue::ShortCircuited)
            }
            ChainValue::Ready(target_value) => Ok(ChainValue::Ready(read_index(
                target_value,
                index,
                target.span,
                session,
            )?)),
            ChainValue::ShortCircuited => Ok(ChainValue::ShortCircuited),
        },
        ExpressionKind::OptionalCall { callee, arguments } => {
            match evaluate_chain(callee, session)? {
                ChainValue::Ready(callable) if callable.is_nullish() => {
                    Ok(ChainValue::ShortCircuited)
                }
                ChainValue::Ready(callable) => Ok(ChainValue::Ready(call_value(
                    callable,
                    callee.span,
                    arguments,
                    expression.span,
                    session,
                )?)),
                ChainValue::ShortCircuited => Ok(ChainValue::ShortCircuited),
            }
        }
        _ => Ok(ChainValue::Ready(evaluate_in(expression, session)?)),
    }
}

/// 首轮成员读取只开放已经确定语义的自身属性与只读方法。
pub(crate) fn read_member(
    target: Value,
    property: &str,
    property_span: Span,
) -> Result<Value, EvalError> {
    match (&target, property) {
        (Value::Namespace(NativeNamespace::Object), "hasOwn") => Ok(Value::Callable(
            NativeCallable::function(NativeFunction::ObjectHasOwn),
        )),
        (Value::Namespace(NativeNamespace::Object), "assign") => Ok(Value::Callable(
            NativeCallable::function(NativeFunction::ObjectAssign),
        )),
        // Object 点访问首轮只读取字面量建立的自身属性，尚不进入原型表。
        (Value::Object(properties), property) => properties
            .get(property)
            .ok_or(EvalError::UnknownMember(property_span)),
        (Value::Array(items), "length") => Ok(Value::Number(items.len() as f64)),
        (Value::Array(_), "at") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::ArrayAt,
        ))),
        (Value::Array(_), "concat") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::ArrayConcat,
        ))),
        (Value::Array(_), "includes") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::ArrayIncludes,
        ))),
        (Value::Array(_), "indexOf") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::ArrayIndexOf,
        ))),
        (Value::Array(_), "join") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::ArrayJoin,
        ))),
        (Value::Array(_), "pop") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::ArrayPop,
        ))),
        (Value::Array(_), "push") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::ArrayPush,
        ))),
        (Value::Array(_), "shift") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::ArrayShift,
        ))),
        (Value::Array(_), "slice") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::ArraySlice,
        ))),
        (Value::Array(_), "splice") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::ArraySplice,
        ))),
        (Value::Array(_), "unshift") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::ArrayUnshift,
        ))),
        // Web 字符串长度按 UTF-16 码元计数，辅助平面字符占两个单位。
        (Value::String(text), "length") => Ok(Value::Number(text.len() as f64)),
        (Value::String(_), "includes") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::StringIncludes,
        ))),
        (Value::String(_), "slice") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::StringSlice,
        ))),
        (Value::String(_), "split") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::StringSplit,
        ))),
        (Value::String(_), "startsWith") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::StringStartsWith,
        ))),
        (Value::String(_), "endsWith") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::StringEndsWith,
        ))),
        (Value::String(_), "trim") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::StringTrim,
        ))),
        (Value::String(_), "toLowerCase") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::StringToLowerCase,
        ))),
        (Value::String(_), "toUpperCase") => Ok(Value::Callable(NativeCallable::bind(
            target,
            NativeMethod::StringToUpperCase,
        ))),
        _ => Err(EvalError::UnknownMember(property_span)),
    }
}

/// 普通索引读取数组元素、对象自身属性或一个 UTF-16 字符串码元。
pub(crate) fn read_index(
    target_value: Value,
    index: &Expression<'_>,
    target_span: Span,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let index_value: Value = evaluate_in(index, session)?;

    match target_value {
        Value::Array(items) => {
            let property: TextValue = index_property(&index_value, index.span)?;
            if let Some(position) = canonical_index(&property) {
                return Ok(items.with(|values: &Vec<Value>| {
                    values.get(position).cloned().unwrap_or(Value::Undefined)
                }));
            }
            read_member(
                Value::Array(items),
                &property_name(&property, index.span)?,
                index.span,
            )
            .or_else(|error| match error {
                EvalError::UnknownMember(_) => Ok(Value::Undefined),
                other => Err(other),
            })
        }
        Value::Object(properties) => {
            let property: TextValue = index_property(&index_value, index.span)?;
            let property: String = property_name(&property, index.span)?;
            Ok(properties.get(&property).unwrap_or(Value::Undefined))
        }
        Value::Namespace(namespace) => {
            let property: TextValue = index_property(&index_value, index.span)?;
            read_member(
                Value::Namespace(namespace),
                &property_name(&property, index.span)?,
                index.span,
            )
            .or_else(|error| match error {
                EvalError::UnknownMember(_) => Ok(Value::Undefined),
                other => Err(other),
            })
        }
        Value::String(text) => {
            let property: TextValue = index_property(&index_value, index.span)?;
            if let Some(position) = canonical_index(&property) {
                return Ok(text
                    .as_units()
                    .get(position)
                    .map(|unit: &u16| Value::String(TextValue::from_units(vec![*unit])))
                    .unwrap_or(Value::Undefined));
            }
            read_member(
                Value::String(text),
                &property_name(&property, index.span)?,
                index.span,
            )
            .or_else(|error| match error {
                EvalError::UnknownMember(_) => Ok(Value::Undefined),
                other => Err(other),
            })
        }
        _ => Err(EvalError::InvalidIndexTarget(target_span)),
    }
}

/// 把索引表达式的结果转换为属性名文本。
pub(crate) fn index_property(value: &Value, span: Span) -> Result<TextValue, EvalError> {
    to_string(value).ok_or(EvalError::InvalidStringConversion(span))
}

/// 把 UTF-16 文本转换为可比较的属性名；含孤立代理项时转换失败。
pub(crate) fn property_name(value: &TextValue, span: Span) -> Result<String, EvalError> {
    value
        .to_unicode_string()
        .ok_or(EvalError::InvalidStringConversion(span))
}

/// 数组和字符串只把规范十进制属性名解释为元素索引。
pub(crate) fn canonical_index(value: &TextValue) -> Option<usize> {
    let property: String = value.to_unicode_string()?;
    let bytes: &[u8] = property.as_bytes();
    let canonical: bool = property == "0"
        || (bytes
            .first()
            .is_some_and(|byte: &u8| matches!(byte, b'1'..=b'9'))
            && bytes.iter().all(u8::is_ascii_digit));
    canonical.then(|| property.parse().ok()).flatten()
}

/// 普通与可选调用共用参数求值和 callable 检查。
pub(crate) fn call_value(
    callable: Value,
    callee_span: Span,
    argument_nodes: &[Expression<'_>],
    call_span: Span,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let arguments: Result<Vec<Value>, EvalError> = argument_nodes
        .iter()
        .map(|argument: &Expression<'_>| evaluate_in(argument, session))
        .collect();
    let arguments: Vec<Value> = arguments?;
    match callable {
        Value::Callable(callable) => {
            call_native(callable, arguments, argument_nodes, call_span, session)
        }
        Value::ScriptCallable(callable) => {
            session.context.call_script(&callable, arguments, call_span)
        }
        _ => Err(EvalError::NotCallable(callee_span)),
    }
}

/// 调用已绑定的原生方法，并在方法边界检查参数。
pub(crate) fn call_native(
    callable: NativeCallable,
    arguments: Vec<Value>,
    argument_nodes: &[Expression<'_>],
    call_span: Span,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    match callable.into_kind() {
        NativeCallableKind::Function(function) => {
            call_native_function(function, arguments, argument_nodes, call_span, session)
        }
        NativeCallableKind::Method { receiver, method } => call_native_method(
            *receiver,
            method,
            arguments,
            argument_nodes,
            call_span,
            session,
        ),
    }
}

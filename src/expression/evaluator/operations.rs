//! Expression 的一元、二元、比较与成员运算。

use super::{
    EvalError, EvaluationSession, evaluate_in, string_to_number, to_int32, to_number, to_string,
    to_uint32,
};
use crate::expression::{
    BetweenBounds, BinaryOperator, Expression, ExpressionKind, Span, UnaryOperator,
    prototype::{Prototype, is_instance},
    value::{TextValue, Value},
};

/// 二元运算入口：先做支持性检查，再分派短路、成员、相等、三向、关系与算术路径。
pub(super) fn evaluate_binary(
    operator: BinaryOperator,
    left: &Expression<'_>,
    right: &Expression<'_>,
    expression_span: Span,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let supported: bool = matches!(
        operator,
        BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::IntegerDivide
            | BinaryOperator::Remainder
            | BinaryOperator::Power
            | BinaryOperator::BitwiseAnd
            | BinaryOperator::BitwiseOr
            | BinaryOperator::BitwiseXor
            | BinaryOperator::ShiftLeft
            | BinaryOperator::ShiftRight
            | BinaryOperator::UnsignedShiftRight
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
            | BinaryOperator::In
            | BinaryOperator::InstanceOf
            | BinaryOperator::NotIn
            | BinaryOperator::ThreeWayCompare
            | BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::StrictEqual
            | BinaryOperator::StrictNotEqual
            | BinaryOperator::LogicalAnd
            | BinaryOperator::LogicalOr
            | BinaryOperator::NullishCoalesce
    );
    if !supported {
        return Err(EvalError::UnsupportedExpression(expression_span));
    }

    if operator == BinaryOperator::InstanceOf {
        let ExpressionKind::Global(name) = right.kind else {
            return Err(EvalError::InvalidPrototype(right.span));
        };
        let expected: Prototype =
            Prototype::from_name(name).ok_or(EvalError::InvalidPrototype(right.span))?;
        let value: Value = evaluate_in(left, session)?;
        return Ok(Value::Boolean(is_instance(&value, expected)));
    }

    let short_circuit: bool = matches!(
        operator,
        BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr | BinaryOperator::NullishCoalesce
    );
    if short_circuit {
        // 右侧只能在确实被选中时求值，运行时访问和副作用都遵守该边界。
        let left_value: Value = evaluate_in(left, session)?;
        let use_left: bool = match operator {
            BinaryOperator::LogicalAnd => !left_value.is_truthy(),
            BinaryOperator::LogicalOr => left_value.is_truthy(),
            BinaryOperator::NullishCoalesce => !left_value.is_nullish(),
            _ => unreachable!(),
        };
        return if use_left {
            Ok(left_value)
        } else {
            evaluate_in(right, session)
        };
    }

    let left_value: Value = evaluate_in(left, session)?;
    let right_value: Value = evaluate_in(right, session)?;

    if matches!(operator, BinaryOperator::In | BinaryOperator::NotIn) {
        let contains: bool = contains_member(&left_value, left.span, &right_value, right.span)?;
        let result: bool = if operator == BinaryOperator::NotIn {
            !contains
        } else {
            contains
        };
        return Ok(Value::Boolean(result));
    }

    let equality: bool = matches!(
        operator,
        BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::StrictEqual
            | BinaryOperator::StrictNotEqual
    );
    if equality {
        let strict: bool = matches!(
            operator,
            BinaryOperator::StrictEqual | BinaryOperator::StrictNotEqual
        );
        let equal: bool = if strict {
            strict_equal(&left_value, &right_value)
        } else {
            loose_equal(&left_value, &right_value)
        };
        let negate: bool = matches!(
            operator,
            BinaryOperator::NotEqual | BinaryOperator::StrictNotEqual
        );
        return Ok(Value::Boolean(if negate { !equal } else { equal }));
    }

    if operator == BinaryOperator::ThreeWayCompare {
        let ordering: std::cmp::Ordering =
            if let (Value::String(left_string), Value::String(right_string)) =
                (&left_value, &right_value)
            {
                left_string.cmp(right_string)
            } else {
                let left_number: f64 =
                    to_number(&left_value).ok_or(EvalError::InvalidNumericConversion(left.span))?;
                let right_number: f64 = to_number(&right_value)
                    .ok_or(EvalError::InvalidNumericConversion(right.span))?;
                if left_number.is_nan() {
                    return Err(EvalError::UnorderedComparison(left.span));
                }
                if right_number.is_nan() {
                    return Err(EvalError::UnorderedComparison(right.span));
                }
                left_number
                    .partial_cmp(&right_number)
                    .expect("已排除 NaN 的 Number 必须可排序")
            };
        let result: f64 = match ordering {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Equal => 0.0,
            std::cmp::Ordering::Greater => 1.0,
        };
        return Ok(Value::Number(result));
    }

    let relational: bool = matches!(
        operator,
        BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
    );
    if relational {
        let result: bool =
            compare_relational(operator, &left_value, left.span, &right_value, right.span)?;
        return Ok(Value::Boolean(result));
    }

    evaluate_arithmetic_values(operator, left_value, left.span, right_value, right.span)
}

/// 普通二元运算与复合赋值共用完全相同的数值、字符串和位运算规则。
pub(super) fn evaluate_arithmetic_values(
    operator: BinaryOperator,
    left_value: Value,
    left_span: Span,
    right_value: Value,
    right_span: Span,
) -> Result<Value, EvalError> {
    let bitwise: bool = matches!(
        operator,
        BinaryOperator::BitwiseAnd
            | BinaryOperator::BitwiseOr
            | BinaryOperator::BitwiseXor
            | BinaryOperator::ShiftLeft
            | BinaryOperator::ShiftRight
            | BinaryOperator::UnsignedShiftRight
    );
    if bitwise {
        let left_number: f64 =
            to_number(&left_value).ok_or(EvalError::InvalidNumericConversion(left_span))?;
        let right_number: f64 =
            to_number(&right_value).ok_or(EvalError::InvalidNumericConversion(right_span))?;
        let shift: u32 = to_uint32(right_number) & 31;
        let result: f64 = match operator {
            BinaryOperator::BitwiseAnd => f64::from(to_int32(left_number) & to_int32(right_number)),
            BinaryOperator::BitwiseOr => f64::from(to_int32(left_number) | to_int32(right_number)),
            BinaryOperator::BitwiseXor => f64::from(to_int32(left_number) ^ to_int32(right_number)),
            BinaryOperator::ShiftLeft => f64::from(to_int32(left_number).wrapping_shl(shift)),
            BinaryOperator::ShiftRight => f64::from(to_int32(left_number) >> shift),
            BinaryOperator::UnsignedShiftRight => f64::from(to_uint32(left_number) >> shift),
            _ => unreachable!(),
        };
        return Ok(Value::Number(result));
    }

    if operator == BinaryOperator::Add
        && (matches!(left_value, Value::String(_)) || matches!(right_value, Value::String(_)))
    {
        let mut left_string: TextValue =
            to_string(&left_value).ok_or(EvalError::InvalidStringConversion(left_span))?;
        let right_string: TextValue =
            to_string(&right_value).ok_or(EvalError::InvalidStringConversion(right_span))?;
        left_string.append(&right_string);
        return Ok(Value::String(left_string));
    }

    let left_number: f64 =
        to_number(&left_value).ok_or(EvalError::InvalidNumericConversion(left_span))?;
    let right_number: f64 =
        to_number(&right_value).ok_or(EvalError::InvalidNumericConversion(right_span))?;
    let result: f64 = match operator {
        BinaryOperator::Add => left_number + right_number,
        BinaryOperator::Subtract => left_number - right_number,
        BinaryOperator::Multiply => left_number * right_number,
        BinaryOperator::Divide => left_number / right_number,
        BinaryOperator::IntegerDivide => (left_number / right_number).trunc(),
        BinaryOperator::Remainder => left_number % right_number,
        BinaryOperator::Power => left_number.powf(right_number),
        _ => unreachable!(),
    };

    Ok(Value::Number(result))
}

/// 成员判断只读取集合自身内容，受控原型属性留给原型系统处理。
fn contains_member(
    needle: &Value,
    needle_span: Span,
    container: &Value,
    container_span: Span,
) -> Result<bool, EvalError> {
    match container {
        Value::Array(items) => items.with(|values: &Vec<Value>| {
            for item in values {
                if strict_equal(needle, item) {
                    return Ok(true);
                }
            }
            Ok(false)
        }),
        Value::Object(properties) => {
            let key: TextValue =
                to_string(needle).ok_or(EvalError::InvalidStringConversion(needle_span))?;
            let key: String = key
                .to_unicode_string()
                .ok_or(EvalError::InvalidStringConversion(needle_span))?;
            Ok(properties
                .with(|values: &Vec<(String, Value)>| values.iter().any(|(name, _)| name == &key)))
        }
        Value::String(text) => {
            let Value::String(fragment) = needle else {
                return Err(EvalError::InvalidStringConversion(needle_span));
            };
            Ok(text.contains(fragment))
        }
        _ => Err(EvalError::InvalidMembershipTarget(container_span)),
    }
}

/// 区间按源码顺序各求值一次，再复用普通关系比较规则检查两侧。
pub(super) fn evaluate_between(
    bounds: BetweenBounds,
    value: &Expression<'_>,
    lower: &Expression<'_>,
    upper: &Expression<'_>,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let value_result: Value = evaluate_in(value, session)?;
    let lower_result: Value = evaluate_in(lower, session)?;
    let upper_result: Value = evaluate_in(upper, session)?;

    let lower_operator: BinaryOperator = match bounds {
        BetweenBounds::OpenOpen | BetweenBounds::OpenClosed => BinaryOperator::Less,
        BetweenBounds::ClosedOpen | BetweenBounds::ClosedClosed => BinaryOperator::LessEqual,
    };
    let upper_operator: BinaryOperator = match bounds {
        BetweenBounds::OpenOpen | BetweenBounds::ClosedOpen => BinaryOperator::Less,
        BetweenBounds::OpenClosed | BetweenBounds::ClosedClosed => BinaryOperator::LessEqual,
    };
    let inside_lower: bool = compare_relational(
        lower_operator,
        &lower_result,
        lower.span,
        &value_result,
        value.span,
    )?;
    let inside_upper: bool = compare_relational(
        upper_operator,
        &value_result,
        value.span,
        &upper_result,
        upper.span,
    )?;

    Ok(Value::Boolean(inside_lower && inside_upper))
}

/// 关系比较同时服务普通二元比较和区间边界判断。
fn compare_relational(
    operator: BinaryOperator,
    left: &Value,
    left_span: Span,
    right: &Value,
    right_span: Span,
) -> Result<bool, EvalError> {
    if let (Value::String(left_string), Value::String(right_string)) = (left, right) {
        // Web 字符串关系比较使用 UTF-16 码元，而不是 Unicode 标量顺序。
        let ordering: std::cmp::Ordering = left_string.cmp(right_string);
        return Ok(match operator {
            BinaryOperator::Less => ordering.is_lt(),
            BinaryOperator::LessEqual => ordering.is_le(),
            BinaryOperator::Greater => ordering.is_gt(),
            BinaryOperator::GreaterEqual => ordering.is_ge(),
            _ => unreachable!(),
        });
    }

    let left_number: f64 = to_number(left).ok_or(EvalError::InvalidNumericConversion(left_span))?;
    let right_number: f64 =
        to_number(right).ok_or(EvalError::InvalidNumericConversion(right_span))?;
    Ok(match operator {
        BinaryOperator::Less => left_number < right_number,
        BinaryOperator::LessEqual => left_number <= right_number,
        BinaryOperator::Greater => left_number > right_number,
        BinaryOperator::GreaterEqual => left_number >= right_number,
        _ => unreachable!(),
    })
}

/// 严格相等只比较同类型标量；`NaN` 与自身也不相等。
pub(super) fn strict_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Array(left), Value::Array(right)) => left.same_identity(right),
        (Value::Callable(left), Value::Callable(right)) => left.same_identity(right),
        (Value::Object(left), Value::Object(right)) => left.same_identity(right),
        _ => false,
    }
}

/// 非严格相等遵循 Web 标量转换顺序，但不触发对象到原始值转换。
fn loose_equal(left: &Value, right: &Value) -> bool {
    if std::mem::discriminant(left) == std::mem::discriminant(right) {
        return strict_equal(left, right);
    }

    match (left, right) {
        (Value::Null, Value::Undefined) | (Value::Undefined, Value::Null) => true,
        (Value::Number(left), Value::String(right)) => *left == string_to_number(right),
        (Value::String(left), Value::Number(right)) => string_to_number(left) == *right,
        (Value::Boolean(left), right) => {
            let number: Value = Value::Number(if *left { 1.0 } else { 0.0 });
            loose_equal(&number, right)
        }
        (left, Value::Boolean(right)) => {
            let number: Value = Value::Number(if *right { 1.0 } else { 0.0 });
            loose_equal(left, &number)
        }
        _ => false,
    }
}

/// 求值不需要运行时上下文的一元运算。
pub(super) fn evaluate_unary(
    operator: UnaryOperator,
    operand: &Expression<'_>,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let value: Value = evaluate_in(operand, session)?;
    if operator == UnaryOperator::TypeOf {
        return Ok(Value::string(value.type_name()));
    }
    if operator == UnaryOperator::LogicalNot {
        return Ok(Value::Boolean(!value.is_truthy()));
    }

    let number: f64 = to_number(&value).ok_or(EvalError::InvalidNumericConversion(operand.span))?;
    let result: f64 = match operator {
        UnaryOperator::Positive => number,
        UnaryOperator::Negative => -number,
        UnaryOperator::BitwiseNot => f64::from(!to_int32(number)),
        UnaryOperator::LogicalNot | UnaryOperator::TypeOf => unreachable!(),
    };
    Ok(Value::Number(result))
}

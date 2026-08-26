//! 求值会话、上下文访问与赋值降级。

use super::chain::{ChainValue, evaluate_chain, native_function, native_namespace};
use super::*;

/// 只读或可写 Context 的统一访问门面；读取方法对两种模式等价。
pub(crate) enum ContextAccess<'a> {
    Read(&'a dyn EvaluationContext),
    Write(&'a mut dyn WritableEvaluationContext),
}

impl ContextAccess<'_> {
    /// 只读查询 State.global；Read 与 Write 模式行为一致。
    pub(crate) fn global(&self, name: &str) -> Option<&Value> {
        match self {
            Self::Read(context) => context.global(name),
            Self::Write(context) => context.global(name),
        }
    }

    /// 只读查询 setup State；Read 与 Write 模式行为一致。
    pub(crate) fn setup(&self) -> Option<&Value> {
        match self {
            Self::Read(context) => context.setup(),
            Self::Write(context) => context.setup(),
        }
    }

    /// 只读查询指定作用域的变量绑定；Read 与 Write 模式行为一致。
    pub(crate) fn variable(&self, scope: VariableScope, name: &str) -> Option<&Value> {
        match self {
            Self::Read(context) => context.variable(scope, name),
            Self::Write(context) => context.variable(scope, name),
        }
    }

    /// 取回可变 Context；只读会话在此报出 MissingWriteContext。
    pub(crate) fn writer(
        &mut self,
        span: Span,
    ) -> Result<&mut dyn WritableEvaluationContext, EvalError> {
        match self {
            Self::Read(_) => Err(EvalError::MissingWriteContext(span)),
            Self::Write(context) => Ok(*context),
        }
    }

    /// 修改型原生函数在触碰共享引用前单独取得授权。
    pub(crate) fn authorize_reference_write(&mut self, span: Span) -> Result<(), EvalError> {
        match self {
            Self::Read(_) => Err(EvalError::MissingWriteContext(span)),
            Self::Write(context) => context
                .authorize_reference_write()
                .map_err(|ContextWriteError::Rejected| EvalError::ContextWriteRejected(span)),
        }
    }

    /// 把脚本可调用值交回 Binding 执行；只读会话或调用失败都映射为 EvalError。
    pub(crate) fn call_script(
        &mut self,
        callable: &ScriptCallable,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, EvalError> {
        let writer: &mut dyn WritableEvaluationContext = self.writer(span)?;
        writer
            .call_script(callable, arguments)
            .map_err(|_error: ScriptCallError| EvalError::ScriptCallFailed(span))
    }
}

/// 一次求值会话：持有 Context 访问方式与可选随机源，随求值调用创建。
pub(crate) struct EvaluationSession<'a> {
    pub(crate) context: ContextAccess<'a>,
    pub(crate) random: Option<&'a mut dyn RandomSource>,
}

/// 不提供任何全局值、变量与 setup 的只读 Context。
pub(crate) struct EmptyContext;

impl EvaluationContext for EmptyContext {
    fn global(&self, _name: &str) -> Option<&Value> {
        None
    }
}

/// 求值单个 Expression 节点；链式表达式与赋值目标走专门路径。
pub(crate) fn evaluate_in(
    expression: &Expression<'_>,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    match &expression.kind {
        ExpressionKind::Assignment {
            operator,
            target,
            value,
        } => evaluate_assignment(*operator, target, value, expression.span, session),
        ExpressionKind::Array(items) => {
            let values: Result<Vec<Value>, EvalError> = items
                .iter()
                .map(|item: &Expression<'_>| evaluate_in(item, session))
                .collect();
            Ok(Value::array(values?))
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => evaluate_binary(*operator, left, right, expression.span, session),
        ExpressionKind::Between {
            bounds,
            value,
            lower,
            upper,
        } => evaluate_between(*bounds, value, lower, upper, session),
        ExpressionKind::Conditional {
            condition,
            consequent,
            alternate,
        } => {
            let condition: Value = evaluate_in(condition, session)?;
            // 条件只决定分支，返回值保持被选分支的原始类型。
            if condition.is_truthy() {
                evaluate_in(consequent, session)
            } else {
                evaluate_in(alternate, session)
            }
        }
        ExpressionKind::Undefined => Ok(Value::Undefined),
        ExpressionKind::Null => Ok(Value::Null),
        ExpressionKind::Boolean(value) => Ok(Value::Boolean(*value)),
        ExpressionKind::Number(source) => {
            let value: f64 = source
                .parse()
                .map_err(|_| EvalError::InvalidNumber(expression.span))?;
            Ok(Value::Number(value))
        }
        // Lexer 只负责确认引号边界，具体转义规则由求值层统一解释。
        ExpressionKind::String(value) => {
            let value: String = decode_string(value, expression.span)?;
            Ok(Value::string(value))
        }
        ExpressionKind::Global(name) => {
            // 引擎保留函数先于 State.global 解析，且不穿透宿主全局对象。
            if let Some(function) = native_function(name) {
                Ok(Value::Callable(NativeCallable::function(function)))
            } else if let Some(namespace) = native_namespace(name) {
                Ok(Value::Namespace(namespace))
            } else {
                session
                    .context
                    .global(name)
                    .cloned()
                    .ok_or(EvalError::UnknownGlobal(expression.span))
            }
        }
        ExpressionKind::Group(inner) => evaluate_in(inner, session),
        ExpressionKind::Setup => session
            .context
            .setup()
            .cloned()
            .ok_or(EvalError::MissingSetup(expression.span)),
        ExpressionKind::Variable { scope, name } => Ok(session
            .context
            .variable(*scope, name)
            .cloned()
            .unwrap_or(Value::Undefined)),
        ExpressionKind::Call { .. }
        | ExpressionKind::Index { .. }
        | ExpressionKind::Member { .. }
        | ExpressionKind::OptionalCall { .. }
        | ExpressionKind::OptionalIndex { .. }
        | ExpressionKind::OptionalMember { .. } => {
            let result: ChainValue = evaluate_chain(expression, session)?;
            Ok(result.into_value())
        }
        ExpressionKind::Object(properties) => {
            let mut values: Vec<(String, Value)> = Vec::with_capacity(properties.len());

            for property in properties {
                // 字符串键与字符串值共享完全相同的转义规则。
                let key: String = match property.key {
                    ObjectKey::Identifier(key) => String::from(key),
                    ObjectKey::String(key) => decode_string(key, property.key_span)?,
                };
                let value: Value = evaluate_in(&property.value, session)?;
                let existing: Option<&mut (String, Value)> = values
                    .iter_mut()
                    .find(|(existing_key, _)| existing_key == &key);

                if let Some(existing) = existing {
                    // 与 JS 对象字面量一致：后值覆盖前值，键的原始顺序不变。
                    existing.1 = value;
                } else {
                    values.push((key, value));
                }
            }

            Ok(Value::object(values))
        }
        ExpressionKind::Unary { operator, operand } => evaluate_unary(*operator, operand, session),
        ExpressionKind::Update {
            operator,
            position,
            target,
        } => evaluate_update(*operator, *position, target, session),
    }
}

/// 前缀/后缀自增自减：解析目标路径、读旧值、提交新值，再按位置决定返回值。
fn evaluate_update(
    operator: UpdateOperator,
    position: UpdatePosition,
    target: &Expression<'_>,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let path: AssignmentPath = AssignmentPath::resolve(target, session)?;
    let mut root: Value = path.read_root(session)?;
    let current: Value = path.read_value(&root)?;
    let number: f64 =
        to_number(&current).ok_or(EvalError::InvalidNumericConversion(target.span))?;
    let updated: f64 = match operator {
        UpdateOperator::Increment => number + 1.0,
        UpdateOperator::Decrement => number - 1.0,
    };
    path.commit_value(&mut root, Value::Number(updated), session)?;

    Ok(Value::Number(match position {
        UpdatePosition::Prefix => updated,
        UpdatePosition::Postfix => number,
    }))
}

/// 赋值求值：普通赋值直接写入，复合赋值先读当前值，短路赋值按真假值决定是否写入。
fn evaluate_assignment(
    operator: AssignmentOperator,
    target: &Expression<'_>,
    value: &Expression<'_>,
    expression_span: Span,
    session: &mut EvaluationSession<'_>,
) -> Result<Value, EvalError> {
    let path: AssignmentPath = AssignmentPath::resolve(target, session)?;
    if operator == AssignmentOperator::Assign {
        let result: Value = evaluate_in(value, session)?;
        if path.members.is_empty() {
            path.write_root(result.clone(), session)?;
        } else {
            let mut root: Value = path.read_root(session)?;
            path.commit_value(&mut root, result.clone(), session)?;
        }
        return Ok(result);
    }

    let mut root: Value = path.read_root(session)?;
    let current: Value = path.read_value(&root)?;
    let skip_write: bool = match operator {
        AssignmentOperator::LogicalAnd => !current.is_truthy(),
        AssignmentOperator::LogicalOr => current.is_truthy(),
        AssignmentOperator::NullishCoalesce => !current.is_nullish(),
        _ => false,
    };
    if skip_write {
        return Ok(current);
    }

    let right: Value = evaluate_in(value, session)?;
    let result: Value = if matches!(
        operator,
        AssignmentOperator::LogicalAnd
            | AssignmentOperator::LogicalOr
            | AssignmentOperator::NullishCoalesce
    ) {
        right
    } else {
        let binary: BinaryOperator = assignment_binary_operator(operator)
            .ok_or(EvalError::UnsupportedExpression(expression_span))?;
        evaluate_arithmetic_values(binary, current, target.span, right, value.span)?
    };
    path.commit_value(&mut root, result.clone(), session)?;
    Ok(result)
}

/// 把复合赋值运算符映射回对应的二元运算符；普通赋值与短路赋值返回 `None`。
fn assignment_binary_operator(operator: AssignmentOperator) -> Option<BinaryOperator> {
    match operator {
        AssignmentOperator::Add => Some(BinaryOperator::Add),
        AssignmentOperator::BitwiseAnd => Some(BinaryOperator::BitwiseAnd),
        AssignmentOperator::BitwiseOr => Some(BinaryOperator::BitwiseOr),
        AssignmentOperator::BitwiseXor => Some(BinaryOperator::BitwiseXor),
        AssignmentOperator::Divide => Some(BinaryOperator::Divide),
        AssignmentOperator::IntegerDivide => Some(BinaryOperator::IntegerDivide),
        AssignmentOperator::Multiply => Some(BinaryOperator::Multiply),
        AssignmentOperator::Power => Some(BinaryOperator::Power),
        AssignmentOperator::Remainder => Some(BinaryOperator::Remainder),
        AssignmentOperator::ShiftLeft => Some(BinaryOperator::ShiftLeft),
        AssignmentOperator::ShiftRight => Some(BinaryOperator::ShiftRight),
        AssignmentOperator::Subtract => Some(BinaryOperator::Subtract),
        AssignmentOperator::UnsignedShiftRight => Some(BinaryOperator::UnsignedShiftRight),
        AssignmentOperator::Assign
        | AssignmentOperator::LogicalAnd
        | AssignmentOperator::LogicalOr
        | AssignmentOperator::NullishCoalesce => None,
    }
}

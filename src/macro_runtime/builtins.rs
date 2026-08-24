//! 不直接产生 Presentation 输出的原生逻辑 Macro。

mod presentation;
mod story;

pub use presentation::*;
pub use story::*;

use crate::expression::{
    Expression,
    evaluator::{EvalError, WritableEvaluationContext, delete_with_mut, evaluate_with_mut},
    value::Value,
};

/// 执行 `<<run expression>>`，保留副作用但丢弃表达式结果。
pub fn run(
    expression: &Expression<'_>,
    context: &mut dyn WritableEvaluationContext,
) -> Result<Value, EvalError> {
    execute_discarded(expression, context)
}

/// 执行 HIR 已验证的 `<<set target = value>>` 赋值节点。
///
/// 源码中的 `to` 已在 HIR 阶段归一化为普通赋值，这里不再区分两种拼写。
pub fn set(
    assignment: &Expression<'_>,
    context: &mut dyn WritableEvaluationContext,
) -> Result<Value, EvalError> {
    execute_discarded(assignment, context)
}

/// 执行 `<<unset target>>`，真正删除目标并丢弃被删除的旧值。
pub fn unset(
    target: &Expression<'_>,
    context: &mut dyn WritableEvaluationContext,
) -> Result<Value, EvalError> {
    let _deleted: Option<Value> = delete_with_mut(target, context)?;
    Ok(Value::Undefined)
}

/// 逻辑语句保留 Expression 副作用，但不把中间值当作 Macro 输出。
fn execute_discarded(
    expression: &Expression<'_>,
    context: &mut dyn WritableEvaluationContext,
) -> Result<Value, EvalError> {
    let _result: Value = evaluate_with_mut(expression, context)?;
    Ok(Value::Undefined)
}

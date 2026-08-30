//! 不直接产生 SemanticOutput 输出的 HIR 逻辑节点分派。

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    expression::{
        evaluator::{EvalError, assign_value_with_mut, evaluate_with_mut, values_strict_equal},
        value::Value,
    },
    hir::{HirBodyKind, HirBodyNode, HirFor, HirForKind, HirIf, HirPrint, HirSwitch, HirWhile},
    macro_runtime::{
        MacroLogicContext, MacroStoryAccess, StoryMacroError, goto, include, run, set, unset,
    },
    twee,
};

use super::{BodyControl, execute_hir_body};

/// 首组逻辑节点执行时保留各自所属边界的错误。
#[derive(Debug, PartialEq)]
pub enum LogicNodeError<StoryError> {
    ExecutionLimitExceeded { limit: usize },
    Evaluation(EvalError),
    InvalidText(crate::expression::Span),
    Story(StoryMacroError<StoryError>),
    UnsupportedNode(twee::Span),
}

impl<StoryError> LogicNodeError<StoryError> {
    /// 产生稳定 Diagnostic；源码位置由持有 HIR Passage 的上层附加。
    pub fn diagnostic(self, convert_story: impl FnOnce(StoryError) -> Diagnostic) -> Diagnostic {
        match self {
            Self::ExecutionLimitExceeded { limit } => Diagnostic::new(
                "runtime.logic.execution_limit_exceeded",
                DiagnosticSeverity::Error,
                &format!("同步逻辑执行超过步骤预算：{limit}"),
            ),
            Self::Evaluation(error) => error.diagnostic(),
            Self::InvalidText(_) => Diagnostic::new(
                "runtime.invalid_text_value",
                DiagnosticSeverity::Error,
                "Expression 结果不能转换为语义 Text",
            ),
            Self::Story(error) => error.diagnostic(convert_story),
            Self::UnsupportedNode(_) => Diagnostic::new(
                "runtime.unsupported_hir_node",
                DiagnosticSeverity::Error,
                "Runtime 尚未支持该 HIR 正文节点",
            ),
        }
    }
}

/// 执行当前阶段已经接入的单个逻辑 HIR 节点。
pub fn execute_logic_node<Story>(
    node: &HirBodyNode<'_>,
    context: &mut MacroLogicContext<'_, Story>,
) -> Result<BodyControl, LogicNodeError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    if !context.consume_execution_step() {
        return Err(LogicNodeError::ExecutionLimitExceeded {
            limit: context.execution_limit(),
        });
    }
    match &node.kind {
        HirBodyKind::Text(_) => Ok(BodyControl::Continue),
        HirBodyKind::Print(HirPrint::Literal(_)) => Ok(BodyControl::Continue),
        HirBodyKind::Print(HirPrint::Expression(expression)) => {
            let _value: Value =
                evaluate_with_mut(expression, context).map_err(LogicNodeError::Evaluation)?;
            Ok(BodyControl::Continue)
        }
        HirBodyKind::Run(expression) => {
            let _output: Value = run(expression, context).map_err(LogicNodeError::Evaluation)?;
            Ok(BodyControl::Continue)
        }
        HirBodyKind::Set(assignment) => {
            let _output: Value = set(assignment, context).map_err(LogicNodeError::Evaluation)?;
            Ok(BodyControl::Continue)
        }
        HirBodyKind::Unset(target) => {
            let _output: Value = unset(target, context).map_err(LogicNodeError::Evaluation)?;
            Ok(BodyControl::Continue)
        }
        HirBodyKind::Include(expression) => {
            include(expression, context).map_err(LogicNodeError::Story)
        }
        HirBodyKind::Goto(expression) => goto(expression, context).map_err(LogicNodeError::Story),
        HirBodyKind::If(conditional) => execute_if(conditional, context),
        HirBodyKind::Switch(switch) => execute_switch(switch, context),
        HirBodyKind::While(loop_node) => execute_while(loop_node, context),
        HirBodyKind::For(loop_node) => execute_for(loop_node, context),
        HirBodyKind::Break => Ok(BodyControl::BreakLoop),
        HirBodyKind::Continue => Ok(BodyControl::ContinueLoop),
        HirBodyKind::Exit => Ok(BodyControl::ExitScope),
        _ => Err(LogicNodeError::UnsupportedNode(node.span)),
    }
}

/// `in` 与 `of` 在循环开始时建立稳定快照，再逐轮写入目标。
fn execute_for<Story>(
    loop_node: &HirFor<'_>,
    context: &mut MacroLogicContext<'_, Story>,
) -> Result<BodyControl, LogicNodeError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    let values: Vec<Value> = match &loop_node.kind {
        HirForKind::In { collection, .. } => {
            let span: crate::expression::Span = collection.span;
            let collection_value: Value =
                evaluate_with_mut(collection, context).map_err(LogicNodeError::Evaluation)?;
            collection_iteration_values(collection_value, true, span)
                .map_err(LogicNodeError::Evaluation)?
        }
        HirForKind::Of { collection, .. } => {
            let span: crate::expression::Span = collection.span;
            let collection_value: Value =
                evaluate_with_mut(collection, context).map_err(LogicNodeError::Evaluation)?;
            collection_iteration_values(collection_value, false, span)
                .map_err(LogicNodeError::Evaluation)?
        }
        HirForKind::Range {
            start, end, step, ..
        } => return execute_for_range(loop_node, start, end, step.as_ref(), context),
    };

    for value in values {
        if !context.consume_execution_step() {
            return Err(LogicNodeError::ExecutionLimitExceeded {
                limit: context.execution_limit(),
            });
        }
        if let Some(control) = execute_for_value(loop_node, value, context)? {
            return Ok(control);
        }
    }
    Ok(BodyControl::Continue)
}

/// Range 包含终点；边界与步长在进入循环前各求值一次。
fn execute_for_range<Story>(
    loop_node: &HirFor<'_>,
    start: &crate::expression::Expression<'_>,
    end: &crate::expression::Expression<'_>,
    step: Option<&crate::expression::Expression<'_>>,
    context: &mut MacroLogicContext<'_, Story>,
) -> Result<BodyControl, LogicNodeError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    let start_value: Value =
        evaluate_with_mut(start, context).map_err(LogicNodeError::Evaluation)?;
    let end_value: Value = evaluate_with_mut(end, context).map_err(LogicNodeError::Evaluation)?;
    let start_number: f64 =
        finite_range_number(start_value, start.span).map_err(LogicNodeError::Evaluation)?;
    let end_number: f64 =
        finite_range_number(end_value, end.span).map_err(LogicNodeError::Evaluation)?;
    let step_number: f64 = match step {
        Some(expression) => {
            let value: Value =
                evaluate_with_mut(expression, context).map_err(LogicNodeError::Evaluation)?;
            finite_range_number(value, expression.span).map_err(LogicNodeError::Evaluation)?
        }
        None if start_number <= end_number => 1.0,
        None => -1.0,
    };
    let step_span: crate::expression::Span = step.map_or(start.span, |value| value.span);
    if step_number == 0.0
        || (start_number < end_number && step_number < 0.0)
        || (start_number > end_number && step_number > 0.0)
    {
        return Err(LogicNodeError::Evaluation(EvalError::InvalidRange(
            step_span,
        )));
    }

    let ascending: bool = step_number > 0.0;
    let mut current: f64 = start_number;
    while if ascending {
        current <= end_number
    } else {
        current >= end_number
    } {
        if !context.consume_execution_step() {
            return Err(LogicNodeError::ExecutionLimitExceeded {
                limit: context.execution_limit(),
            });
        }
        if let Some(control) = execute_for_value(loop_node, Value::Number(current), context)? {
            return Ok(control);
        }
        // 已执行终点后直接结束，避免为不再使用的下一值触发浮点停滞检查。
        if current == end_number {
            return Ok(BodyControl::Continue);
        }
        let next: f64 = current + step_number;
        if next == current {
            return Err(LogicNodeError::Evaluation(EvalError::InvalidRange(
                step_span,
            )));
        }
        current = next;
    }
    Ok(BodyControl::Continue)
}

/// 把循环范围边界强制为有限数值；非数值或无穷大报告对应 Span 错误。
pub(crate) fn finite_range_number(
    value: Value,
    span: crate::expression::Span,
) -> Result<f64, EvalError> {
    match value {
        Value::Number(number) if number.is_finite() => Ok(number),
        Value::Number(_) => Err(EvalError::InvalidRange(span)),
        _ => Err(EvalError::InvalidNumericArgument(span)),
    }
}

/// 执行一轮，并把 break 或 Passage 停止转换为循环的返回结果。
fn execute_for_value<Story>(
    loop_node: &HirFor<'_>,
    value: Value,
    context: &mut MacroLogicContext<'_, Story>,
) -> Result<Option<BodyControl>, LogicNodeError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    assign_value_with_mut(&loop_node.target.value, value, context)
        .map_err(LogicNodeError::Evaluation)?;
    match execute_logic_body(&loop_node.body, context)? {
        BodyControl::Continue | BodyControl::ContinueLoop => Ok(None),
        BodyControl::BreakLoop => Ok(Some(BodyControl::Continue)),
        BodyControl::ExitScope => Ok(Some(BodyControl::ExitScope)),
        BodyControl::StopPassage => Ok(Some(BodyControl::StopPassage)),
    }
}

/// 按 `in`（键）或 `of`（值）语义把集合展开为稳定迭代快照。
pub(crate) fn collection_iteration_values(
    collection: Value,
    keys: bool,
    span: crate::expression::Span,
) -> Result<Vec<Value>, EvalError> {
    match collection {
        Value::Array(items) if keys => Ok((0..items.len())
            .map(|index: usize| Value::Number(index as f64))
            .collect()),
        Value::Array(items) => Ok(items.snapshot()),
        Value::Object(properties) if keys => Ok(properties
            .snapshot()
            .into_iter()
            .map(|(name, _value): (String, Value)| Value::string(name))
            .collect()),
        Value::Object(properties) => Ok(properties
            .snapshot()
            .into_iter()
            .map(|(_name, value): (String, Value)| value)
            .collect()),
        _ => Err(EvalError::InvalidCollectionTarget(span)),
    }
}

/// 每轮重新求值条件，并只在当前循环边界消费 break 与 continue。
fn execute_while<Story>(
    loop_node: &HirWhile<'_>,
    context: &mut MacroLogicContext<'_, Story>,
) -> Result<BodyControl, LogicNodeError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    loop {
        if !context.consume_execution_step() {
            return Err(LogicNodeError::ExecutionLimitExceeded {
                limit: context.execution_limit(),
            });
        }
        let condition: Value =
            evaluate_with_mut(&loop_node.condition, context).map_err(LogicNodeError::Evaluation)?;
        if !condition.is_truthy() {
            return Ok(BodyControl::Continue);
        }

        match execute_logic_body(&loop_node.body, context)? {
            BodyControl::Continue | BodyControl::ContinueLoop => continue,
            BodyControl::BreakLoop => return Ok(BodyControl::Continue),
            BodyControl::ExitScope => return Ok(BodyControl::ExitScope),
            BodyControl::StopPassage => return Ok(BodyControl::StopPassage),
        }
    }
}

/// 主值只求值一次，case 按严格相等选择首个匹配分支。
fn execute_switch<Story>(
    switch: &HirSwitch<'_>,
    context: &mut MacroLogicContext<'_, Story>,
) -> Result<BodyControl, LogicNodeError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    let selected: Value =
        evaluate_with_mut(&switch.value, context).map_err(LogicNodeError::Evaluation)?;
    for case in &switch.cases {
        let candidate: Value =
            evaluate_with_mut(&case.value, context).map_err(LogicNodeError::Evaluation)?;
        if values_strict_equal(&selected, &candidate) {
            return execute_logic_body(&case.body, context);
        }
    }

    match &switch.default {
        Some(body) => execute_logic_body(body, context),
        None => Ok(BodyControl::Continue),
    }
}

/// 按源码顺序选择第一个真值分支，未命中时执行可选 fallback。
fn execute_if<Story>(
    conditional: &HirIf<'_>,
    context: &mut MacroLogicContext<'_, Story>,
) -> Result<BodyControl, LogicNodeError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    for branch in &conditional.branches {
        let condition: Value =
            evaluate_with_mut(&branch.condition, context).map_err(LogicNodeError::Evaluation)?;
        if condition.is_truthy() {
            return execute_logic_body(&branch.body, context);
        }
    }

    match &conditional.fallback {
        Some(body) => execute_logic_body(body, context),
        None => Ok(BodyControl::Continue),
    }
}

/// 顺序执行只包含当前阶段逻辑节点的 HIR 正文。
pub fn execute_logic_body<Story>(
    body: &[HirBodyNode<'_>],
    context: &mut MacroLogicContext<'_, Story>,
) -> Result<BodyControl, LogicNodeError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    execute_hir_body(body, |node: &HirBodyNode<'_>| {
        execute_logic_node(node, context)
    })
}

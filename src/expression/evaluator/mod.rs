//! 无上下文 Expression 求值器。
//!
//! 错误类型与公开入口在本模块；求值会话与赋值降级在 `session`，
//! 链式读取、成员与调用解析在 `chain`。

mod chain;
mod session;

pub(crate) use session::evaluate_in;

mod conversion;
mod native_functions;
mod native_methods;
mod operations;
mod target;

use crate::diagnostic::{Diagnostic, DiagnosticSeverity};

use super::{
    AssignmentOperator, BinaryOperator, Expression, ExpressionKind, ObjectKey, Span,
    UpdateOperator, UpdatePosition, VariableScope,
    value::{
        NativeCallable, NativeCallableKind, NativeFunction, NativeMethod, NativeNamespace,
        ScriptCallable, TextValue, Value,
    },
};
use conversion::{decode_string, string_to_number, to_int32, to_number, to_string, to_uint32};
use native_functions::call_native_function;
use native_methods::call_native_method;
use operations::{
    evaluate_arithmetic_values, evaluate_between, evaluate_binary, evaluate_unary, strict_equal,
};
use session::{ContextAccess, EmptyContext, EvaluationSession};
use target::AssignmentPath;

/// 求值错误始终携带原 Expression 的源码位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvalError {
    ContextWriteRejected(Span),
    InvalidDeleteTarget(Span),
    InvalidArgumentCount(Span),
    InvalidArrayIndex(Span),
    InvalidCollectionTarget(Span),
    InvalidIndexTarget(Span),
    InvalidMembershipTarget(Span),
    InvalidNumber(Span),
    InvalidNumericArgument(Span),
    InvalidNumericConversion(Span),
    InvalidObjectTarget(Span),
    InvalidPrototype(Span),
    InvalidRandomValue(Span),
    InvalidRange(Span),
    InvalidStringConversion(Span),
    InvalidStringEscape(Span),
    MissingSetup(Span),
    MissingRandomSource(Span),
    MissingWriteContext(Span),
    NotCallable(Span),
    ScriptCallFailed(Span),
    ReservedGlobal(Span),
    UnorderedComparison(Span),
    UnknownGlobal(Span),
    UnknownMember(Span),
    UnsupportedExpression(Span),
}

impl EvalError {
    /// 返回错误在 Expression 片段内的 UTF-8 字节范围。
    pub fn span(self) -> Span {
        match self {
            Self::ContextWriteRejected(span)
            | Self::InvalidDeleteTarget(span)
            | Self::InvalidArgumentCount(span)
            | Self::InvalidArrayIndex(span)
            | Self::InvalidCollectionTarget(span)
            | Self::InvalidIndexTarget(span)
            | Self::InvalidMembershipTarget(span)
            | Self::InvalidNumber(span)
            | Self::InvalidNumericArgument(span)
            | Self::InvalidNumericConversion(span)
            | Self::InvalidObjectTarget(span)
            | Self::InvalidPrototype(span)
            | Self::InvalidRandomValue(span)
            | Self::InvalidRange(span)
            | Self::InvalidStringConversion(span)
            | Self::InvalidStringEscape(span)
            | Self::MissingSetup(span)
            | Self::MissingRandomSource(span)
            | Self::MissingWriteContext(span)
            | Self::NotCallable(span)
            | Self::ScriptCallFailed(span)
            | Self::ReservedGlobal(span)
            | Self::UnorderedComparison(span)
            | Self::UnknownGlobal(span)
            | Self::UnknownMember(span)
            | Self::UnsupportedExpression(span) => span,
        }
    }

    /// 转换为稳定 Diagnostic；实际 Source 位置由 Expression 嵌入方附加。
    pub fn diagnostic(self) -> Diagnostic {
        let (code, message): (&str, &str) = match self {
            Self::ContextWriteRejected(_) => (
                "expression.context_write_rejected",
                "Expression Context 拒绝写入",
            ),
            Self::InvalidDeleteTarget(_) => (
                "expression.invalid_delete_target",
                "目标不支持保持结构的删除操作",
            ),
            Self::InvalidArgumentCount(_) => (
                "expression.invalid_argument_count",
                "函数或方法的参数数量不正确",
            ),
            Self::InvalidArrayIndex(_) => (
                "expression.invalid_array_index",
                "Array 索引不是有效的稠密位置",
            ),
            Self::InvalidCollectionTarget(_) => {
                ("expression.invalid_collection_target", "值不是受支持的集合")
            }
            Self::InvalidIndexTarget(_) => ("expression.invalid_index_target", "值不支持索引访问"),
            Self::InvalidMembershipTarget(_) => {
                ("expression.invalid_membership_target", "值不支持成员判断")
            }
            Self::InvalidNumber(_) => ("expression.invalid_number", "Number 字面量无效"),
            Self::InvalidNumericArgument(_) => {
                ("expression.invalid_numeric_argument", "参数必须是 Number")
            }
            Self::InvalidNumericConversion(_) => (
                "expression.invalid_numeric_conversion",
                "值无法转换为 Number",
            ),
            Self::InvalidObjectTarget(_) => (
                "expression.invalid_object_target",
                "值不是有效的 Object 目标",
            ),
            Self::InvalidPrototype(_) => ("expression.invalid_prototype", "原型名称或目标无效"),
            Self::InvalidRandomValue(_) => {
                ("expression.invalid_random_value", "随机源返回了无效数值")
            }
            Self::InvalidRange(_) => ("expression.invalid_range", "范围边界无效"),
            Self::InvalidStringConversion(_) => (
                "expression.invalid_string_conversion",
                "值无法转换为 String",
            ),
            Self::InvalidStringEscape(_) => {
                ("expression.invalid_string_escape", "字符串包含无效转义")
            }
            Self::MissingSetup(_) => ("expression.missing_setup", "State.setup 尚未提供"),
            Self::MissingRandomSource(_) => (
                "expression.missing_random_source",
                "当前求值上下文没有随机源",
            ),
            Self::MissingWriteContext(_) => (
                "expression.missing_write_context",
                "当前求值上下文不允许写入",
            ),
            Self::NotCallable(_) => ("expression.not_callable", "值不可调用"),
            Self::ScriptCallFailed(_) => (
                "expression.script_call_failed",
                "Script Binding 调用函数失败",
            ),
            Self::ReservedGlobal(_) => ("expression.reserved_global", "保留的全局名称不可写入"),
            Self::UnorderedComparison(_) => (
                "expression.unordered_comparison",
                "参与比较的值没有可用顺序",
            ),
            Self::UnknownGlobal(_) => ("expression.unknown_global", "State.global 中不存在该名称"),
            Self::UnknownMember(_) => ("expression.unknown_member", "值不存在该成员"),
            Self::UnsupportedExpression(_) => (
                "expression.unsupported_expression",
                "当前求值器不支持此 Expression",
            ),
        };
        Diagnostic::new(code, DiagnosticSeverity::Error, message)
    }
}

/// Expression 只借用外部全局值，不拥有 State 或其生命周期。
pub trait EvaluationContext {
    /// 读取 State.global 中的全局值；不存在的名称返回 `None`。
    fn global(&self, name: &str) -> Option<&Value>;

    /// 读取 setup 提供的 State；未提供时返回 `None`。
    fn setup(&self) -> Option<&Value> {
        None
    }

    /// 读取指定作用域的变量绑定；未绑定返回 `None`。
    fn variable(&self, _scope: VariableScope, _name: &str) -> Option<&Value> {
        None
    }
}

/// 可写 Context 拒绝写入时返回的错误；目前只有统一的拒绝类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextWriteError {
    Rejected,
}

/// Script Binding 调用函数句柄时可稳定映射的错误类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptCallError {
    Unavailable,
    Failed,
}

/// 写入能力独立扩展只读查询接口，普通求值不需要提供它。
pub trait WritableEvaluationContext: EvaluationContext {
    /// 写入 State.global；被拒绝时返回错误。
    fn set_global(&mut self, name: &str, value: Value) -> Result<(), ContextWriteError>;

    /// 写入指定作用域的变量绑定；被拒绝时返回错误。
    fn set_variable(
        &mut self,
        scope: VariableScope,
        name: &str,
        value: Value,
    ) -> Result<(), ContextWriteError>;

    /// 写入 setup State；默认实现拒绝写入。
    fn set_setup(&mut self, _value: Value) -> Result<(), ContextWriteError> {
        Err(ContextWriteError::Rejected)
    }

    /// 删除 scripts 导入到 State.global 的名称；返回被删除的值。
    fn del_global(&mut self, _name: &str) -> Result<Option<Value>, ContextWriteError> {
        Err(ContextWriteError::Rejected)
    }

    /// 按 `$`、`_` 或 `@` 所属域删除根变量；返回被删除的值。
    fn del_variable(
        &mut self,
        _scope: VariableScope,
        _name: &str,
    ) -> Result<Option<Value>, ContextWriteError> {
        Err(ContextWriteError::Rejected)
    }

    /// 修改型原生函数在触碰共享引用前单独取得授权。
    fn authorize_reference_write(&mut self) -> Result<(), ContextWriteError> {
        Err(ContextWriteError::Rejected)
    }

    /// 把 ScriptCallable 交回拥有真实函数对象的 Binding。
    fn call_script(
        &mut self,
        _callable: &ScriptCallable,
        _arguments: Vec<Value>,
    ) -> Result<Value, ScriptCallError> {
        Err(ScriptCallError::Unavailable)
    }
}

/// Runtime 提供可重放的 `[0, 1)` 随机单位，Expression 不持有随机状态。
pub trait RandomSource {
    /// 返回下一个可重放的 `[0, 1)` 随机单位。
    fn next_unit(&mut self) -> f64;
}

/// 在空上下文中求值；仅用于确定不依赖任何外部状态的表达式。
pub fn evaluate(expression: &Expression<'_>) -> Result<Value, EvalError> {
    evaluate_with(expression, &EmptyContext)
}

/// 使用显式只读上下文求值；全局名解析先查内置函数与命名空间，再落入 Context。
pub fn evaluate_with(
    expression: &Expression<'_>,
    context: &dyn EvaluationContext,
) -> Result<Value, EvalError> {
    let mut session: EvaluationSession<'_> = EvaluationSession {
        context: ContextAccess::Read(context),
        random: None,
    };
    evaluate_in(expression, &mut session)
}

/// 按 Expression 使用的 Web 标量规则转换文本，不隐式转换集合或函数。
pub fn value_to_text(value: &Value) -> Option<TextValue> {
    to_string(value)
}

/// 比较两个已求值结果，使用 `===`／`is` 的严格相等规则。
pub fn values_strict_equal(left: &Value, right: &Value) -> bool {
    strict_equal(left, right)
}

/// 显式注入随机源；State 查询接口仍保持只读。
pub fn evaluate_with_random(
    expression: &Expression<'_>,
    context: &dyn EvaluationContext,
    random: &mut dyn RandomSource,
) -> Result<Value, EvalError> {
    let mut session: EvaluationSession<'_> = EvaluationSession {
        context: ContextAccess::Read(context),
        random: Some(random),
    };
    evaluate_in(expression, &mut session)
}

/// 使用显式可写 Context 求值赋值与更新表达式。
pub fn evaluate_with_mut(
    expression: &Expression<'_>,
    context: &mut dyn WritableEvaluationContext,
) -> Result<Value, EvalError> {
    let mut session: EvaluationSession<'_> = EvaluationSession {
        context: ContextAccess::Write(context),
        random: None,
    };
    evaluate_in(expression, &mut session)
}

/// 把 Runtime 已准备好的值写入一个可赋值目标，不重新构造 Assignment AST。
pub fn assign_value_with_mut(
    target: &Expression<'_>,
    value: Value,
    context: &mut dyn WritableEvaluationContext,
) -> Result<(), EvalError> {
    let mut session: EvaluationSession<'_> = EvaluationSession {
        context: ContextAccess::Write(context),
        random: None,
    };
    let path: AssignmentPath = AssignmentPath::resolve(target, &mut session)?;
    let mut root: Value = if path.members.is_empty() {
        Value::Undefined
    } else {
        path.read_root(&session)?
    };
    path.commit_value(&mut root, value, &mut session)
}

/// 删除可写目标，并返回实际被删除的值；不存在的绑定返回 `None`。
pub fn delete_with_mut(
    target: &Expression<'_>,
    context: &mut dyn WritableEvaluationContext,
) -> Result<Option<Value>, EvalError> {
    let mut session: EvaluationSession<'_> = EvaluationSession {
        context: ContextAccess::Write(context),
        random: None,
    };
    let path: AssignmentPath = AssignmentPath::resolve(target, &mut session)?;
    path.delete(&mut session)
}

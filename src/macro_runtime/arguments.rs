//! Runtime Macro 的参数契约、解析、求值准备与 Diagnostic 映射。

use crate::diagnostic::{
    Diagnostic, DiagnosticLocationError, DiagnosticLocator, DiagnosticSeverity,
};
use crate::expression::{Expression, ParseError, Span, evaluator::EvalError};

/// Macro Definition 决定 Runtime 如何解释 HIR 保留的原始参数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroArgumentKind {
    /// 完整保留参数原文，由自定义 Handler 自行解释。
    Raw,
    /// 顶层空白列表；每项可以是 Expression 或 Interaction Target。
    ArgumentList,
}

/// Argument List 中尚未求值的一项，顺序与调用源码一致。
#[derive(Debug, PartialEq, Eq)]
pub enum MacroArgument<'source> {
    /// Expression 的 Span 相对该参数片段，`offset` 用于映射回宏参数原文。
    Expression {
        expression: Expression<'source>,
        offset: usize,
    },
    /// `[[显示文本|目标]]` 保留为结构化参数，不伪装成 Expression。
    InteractionTarget {
        target: InteractionTarget<'source>,
        label_offset: usize,
        target_offset: usize,
    },
}

/// Argument List 在边界解析阶段产生的错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroArgumentListError {
    Expression {
        error: ParseError,
        offset: usize,
    },
    InteractionTarget {
        error: InteractionTargetError,
        offset: usize,
    },
}

/// Expression 参数求值失败时，保留它在宏参数原文中的起点。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroArgumentValueError<Error> {
    Expression { error: Error, offset: usize },
    InteractionParse { error: ParseError, offset: usize },
    InteractionEvaluation { error: Error, offset: usize },
    InvalidInteractionText { offset: usize },
    UnclosedInteraction { offset: usize },
}

/// Runtime Macro 参数错误的统一 Diagnostic 与片段内范围。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroArgumentIssue {
    pub diagnostic: Diagnostic,
    pub span: Span,
}

impl MacroArgumentIssue {
    /// 把宏参数片段内的位置映射回完整 Twee Source。
    pub fn locate(
        self,
        locator: &DiagnosticLocator<'_>,
        fragment_start: usize,
    ) -> Result<Diagnostic, DiagnosticLocationError> {
        let location = locator.locate(fragment_start, self.span.start, self.span.end)?;
        Ok(self.diagnostic.with_location(location))
    }
}

impl MacroArgumentListError {
    pub fn issue(self, source_len: usize) -> MacroArgumentIssue {
        match self {
            Self::Expression { error, offset } => MacroArgumentIssue {
                diagnostic: error.diagnostic(),
                span: shifted_optional_span(error.span(), offset, source_len),
            },
            Self::InteractionTarget { error, offset } => MacroArgumentIssue {
                diagnostic: error.diagnostic(),
                span: point_span(offset, source_len),
            },
        }
    }
}

impl MacroArgumentValueError<EvalError> {
    pub fn issue(self, source_len: usize) -> MacroArgumentIssue {
        match self {
            Self::Expression { error, offset } | Self::InteractionEvaluation { error, offset } => {
                MacroArgumentIssue {
                    diagnostic: error.diagnostic(),
                    span: shifted_span(error.span(), offset, source_len),
                }
            }
            Self::InteractionParse { error, offset } => MacroArgumentIssue {
                diagnostic: error.diagnostic(),
                span: shifted_optional_span(error.span(), offset, source_len),
            },
            Self::InvalidInteractionText { offset } => MacroArgumentIssue {
                diagnostic: Diagnostic::new(
                    "macro.invalid_interaction_text",
                    DiagnosticSeverity::Error,
                    "Interaction Target 动态内容无法转换为文本",
                ),
                span: point_span(offset, source_len),
            },
            Self::UnclosedInteraction { offset } => MacroArgumentIssue {
                diagnostic: Diagnostic::new(
                    "macro.unclosed_interaction_interpolation",
                    DiagnosticSeverity::Error,
                    "Interaction Target 插值缺少闭合花括号",
                ),
                span: point_span(offset, source_len),
            },
        }
    }
}

fn shifted_optional_span(span: Option<Span>, offset: usize, source_len: usize) -> Span {
    span.map_or_else(
        || point_span(offset, source_len),
        |span: Span| shifted_span(span, offset, source_len),
    )
}

fn shifted_span(span: Span, offset: usize, source_len: usize) -> Span {
    Span {
        start: offset.saturating_add(span.start).min(source_len),
        end: offset.saturating_add(span.end).min(source_len),
    }
}

fn point_span(offset: usize, source_len: usize) -> Span {
    let start: usize = offset.min(source_len);
    let end: usize = source_len.min(start.saturating_add(1));
    Span { start, end }
}

/// 尚未求值的共享交互目标参数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InteractionTarget<'source> {
    pub label: &'source str,
    pub target: &'source str,
}

/// `[[显示文本|目标]]` 参数结构错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionTargetError {
    InvalidWrapper,
    MissingSeparator,
    EmptyLabel,
    EmptyTarget,
}

impl InteractionTargetError {
    pub fn diagnostic(self) -> Diagnostic {
        let message: &str = match self {
            Self::InvalidWrapper => "交互目标参数必须使用 `[[显示文本|目标]]`",
            Self::MissingSeparator => "交互目标参数缺少 `|` 分隔符",
            Self::EmptyLabel => "交互目标的显示文本不能为空",
            Self::EmptyTarget => "交互目标不能为空",
        };
        Diagnostic::new(
            "macro.invalid_interaction_target",
            DiagnosticSeverity::Error,
            message,
        )
    }
}

mod parser;
pub use parser::*;

mod prepare;
pub use prepare::*;

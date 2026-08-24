//! Expression Parser，负责优先级、AST 结构和解析错误。
//!
//! 公开入口与错误类型在本模块；运算符优先级层在 [`binary`]，
//! 一元、主表达式与字面量解析在 [`primary`]。

mod binary;
mod primary;

use crate::diagnostic::{Diagnostic, DiagnosticSeverity};

use binary::{delimiter_depth_after, parse_assignment, parse_token_segment};

use super::{
    AssignmentOperator, BetweenBounds, BinaryOperator, Expression, ExpressionKind, LexError,
    ObjectKey, ObjectProperty, Span, Token, TokenKind, UnaryOperator, UpdateOperator,
    UpdatePosition, lex,
};

/// 最小 Parser 的错误；结构化解析将在后续逐项扩展。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    Lex(LexError),
    InvalidAssignmentTarget(Span),
    MixedNullishLogical(Span),
    ExpectedExpression,
    ExpectedColon(Span),
    ExpectedMemberName(Span),
    ExpectedOptionalPostfix(Span),
    UnclosedArray(Span),
    UnclosedCall(Span),
    UnclosedGroup(Span),
    UnclosedIndex(Span),
    UnclosedObject(Span),
    UnexpectedToken(Span),
}

impl ParseError {
    /// 返回错误在 Expression 片段内的位置；空表达式没有可指向的 Token。
    pub fn span(self) -> Option<Span> {
        match self {
            Self::Lex(error) => Some(error.span()),
            Self::ExpectedExpression => None,
            Self::InvalidAssignmentTarget(span)
            | Self::MixedNullishLogical(span)
            | Self::ExpectedColon(span)
            | Self::ExpectedMemberName(span)
            | Self::ExpectedOptionalPostfix(span)
            | Self::UnclosedArray(span)
            | Self::UnclosedCall(span)
            | Self::UnclosedGroup(span)
            | Self::UnclosedIndex(span)
            | Self::UnclosedObject(span)
            | Self::UnexpectedToken(span) => Some(span),
        }
    }

    /// 转换为稳定 Diagnostic；片段位置仍由 Expression 嵌入方映射。
    pub fn diagnostic(self) -> Diagnostic {
        let (code, message): (&str, &str) = match self {
            Self::Lex(error) => return error.diagnostic(),
            Self::InvalidAssignmentTarget(_) => (
                "expression.invalid_assignment_target",
                "Expression 不是可赋值目标",
            ),
            Self::MixedNullishLogical(_) => (
                "expression.mixed_nullish_logical",
                "空值合并与逻辑运算混用时必须使用括号",
            ),
            Self::ExpectedExpression => ("expression.expected_expression", "此处需要 Expression"),
            Self::ExpectedColon(_) => ("expression.expected_colon", "此处需要冒号"),
            Self::ExpectedMemberName(_) => ("expression.expected_member_name", "成员访问缺少名称"),
            Self::ExpectedOptionalPostfix(_) => (
                "expression.expected_optional_postfix",
                "可选链后需要成员、索引或调用",
            ),
            Self::UnclosedArray(_) => ("expression.unclosed_array", "数组缺少闭合方括号"),
            Self::UnclosedCall(_) => ("expression.unclosed_call", "调用缺少闭合圆括号"),
            Self::UnclosedGroup(_) => ("expression.unclosed_group", "分组缺少闭合圆括号"),
            Self::UnclosedIndex(_) => ("expression.unclosed_index", "索引缺少闭合方括号"),
            Self::UnclosedObject(_) => ("expression.unclosed_object", "对象缺少闭合花括号"),
            Self::UnexpectedToken(_) => {
                ("expression.unexpected_token", "Expression 包含意外的 Token")
            }
        };
        Diagnostic::new(code, DiagnosticSeverity::Error, message)
    }
}

/// 将基础值、变量引用或括号分组转换为 Expression AST。
pub fn parse<'source>(source: &'source str) -> Result<Expression<'source>, ParseError> {
    let tokens: Vec<Token<'source>> = lex(source).map_err(ParseError::Lex)?;
    let mut cursor: usize = 0;
    let expression: Expression<'source> = parse_assignment(&tokens, &mut cursor)?;

    if let Some(extra) = tokens.get(cursor) {
        return Err(ParseError::UnexpectedToken(extra.span));
    }

    Ok(expression)
}

/// 解析由顶层空白分隔的 Expression 列表，供 Macro 调用等边界复用。
pub fn parse_list<'source>(source: &'source str) -> Result<Vec<Expression<'source>>, ParseError> {
    let tokens: Vec<Token<'source>> = lex(source).map_err(ParseError::Lex)?;
    let mut expressions: Vec<Expression<'source>> = Vec::new();

    if tokens.is_empty() {
        return Ok(expressions);
    }

    let mut segment_start: usize = 0;
    let mut depth: usize = 0;
    for index in 1..tokens.len() {
        depth = delimiter_depth_after(depth, tokens[index - 1].kind);
        let gap: &str = &source[tokens[index - 1].span.end..tokens[index].span.start];
        if depth == 0 && gap.chars().any(char::is_whitespace) {
            expressions.push(parse_token_segment(&tokens[segment_start..index])?);
            segment_start = index;
        }
    }
    expressions.push(parse_token_segment(&tokens[segment_start..])?);

    Ok(expressions)
}

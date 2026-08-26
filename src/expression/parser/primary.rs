//! 一元、主表达式、调用与字面量解析。

use super::binary::parse_assignment;
use super::*;

/// 解析前缀自增自减、一元运算符；都不是时回落到主表达式解析。
pub(crate) fn parse_unary<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let Some(token) = tokens.get(*cursor).copied() else {
        return Err(ParseError::ExpectedExpression);
    };
    let update: Option<UpdateOperator> = match token.kind {
        TokenKind::PlusPlus => Some(UpdateOperator::Increment),
        TokenKind::MinusMinus => Some(UpdateOperator::Decrement),
        _ => None,
    };
    if let Some(operator) = update {
        *cursor += 1;
        let target: Expression<'source> = parse_unary(tokens, cursor)?;
        if !target.is_assignable_target() {
            return Err(ParseError::InvalidAssignmentTarget(target.span));
        }
        return Ok(Expression {
            span: Span {
                start: token.span.start,
                end: target.span.end,
            },
            kind: ExpressionKind::Update {
                operator,
                position: UpdatePosition::Prefix,
                target: Box::new(target),
            },
        });
    }

    let operator: Option<UnaryOperator> = match token.kind {
        TokenKind::Bang | TokenKind::Identifier("not") => Some(UnaryOperator::LogicalNot),
        TokenKind::Tilde => Some(UnaryOperator::BitwiseNot),
        TokenKind::Plus => Some(UnaryOperator::Positive),
        TokenKind::Minus => Some(UnaryOperator::Negative),
        TokenKind::Identifier("typeof") => Some(UnaryOperator::TypeOf),
        _ => None,
    };
    let Some(operator) = operator else {
        return parse_primary(tokens, cursor);
    };

    *cursor += 1;
    let operand: Expression<'source> = parse_unary(tokens, cursor)?;
    Ok(Expression {
        span: Span {
            start: token.span.start,
            end: operand.span.end,
        },
        kind: ExpressionKind::Unary {
            operator,
            operand: Box::new(operand),
        },
    })
}

/// 解析原子表达式，并持续应用成员、索引、调用与后缀更新。
fn parse_primary<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let mut expression: Expression<'source> = parse_atom(tokens, cursor)?;

    while let Some(postfix) = tokens.get(*cursor).copied() {
        match postfix.kind {
            TokenKind::Dot => {
                *cursor += 1;
                let property: Token<'source> = tokens
                    .get(*cursor)
                    .copied()
                    .ok_or(ParseError::ExpectedMemberName(postfix.span))?;
                let TokenKind::Identifier(name) = property.kind else {
                    return Err(ParseError::ExpectedMemberName(postfix.span));
                };
                *cursor += 1;

                let start: usize = expression.span.start;
                expression = Expression {
                    kind: ExpressionKind::Member {
                        target: Box::new(expression),
                        property: name,
                        property_span: property.span,
                    },
                    span: Span {
                        start,
                        end: property.span.end,
                    },
                };
            }
            TokenKind::LeftBracket => {
                *cursor += 1;
                let index: Expression<'source> = parse_assignment(tokens, cursor)?;
                let closing: Token<'source> = tokens
                    .get(*cursor)
                    .copied()
                    .ok_or(ParseError::UnclosedIndex(postfix.span))?;
                if closing.kind != TokenKind::RightBracket {
                    return Err(ParseError::UnexpectedToken(closing.span));
                }
                *cursor += 1;

                let start: usize = expression.span.start;
                expression = Expression {
                    kind: ExpressionKind::Index {
                        target: Box::new(expression),
                        index: Box::new(index),
                    },
                    span: Span {
                        start,
                        end: closing.span.end,
                    },
                };
            }
            TokenKind::LeftParen => {
                *cursor += 1;
                expression = parse_call(tokens, cursor, expression, postfix.span, false)?;
            }
            TokenKind::QuestionDot => {
                *cursor += 1;
                let optional: Token<'source> = tokens
                    .get(*cursor)
                    .copied()
                    .ok_or(ParseError::ExpectedOptionalPostfix(postfix.span))?;

                match optional.kind {
                    TokenKind::Identifier(property) => {
                        *cursor += 1;
                        let start: usize = expression.span.start;
                        expression = Expression {
                            kind: ExpressionKind::OptionalMember {
                                target: Box::new(expression),
                                property,
                                property_span: optional.span,
                            },
                            span: Span {
                                start,
                                end: optional.span.end,
                            },
                        };
                    }
                    TokenKind::LeftBracket => {
                        *cursor += 1;
                        let index: Expression<'source> = parse_assignment(tokens, cursor)?;
                        let closing: Token<'source> = tokens
                            .get(*cursor)
                            .copied()
                            .ok_or(ParseError::UnclosedIndex(optional.span))?;
                        if closing.kind != TokenKind::RightBracket {
                            return Err(ParseError::UnexpectedToken(closing.span));
                        }
                        *cursor += 1;

                        let start: usize = expression.span.start;
                        expression = Expression {
                            kind: ExpressionKind::OptionalIndex {
                                target: Box::new(expression),
                                index: Box::new(index),
                            },
                            span: Span {
                                start,
                                end: closing.span.end,
                            },
                        };
                    }
                    TokenKind::LeftParen => {
                        *cursor += 1;
                        expression = parse_call(tokens, cursor, expression, optional.span, true)?;
                    }
                    _ => return Err(ParseError::ExpectedOptionalPostfix(postfix.span)),
                }
            }
            TokenKind::PlusPlus | TokenKind::MinusMinus => {
                if !expression.is_assignable_target() {
                    return Err(ParseError::InvalidAssignmentTarget(expression.span));
                }
                *cursor += 1;
                let operator: UpdateOperator = match postfix.kind {
                    TokenKind::PlusPlus => UpdateOperator::Increment,
                    TokenKind::MinusMinus => UpdateOperator::Decrement,
                    _ => unreachable!("分支只接受更新运算符"),
                };
                let start: usize = expression.span.start;
                return Ok(Expression {
                    kind: ExpressionKind::Update {
                        operator,
                        position: UpdatePosition::Postfix,
                        target: Box::new(expression),
                    },
                    span: Span {
                        start,
                        end: postfix.span.end,
                    },
                });
            }
            _ => break,
        }
    }

    Ok(expression)
}

/// 解析实参列表；optional 决定生成普通还是可选调用节点。
fn parse_call<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
    callee: Expression<'source>,
    opening: Span,
    optional: bool,
) -> Result<Expression<'source>, ParseError> {
    let mut arguments: Vec<Expression<'source>> = Vec::new();

    if let Some(closing) = tokens.get(*cursor)
        && closing.kind == TokenKind::RightParen
    {
        *cursor += 1;
        let start: usize = callee.span.start;
        let kind: ExpressionKind<'source> = if optional {
            ExpressionKind::OptionalCall {
                callee: Box::new(callee),
                arguments,
            }
        } else {
            ExpressionKind::Call {
                callee: Box::new(callee),
                arguments,
            }
        };
        return Ok(Expression {
            span: Span {
                start,
                end: closing.span.end,
            },
            kind,
        });
    }

    loop {
        if tokens.get(*cursor).is_none() {
            return Err(ParseError::UnclosedCall(opening));
        }
        let argument: Expression<'source> = parse_assignment(tokens, cursor)?;
        arguments.push(argument);

        let separator: Token<'source> = tokens
            .get(*cursor)
            .copied()
            .ok_or(ParseError::UnclosedCall(opening))?;
        match separator.kind {
            TokenKind::Comma => *cursor += 1,
            TokenKind::RightParen => {
                *cursor += 1;
                let start: usize = callee.span.start;
                let kind: ExpressionKind<'source> = if optional {
                    ExpressionKind::OptionalCall {
                        callee: Box::new(callee),
                        arguments,
                    }
                } else {
                    ExpressionKind::Call {
                        callee: Box::new(callee),
                        arguments,
                    }
                };
                return Ok(Expression {
                    span: Span {
                        start,
                        end: separator.span.end,
                    },
                    kind,
                });
            }
            _ => return Err(ParseError::UnexpectedToken(separator.span)),
        }
    }
}

/// 解析基础字面量、变量与括号分组；复合字面量转交数组/对象解析。
fn parse_atom<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let token: Token<'source> = tokens
        .get(*cursor)
        .copied()
        .ok_or(ParseError::ExpectedExpression)?;
    *cursor += 1;

    let kind: ExpressionKind<'source> = match token.kind {
        TokenKind::Boolean(value) => ExpressionKind::Boolean(value),
        TokenKind::Identifier("setup") => ExpressionKind::Setup,
        TokenKind::Identifier(name) => ExpressionKind::Global(name),
        TokenKind::LeftBrace => return parse_object(tokens, cursor, token.span),
        TokenKind::LeftBracket => return parse_array(tokens, cursor, token.span),
        TokenKind::LeftParen => {
            let inner: Expression<'source> = parse_assignment(tokens, cursor)?;
            let closing: Token<'source> = tokens
                .get(*cursor)
                .copied()
                .ok_or(ParseError::UnclosedGroup(token.span))?;
            if closing.kind != TokenKind::RightParen {
                return Err(ParseError::UnexpectedToken(closing.span));
            }
            *cursor += 1;
            return Ok(Expression {
                kind: ExpressionKind::Group(Box::new(inner)),
                span: Span {
                    start: token.span.start,
                    end: closing.span.end,
                },
            });
        }
        TokenKind::Null => ExpressionKind::Null,
        TokenKind::Number(value) => ExpressionKind::Number(value),
        TokenKind::String(value) => ExpressionKind::String(value),
        TokenKind::Undefined => ExpressionKind::Undefined,
        TokenKind::Variable { scope, name } => ExpressionKind::Variable { scope, name },
        _ => return Err(ParseError::UnexpectedToken(token.span)),
    };

    Ok(Expression {
        kind,
        span: token.span,
    })
}

/// 解析数组字面量元素；空数组直接返回，元素之间必须用逗号分隔。
fn parse_array<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
    opening: Span,
) -> Result<Expression<'source>, ParseError> {
    let mut items: Vec<Expression<'source>> = Vec::new();

    if let Some(closing) = tokens.get(*cursor)
        && closing.kind == TokenKind::RightBracket
    {
        *cursor += 1;
        return Ok(Expression {
            kind: ExpressionKind::Array(items),
            span: Span {
                start: opening.start,
                end: closing.span.end,
            },
        });
    }

    loop {
        if tokens.get(*cursor).is_none() {
            return Err(ParseError::UnclosedArray(opening));
        }
        let item: Expression<'source> = parse_assignment(tokens, cursor)?;
        items.push(item);

        let separator: Token<'source> = tokens
            .get(*cursor)
            .copied()
            .ok_or(ParseError::UnclosedArray(opening))?;
        match separator.kind {
            TokenKind::Comma => *cursor += 1,
            TokenKind::RightBracket => {
                *cursor += 1;
                return Ok(Expression {
                    kind: ExpressionKind::Array(items),
                    span: Span {
                        start: opening.start,
                        end: separator.span.end,
                    },
                });
            }
            _ => return Err(ParseError::UnexpectedToken(separator.span)),
        }
    }
}

/// 解析对象字面量属性；键只接受裸标识符或字符串，值按赋值优先级解析。
fn parse_object<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
    opening: Span,
) -> Result<Expression<'source>, ParseError> {
    let mut properties: Vec<ObjectProperty<'source>> = Vec::new();

    if let Some(closing) = tokens.get(*cursor)
        && closing.kind == TokenKind::RightBrace
    {
        *cursor += 1;
        return Ok(Expression {
            kind: ExpressionKind::Object(properties),
            span: Span {
                start: opening.start,
                end: closing.span.end,
            },
        });
    }

    loop {
        let key_token: Token<'source> = tokens
            .get(*cursor)
            .copied()
            .ok_or(ParseError::UnclosedObject(opening))?;
        let key: ObjectKey<'source> = match key_token.kind {
            TokenKind::Identifier(value) => ObjectKey::Identifier(value),
            TokenKind::String(value) => ObjectKey::String(value),
            _ => return Err(ParseError::UnexpectedToken(key_token.span)),
        };
        *cursor += 1;

        let colon: Token<'source> = tokens
            .get(*cursor)
            .copied()
            .ok_or(ParseError::ExpectedColon(key_token.span))?;
        if colon.kind != TokenKind::Colon {
            return Err(ParseError::ExpectedColon(key_token.span));
        }
        *cursor += 1;

        if tokens.get(*cursor).is_none() {
            return Err(ParseError::UnclosedObject(opening));
        }
        let value: Expression<'source> = parse_assignment(tokens, cursor)?;
        properties.push(ObjectProperty {
            key,
            key_span: key_token.span,
            value,
        });

        let separator: Token<'source> = tokens
            .get(*cursor)
            .copied()
            .ok_or(ParseError::UnclosedObject(opening))?;
        match separator.kind {
            TokenKind::Comma => *cursor += 1,
            TokenKind::RightBrace => {
                *cursor += 1;
                return Ok(Expression {
                    kind: ExpressionKind::Object(properties),
                    span: Span {
                        start: opening.start,
                        end: separator.span.end,
                    },
                });
            }
            _ => return Err(ParseError::UnexpectedToken(separator.span)),
        }
    }
}

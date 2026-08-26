//! 二元与赋值运算符优先级层。

use super::primary::parse_unary;
use super::*;

/// 解析一段完整 Token 序列；段内残留 Token 一律视为错误，供 parse_list 复用。
pub(crate) fn parse_token_segment<'source>(
    tokens: &[Token<'source>],
) -> Result<Expression<'source>, ParseError> {
    let mut cursor: usize = 0;
    let expression: Expression<'source> = parse_assignment(tokens, &mut cursor)?;
    if let Some(extra) = tokens.get(cursor) {
        return Err(ParseError::UnexpectedToken(extra.span));
    }
    Ok(expression)
}

/// 按单个 Token 更新括号深度，供 parse_list 判断顶层切分点。
pub(crate) fn delimiter_depth_after(depth: usize, token: TokenKind<'_>) -> usize {
    match token {
        TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => depth + 1,
        TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
            depth.saturating_sub(1)
        }
        _ => depth,
    }
}

/// 赋值是右结合的最低优先级表达式，并在生成 AST 前验证写入目标。
pub(crate) fn parse_assignment<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let target: Expression<'source> = parse_conditional(tokens, cursor)?;
    let Some(operator) = tokens
        .get(*cursor)
        .and_then(|token| assignment_operator(token.kind))
    else {
        return Ok(target);
    };
    if !target.is_assignable_target() {
        return Err(ParseError::InvalidAssignmentTarget(target.span));
    }

    *cursor += 1;
    let value: Expression<'source> = parse_assignment(tokens, cursor)?;
    let start: usize = target.span.start;
    let end: usize = value.span.end;
    Ok(Expression {
        span: Span { start, end },
        kind: ExpressionKind::Assignment {
            operator,
            target: Box::new(target),
            value: Box::new(value),
        },
    })
}

/// Lexer Token 在这里归一化为 VM 最终需要的赋值种类。
fn assignment_operator(token: TokenKind<'_>) -> Option<AssignmentOperator> {
    match token {
        TokenKind::Assign => Some(AssignmentOperator::Assign),
        TokenKind::AddAssign => Some(AssignmentOperator::Add),
        TokenKind::SubtractAssign => Some(AssignmentOperator::Subtract),
        TokenKind::MultiplyAssign => Some(AssignmentOperator::Multiply),
        TokenKind::DivideAssign => Some(AssignmentOperator::Divide),
        TokenKind::IntegerDivideAssign => Some(AssignmentOperator::IntegerDivide),
        TokenKind::RemainderAssign => Some(AssignmentOperator::Remainder),
        TokenKind::PowerAssign => Some(AssignmentOperator::Power),
        TokenKind::ShiftLeftAssign => Some(AssignmentOperator::ShiftLeft),
        TokenKind::ShiftRightAssign => Some(AssignmentOperator::ShiftRight),
        TokenKind::UnsignedShiftRightAssign => Some(AssignmentOperator::UnsignedShiftRight),
        TokenKind::BitwiseAndAssign => Some(AssignmentOperator::BitwiseAnd),
        TokenKind::BitwiseXorAssign => Some(AssignmentOperator::BitwiseXor),
        TokenKind::BitwiseOrAssign => Some(AssignmentOperator::BitwiseOr),
        // 短路赋值保留独立种类，求值器据此决定是否读取右值。
        TokenKind::LogicalAndAssign => Some(AssignmentOperator::LogicalAnd),
        TokenKind::LogicalOrAssign => Some(AssignmentOperator::LogicalOr),
        TokenKind::NullishAssign => Some(AssignmentOperator::NullishCoalesce),
        _ => None,
    }
}

/// 三目条件低于空值合并；递归解析两个分支以保留右结合结构。
fn parse_conditional<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let condition: Expression<'source> = parse_nullish(tokens, cursor)?;
    let Some(question) = tokens.get(*cursor).copied() else {
        return Ok(condition);
    };
    if question.kind != TokenKind::Question {
        return Ok(condition);
    }

    *cursor += 1;
    let consequent: Expression<'source> = parse_assignment(tokens, cursor)?;
    let Some(colon) = tokens.get(*cursor).copied() else {
        return Err(ParseError::ExpectedColon(question.span));
    };
    if colon.kind != TokenKind::Colon {
        return Err(ParseError::ExpectedColon(question.span));
    }

    *cursor += 1;
    let alternate: Expression<'source> = parse_assignment(tokens, cursor)?;
    let start: usize = condition.span.start;
    let end: usize = alternate.span.end;
    Ok(Expression {
        span: Span { start, end },
        kind: ExpressionKind::Conditional {
            condition: Box::new(condition),
            consequent: Box::new(consequent),
            alternate: Box::new(alternate),
        },
    })
}

/// 空值合并低于逻辑运算，但要求二者混用时用括号明确意图。
fn parse_nullish<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let mut expression: Expression<'source> = parse_logical_or(tokens, cursor)?;

    while let Some(operator) = tokens.get(*cursor).copied() {
        if operator.kind != TokenKind::QuestionQuestion {
            break;
        }
        *cursor += 1;
        let right: Expression<'source> = parse_logical_or(tokens, cursor)?;

        // Group 节点会阻断此检查，因此括号内的逻辑表达式仍可合法组合。
        if is_unparenthesized_logical(&expression) || is_unparenthesized_logical(&right) {
            return Err(ParseError::MixedNullishLogical(operator.span));
        }
        expression = make_binary(expression, BinaryOperator::NullishCoalesce, right);
    }

    Ok(expression)
}

fn is_unparenthesized_logical(expression: &Expression<'_>) -> bool {
    matches!(
        expression.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr,
            ..
        }
    )
}

/// 逻辑或是当前已实现的最低优先级层；求值器随后负责短路。
fn parse_logical_or<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let mut expression: Expression<'source> = parse_logical_and(tokens, cursor)?;

    while matches!(
        tokens.get(*cursor).map(|token| token.kind),
        Some(TokenKind::PipePipe | TokenKind::Identifier("or"))
    ) {
        *cursor += 1;
        let right: Expression<'source> = parse_logical_and(tokens, cursor)?;
        expression = make_binary(expression, BinaryOperator::LogicalOr, right);
    }

    Ok(expression)
}

/// 逻辑与高于逻辑或；符号形式和英文形式生成相同 AST。
fn parse_logical_and<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let mut expression: Expression<'source> = parse_bitwise_or(tokens, cursor)?;

    while matches!(
        tokens.get(*cursor).map(|token| token.kind),
        Some(TokenKind::AmpersandAmpersand | TokenKind::Identifier("and"))
    ) {
        *cursor += 1;
        let right: Expression<'source> = parse_bitwise_or(tokens, cursor)?;
        expression = make_binary(expression, BinaryOperator::LogicalAnd, right);
    }

    Ok(expression)
}

/// 按位运算拆成三层，保持 `&` 高于 `^`、`^` 高于 `|`。
/// 按位或层：`|` 左结合，优先级低于按位异或。
fn parse_bitwise_or<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    parse_binary_level(
        tokens,
        cursor,
        parse_bitwise_xor,
        TokenKind::Pipe,
        BinaryOperator::BitwiseOr,
    )
}

/// 按位异或层：`^` 左结合，优先级高于按位或、低于按位与。
fn parse_bitwise_xor<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    parse_binary_level(
        tokens,
        cursor,
        parse_bitwise_and,
        TokenKind::Caret,
        BinaryOperator::BitwiseXor,
    )
}

/// 按位与层：`&` 左结合，优先级高于异或、低于相等层。
fn parse_bitwise_and<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    parse_binary_level(
        tokens,
        cursor,
        parse_equality,
        TokenKind::Ampersand,
        BinaryOperator::BitwiseAnd,
    )
}

/// 解析仅含一种 Token 的左结合二元层，供三个按位层复用。
fn parse_binary_level<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
    parse_operand: fn(&[Token<'source>], &mut usize) -> Result<Expression<'source>, ParseError>,
    token_kind: TokenKind<'source>,
    operator: BinaryOperator,
) -> Result<Expression<'source>, ParseError> {
    let mut expression: Expression<'source> = parse_operand(tokens, cursor)?;

    while tokens.get(*cursor).map(|token| token.kind) == Some(token_kind) {
        *cursor += 1;
        let right: Expression<'source> = parse_operand(tokens, cursor)?;
        expression = make_binary(expression, operator, right);
    }

    Ok(expression)
}

/// 建立覆盖左右操作数完整范围的二元节点。
fn make_binary<'source>(
    left: Expression<'source>,
    operator: BinaryOperator,
    right: Expression<'source>,
) -> Expression<'source> {
    let start: usize = left.span.start;
    let end: usize = right.span.end;
    Expression {
        span: Span { start, end },
        kind: ExpressionKind::Binary {
            operator,
            left: Box::new(left),
            right: Box::new(right),
        },
    }
}

/// 相等层把英文别名归一化，但不在 Parser 中决定类型转换规则。
fn parse_equality<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let mut expression: Expression<'source> = parse_comparison(tokens, cursor)?;

    while let Some(token) = tokens.get(*cursor).copied() {
        let operator: BinaryOperator = match token.kind {
            TokenKind::EqualEqual | TokenKind::Identifier("equ") => BinaryOperator::Equal,
            TokenKind::NotEqual => BinaryOperator::NotEqual,
            TokenKind::StrictEqual | TokenKind::Identifier("is") => BinaryOperator::StrictEqual,
            TokenKind::StrictNotEqual | TokenKind::Identifier("isnot") => {
                BinaryOperator::StrictNotEqual
            }
            _ => break,
        };
        *cursor += 1;
        let right: Expression<'source> = parse_comparison(tokens, cursor)?;
        expression = make_binary(expression, operator, right);
    }

    Ok(expression)
}

/// 比较层包含普通比较、成员判断、三向比较和区间判断。
fn parse_comparison<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let mut expression: Expression<'source> = parse_shift(tokens, cursor)?;

    while let Some(token) = tokens.get(*cursor).copied() {
        if token.kind == TokenKind::Identifier("between") {
            let Some(bounds) = parse_between_bounds(tokens, *cursor + 1) else {
                break;
            };
            *cursor += 3;
            let lower: Expression<'source> = parse_shift(tokens, cursor)?;
            let upper: Expression<'source> = parse_shift(tokens, cursor)?;
            let start: usize = expression.span.start;
            expression = Expression {
                span: Span {
                    start,
                    end: upper.span.end,
                },
                kind: ExpressionKind::Between {
                    bounds,
                    value: Box::new(expression),
                    lower: Box::new(lower),
                    upper: Box::new(upper),
                },
            };
            continue;
        }

        let operator: BinaryOperator = match token.kind {
            TokenKind::Less | TokenKind::Identifier("lt") => BinaryOperator::Less,
            TokenKind::LessEqual | TokenKind::Identifier("lte") => BinaryOperator::LessEqual,
            TokenKind::Greater | TokenKind::Identifier("gt") => BinaryOperator::Greater,
            TokenKind::GreaterEqual | TokenKind::Identifier("gte") => BinaryOperator::GreaterEqual,
            TokenKind::Identifier("in") => BinaryOperator::In,
            TokenKind::Identifier("instanceof") => BinaryOperator::InstanceOf,
            TokenKind::Identifier("notin") => BinaryOperator::NotIn,
            TokenKind::ThreeWayCompare => BinaryOperator::ThreeWayCompare,
            _ => break,
        };
        *cursor += 1;
        let right: Expression<'source> = parse_shift(tokens, cursor)?;
        expression = make_binary(expression, operator, right);
    }

    Ok(expression)
}

/// 读取 between 后紧跟的开闭括号组合；不匹配时返回 None，按普通比较继续。
fn parse_between_bounds(tokens: &[Token<'_>], cursor: usize) -> Option<BetweenBounds> {
    let left: TokenKind<'_> = tokens.get(cursor)?.kind;
    let right: TokenKind<'_> = tokens.get(cursor + 1)?.kind;

    match (left, right) {
        (TokenKind::LeftParen, TokenKind::RightParen) => Some(BetweenBounds::OpenOpen),
        (TokenKind::LeftParen, TokenKind::RightBracket) => Some(BetweenBounds::OpenClosed),
        (TokenKind::LeftBracket, TokenKind::RightParen) => Some(BetweenBounds::ClosedOpen),
        (TokenKind::LeftBracket, TokenKind::RightBracket) => Some(BetweenBounds::ClosedClosed),
        _ => None,
    }
}

/// 移位层：左移、右移与无符号右移均为左结合。
fn parse_shift<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let mut expression: Expression<'source> = parse_additive(tokens, cursor)?;

    while let Some(token) = tokens.get(*cursor).copied() {
        let operator: BinaryOperator = match token.kind {
            TokenKind::ShiftLeft => BinaryOperator::ShiftLeft,
            TokenKind::ShiftRight => BinaryOperator::ShiftRight,
            TokenKind::UnsignedShiftRight => BinaryOperator::UnsignedShiftRight,
            _ => break,
        };
        *cursor += 1;
        let right: Expression<'source> = parse_additive(tokens, cursor)?;
        expression = make_binary(expression, operator, right);
    }

    Ok(expression)
}

/// 加减层：`+` 与 `-` 左结合。
fn parse_additive<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let mut expression: Expression<'source> = parse_multiplicative(tokens, cursor)?;

    while let Some(token) = tokens.get(*cursor).copied() {
        let operator: BinaryOperator = match token.kind {
            TokenKind::Plus => BinaryOperator::Add,
            TokenKind::Minus => BinaryOperator::Subtract,
            _ => break,
        };
        *cursor += 1;
        let right: Expression<'source> = parse_multiplicative(tokens, cursor)?;
        expression = make_binary(expression, operator, right);
    }

    Ok(expression)
}

/// 乘除层：`*`、`/`、`//`、`%` 左结合，优先级低于幂运算。
fn parse_multiplicative<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let mut expression: Expression<'source> = parse_power(tokens, cursor)?;

    while let Some(token) = tokens.get(*cursor).copied() {
        let operator: BinaryOperator = match token.kind {
            TokenKind::Star => BinaryOperator::Multiply,
            TokenKind::Slash => BinaryOperator::Divide,
            TokenKind::SlashSlash => BinaryOperator::IntegerDivide,
            TokenKind::Percent => BinaryOperator::Remainder,
            _ => break,
        };
        *cursor += 1;
        let right: Expression<'source> = parse_power(tokens, cursor)?;
        expression = make_binary(expression, operator, right);
    }

    Ok(expression)
}

/// 幂运算递归解析右侧，从而保持右结合。
fn parse_power<'source>(
    tokens: &[Token<'source>],
    cursor: &mut usize,
) -> Result<Expression<'source>, ParseError> {
    let left: Expression<'source> = parse_unary(tokens, cursor)?;
    let Some(operator) = tokens.get(*cursor).copied() else {
        return Ok(left);
    };
    if operator.kind != TokenKind::StarStar {
        return Ok(left);
    }

    *cursor += 1;
    let right: Expression<'source> = parse_power(tokens, cursor)?;
    Ok(Expression {
        span: Span {
            start: left.span.start,
            end: right.span.end,
        },
        kind: ExpressionKind::Binary {
            operator: BinaryOperator::Power,
            left: Box::new(left),
            right: Box::new(right),
        },
    })
}

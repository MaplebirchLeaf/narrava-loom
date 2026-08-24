//! Expression 词法分析，只负责把源码切分为带位置的 Token。

use crate::diagnostic::{Diagnostic, DiagnosticSeverity};

use super::{Span, VariableScope};

/// Expression Lexer 当前支持的 Token。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind<'source> {
    Ampersand,
    AmpersandAmpersand,
    LogicalAndAssign,
    BitwiseAndAssign,
    Assign,
    AddAssign,
    Bang,
    Boolean(bool),
    Colon,
    Comma,
    Caret,
    BitwiseXorAssign,
    Dot,
    EqualEqual,
    Greater,
    GreaterEqual,
    Identifier(&'source str),
    LeftBrace,
    LeftBracket,
    LeftParen,
    Less,
    LessEqual,
    NotEqual,
    Null,
    Number(&'source str),
    Minus,
    MinusMinus,
    SubtractAssign,
    Percent,
    RemainderAssign,
    Pipe,
    PipePipe,
    LogicalOrAssign,
    BitwiseOrAssign,
    Plus,
    PlusPlus,
    Question,
    QuestionQuestion,
    NullishAssign,
    QuestionDot,
    RightBrace,
    RightBracket,
    RightParen,
    ShiftLeft,
    ShiftLeftAssign,
    ShiftRight,
    ShiftRightAssign,
    Slash,
    SlashSlash,
    DivideAssign,
    IntegerDivideAssign,
    Star,
    StarStar,
    MultiplyAssign,
    PowerAssign,
    String(&'source str),
    StrictEqual,
    StrictNotEqual,
    Tilde,
    ThreeWayCompare,
    Undefined,
    UnsignedShiftRight,
    UnsignedShiftRightAssign,
    Variable {
        scope: VariableScope,
        name: &'source str,
    },
}

/// 一个带原始位置的 Expression Token。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token<'source> {
    pub kind: TokenKind<'source>,
    pub span: Span,
}

/// Expression 词法错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexError {
    InvalidVariable(Span),
    UnexpectedCharacter(Span),
    UnclosedString(Span),
}

impl LexError {
    /// 返回错误在 Expression 片段内的 UTF-8 字节范围。
    pub fn span(self) -> Span {
        match self {
            Self::InvalidVariable(span)
            | Self::UnexpectedCharacter(span)
            | Self::UnclosedString(span) => span,
        }
    }

    /// 转换为稳定 Diagnostic；文件位置由 Expression 嵌入方附加。
    pub fn diagnostic(self) -> Diagnostic {
        let (code, message): (&str, &str) = match self {
            Self::InvalidVariable(_) => ("expression.invalid_variable", "Expression 变量缺少名称"),
            Self::UnexpectedCharacter(_) => (
                "expression.unexpected_character",
                "Expression 包含无法识别的字符",
            ),
            Self::UnclosedString(_) => (
                "expression.unclosed_string",
                "Expression 字符串缺少闭合引号",
            ),
        };
        Diagnostic::new(code, DiagnosticSeverity::Error, message)
    }
}

/// 识别当前已支持的 Expression Token。
pub fn lex(source: &str) -> Result<Vec<Token<'_>>, LexError> {
    let bytes: &[u8] = source.as_bytes();
    let mut tokens: Vec<Token<'_>> = Vec::new();
    let mut index: usize = 0;

    while index < bytes.len() {
        // 空白是唯一允许直接跳过的字符，其他未知字符必须报告位置。
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }

        if matches!(bytes[index], b'\'' | b'"') {
            let quote: u8 = bytes[index];
            let content_start: usize = index + 1;
            let end: usize =
                scan_string(bytes, content_start, quote).ok_or(LexError::UnclosedString(Span {
                    start: index,
                    end: bytes.len(),
                }))?;
            tokens.push(Token {
                kind: TokenKind::String(&source[content_start..end]),
                span: Span {
                    start: index,
                    end: end + 1,
                },
            });
            index = end + 1;
            continue;
        }

        if bytes[index].is_ascii_digit() {
            let end: usize = scan_number(bytes, index);
            tokens.push(Token {
                kind: TokenKind::Number(&source[index..end]),
                span: Span { start: index, end },
            });
            index = end;
            continue;
        }

        if bytes[index].is_ascii_alphabetic() {
            let end: usize = scan_identifier(bytes, index);
            let text: &str = &source[index..end];
            let kind: TokenKind<'_> = match text {
                "true" => TokenKind::Boolean(true),
                "false" => TokenKind::Boolean(false),
                "null" => TokenKind::Null,
                "undefined" => TokenKind::Undefined,
                _ => TokenKind::Identifier(text),
            };
            tokens.push(Token {
                kind,
                span: Span { start: index, end },
            });
            index = end;
            continue;
        }

        // 必须先检查四字符形式，避免把 `>>>=` 拆成 `>>>` 与 `=`。
        if bytes.get(index..index + 4) == Some(b">>>=") {
            tokens.push(Token {
                kind: TokenKind::UnsignedShiftRightAssign,
                span: Span {
                    start: index,
                    end: index + 4,
                },
            });
            index += 4;
            continue;
        }

        let tripled: Option<TokenKind<'_>> = match bytes.get(index..index + 3) {
            Some(b">>>") => Some(TokenKind::UnsignedShiftRight),
            Some(b"<=>") => Some(TokenKind::ThreeWayCompare),
            Some(b"===") => Some(TokenKind::StrictEqual),
            Some(b"!==") => Some(TokenKind::StrictNotEqual),
            Some(b"//=") => Some(TokenKind::IntegerDivideAssign),
            Some(b"**=") => Some(TokenKind::PowerAssign),
            Some(b"<<=") => Some(TokenKind::ShiftLeftAssign),
            Some(b">>=") => Some(TokenKind::ShiftRightAssign),
            Some(b"&&=") => Some(TokenKind::LogicalAndAssign),
            Some(b"||=") => Some(TokenKind::LogicalOrAssign),
            Some(b"??=") => Some(TokenKind::NullishAssign),
            _ => None,
        };
        if let Some(kind) = tripled {
            tokens.push(Token {
                kind,
                span: Span {
                    start: index,
                    end: index + 3,
                },
            });
            index += 3;
            continue;
        }

        let doubled: Option<TokenKind<'_>> = match (bytes[index], bytes.get(index + 1)) {
            (b'+', Some(b'+')) => Some(TokenKind::PlusPlus),
            (b'-', Some(b'-')) => Some(TokenKind::MinusMinus),
            (b'/', Some(b'/')) => Some(TokenKind::SlashSlash),
            (b'*', Some(b'*')) => Some(TokenKind::StarStar),
            (b'<', Some(b'<')) => Some(TokenKind::ShiftLeft),
            (b'>', Some(b'>')) => Some(TokenKind::ShiftRight),
            (b'<', Some(b'=')) => Some(TokenKind::LessEqual),
            (b'>', Some(b'=')) => Some(TokenKind::GreaterEqual),
            (b'=', Some(b'=')) => Some(TokenKind::EqualEqual),
            (b'!', Some(b'=')) => Some(TokenKind::NotEqual),
            (b'&', Some(b'&')) => Some(TokenKind::AmpersandAmpersand),
            (b'|', Some(b'|')) => Some(TokenKind::PipePipe),
            (b'?', Some(b'?')) => Some(TokenKind::QuestionQuestion),
            (b'+', Some(b'=')) => Some(TokenKind::AddAssign),
            (b'-', Some(b'=')) => Some(TokenKind::SubtractAssign),
            (b'*', Some(b'=')) => Some(TokenKind::MultiplyAssign),
            (b'/', Some(b'=')) => Some(TokenKind::DivideAssign),
            (b'%', Some(b'=')) => Some(TokenKind::RemainderAssign),
            (b'&', Some(b'=')) => Some(TokenKind::BitwiseAndAssign),
            (b'^', Some(b'=')) => Some(TokenKind::BitwiseXorAssign),
            (b'|', Some(b'=')) => Some(TokenKind::BitwiseOrAssign),
            _ => None,
        };
        if let Some(kind) = doubled {
            tokens.push(Token {
                kind,
                span: Span {
                    start: index,
                    end: index + 2,
                },
            });
            index += 2;
            continue;
        }

        if bytes[index] == b'?' && bytes.get(index + 1) == Some(&b'.') {
            tokens.push(Token {
                kind: TokenKind::QuestionDot,
                span: Span {
                    start: index,
                    end: index + 2,
                },
            });
            index += 2;
            continue;
        }

        let punctuation: Option<TokenKind<'_>> = punctuation_kind(bytes[index]);
        if let Some(kind) = punctuation {
            tokens.push(Token {
                kind,
                span: Span {
                    start: index,
                    end: index + 1,
                },
            });
            index += 1;
            continue;
        }

        let scope: Option<VariableScope> = match bytes[index] {
            b'$' => Some(VariableScope::Variables),
            b'_' => Some(VariableScope::Temporary),
            b'@' => Some(VariableScope::Local),
            _ => None,
        };
        let Some(scope) = scope else {
            let character: char = source[index..]
                .chars()
                .next()
                .expect("索引位于有效 UTF-8 内容中");
            return Err(LexError::UnexpectedCharacter(Span {
                start: index,
                end: index + character.len_utf8(),
            }));
        };

        let name_start: usize = index + 1;
        if name_start >= bytes.len() || !is_identifier_start(bytes[name_start]) {
            return Err(LexError::InvalidVariable(Span {
                start: index,
                end: name_start,
            }));
        }

        let mut end: usize = name_start + 1;
        while end < bytes.len() && is_identifier_continue(bytes[end]) {
            end += 1;
        }

        tokens.push(Token {
            kind: TokenKind::Variable {
                scope,
                name: &source[name_start..end],
            },
            span: Span { start: index, end },
        });
        index = end;
    }

    Ok(tokens)
}

/// 反斜杠保护其后的字符，具体转义值由 Evaluator 解码。
fn scan_string(bytes: &[u8], start: usize, quote: u8) -> Option<usize> {
    let mut end: usize = start;
    while end < bytes.len() {
        if bytes[end] == b'\\' {
            end += 2;
            continue;
        }
        if bytes[end] == quote {
            return Some(end);
        }
        end += 1;
    }

    None
}

/// 小数点后必须有数字，避免吞掉成员访问符。
fn scan_number(bytes: &[u8], start: usize) -> usize {
    let mut end: usize = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }

    let fraction_start: usize = end + 1;
    if end < bytes.len()
        && bytes[end] == b'.'
        && fraction_start < bytes.len()
        && bytes[fraction_start].is_ascii_digit()
    {
        end = fraction_start + 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
    }

    end
}

fn scan_identifier(bytes: &[u8], start: usize) -> usize {
    let mut end: usize = start + 1;
    while end < bytes.len() && is_identifier_continue(bytes[end]) {
        end += 1;
    }
    end
}

fn punctuation_kind(byte: u8) -> Option<TokenKind<'static>> {
    match byte {
        b'&' => Some(TokenKind::Ampersand),
        b'=' => Some(TokenKind::Assign),
        b'!' => Some(TokenKind::Bang),
        b'(' => Some(TokenKind::LeftParen),
        b')' => Some(TokenKind::RightParen),
        b'[' => Some(TokenKind::LeftBracket),
        b']' => Some(TokenKind::RightBracket),
        b'{' => Some(TokenKind::LeftBrace),
        b'}' => Some(TokenKind::RightBrace),
        b',' => Some(TokenKind::Comma),
        b':' => Some(TokenKind::Colon),
        b'^' => Some(TokenKind::Caret),
        b'.' => Some(TokenKind::Dot),
        b'+' => Some(TokenKind::Plus),
        b'-' => Some(TokenKind::Minus),
        b'%' => Some(TokenKind::Percent),
        b'|' => Some(TokenKind::Pipe),
        b'?' => Some(TokenKind::Question),
        b'/' => Some(TokenKind::Slash),
        b'*' => Some(TokenKind::Star),
        b'<' => Some(TokenKind::Less),
        b'>' => Some(TokenKind::Greater),
        b'~' => Some(TokenKind::Tilde),
        _ => None,
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

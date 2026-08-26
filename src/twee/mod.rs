//! Twee 词法、解析与 Story 语义检查。
//!
//! 类型定义在本模块；词法在 `lexer`、解析在 `parser`、Story 聚合与
//! 语义检查在 `story`。

mod lexer;
mod parser;
mod story;

pub use lexer::lex;
pub use parser::{parse, parse_fragment};
pub use story::validate;

use std::{collections::HashSet, error::Error, fmt};

use crate::diagnostic::{Diagnostic, DiagnosticLocation, DiagnosticSeverity};
use crate::source::{Source, SourceKind, SourcePath};

/// 一段内容在源码中的位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

/// Lexer 产生的单个词法单元。
#[derive(Debug, PartialEq, Eq)]
pub struct Token<'source> {
    pub source: &'source SourcePath,
    pub content: &'source str,
    pub kind: TokenKind<'source>,
    pub span: Span,
}

/// 当前只区分 Passage 声明与尚未解析的正文。
#[derive(Debug, PartialEq, Eq)]
pub enum TokenKind<'source> {
    /// `::` 开头的 Passage 声明行。
    PassageDeclaration {
        name: &'source str,
        tags: Vec<&'source str>,
    },
    /// 尚未解析的正文行。
    Text(&'source str),
}

/// Passage 正文中的最小 AST 节点。
#[derive(Debug, PartialEq, Eq)]
pub struct BodyNode<'source> {
    pub kind: BodyNodeKind<'source>,
    pub span: Span,
}

/// Passage 正文支持字面文本与通用 Macro。
#[derive(Debug, PartialEq, Eq)]
pub enum BodyNodeKind<'source> {
    Text(&'source str),
    Macro(MacroNode<'source>),
}

/// Macro 源码是否显式包含对应的闭合标签。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MacroSyntaxKind {
    /// 单行调用，没有显式闭合标签。
    Inline,
    /// 显式闭合标签包裹的正文容器。
    Container,
}

/// 尚未绑定运行时定义的通用 Macro 调用。
#[derive(Debug, PartialEq, Eq)]
pub struct MacroNode<'source> {
    pub name: &'source str,
    pub arguments: &'source str,
    pub arguments_span: Span,
    pub syntax_kind: MacroSyntaxKind,
    pub body: Vec<BodyNode<'source>>,
}

/// 一个声明及其后续正文组成的最小 Passage。
#[derive(Debug, PartialEq, Eq)]
pub struct Passage<'source> {
    pub source: &'source SourcePath,
    pub content: &'source str,
    pub name: &'source str,
    pub tags: Vec<&'source str>,
    pub body: Vec<BodyNode<'source>>,
    pub span: Span,
}

/// 所有 Twee Source 汇总后的最小故事结构。
#[derive(Debug, PartialEq, Eq)]
pub struct Story<'source> {
    pub passages: Vec<Passage<'source>>,
}

/// Parser 当前能够报告的结构错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseErrorKind<'source> {
    TextBeforeDeclaration,
    EmptyPassageName,
    UnclosedMacro {
        name: &'source str,
    },
    MismatchedMacroClosing {
        expected: &'source str,
        found: &'source str,
    },
    UnexpectedMacroClosing {
        name: &'source str,
    },
    UngroupedMacroShift,
    UnclosedComment,
}

/// 带源码位置的 Twee 解析错误。
#[derive(Debug, PartialEq, Eq)]
pub struct ParseError<'source> {
    pub source: &'source SourcePath,
    pub kind: ParseErrorKind<'source>,
    pub span: Span,
}

impl ParseError<'_> {
    /// 转换为带相对 Source 与精确 Span 的公共 Diagnostic。
    pub fn diagnostic(&self) -> Diagnostic {
        let (code, message): (&str, String) = match self.kind {
            ParseErrorKind::TextBeforeDeclaration => (
                "twee.text_before_declaration",
                "首个 Passage 声明前不能出现文本".to_owned(),
            ),
            ParseErrorKind::EmptyPassageName => {
                ("twee.empty_passage_name", "Passage 名称不能为空".to_owned())
            }
            ParseErrorKind::UnclosedMacro { name } => {
                ("twee.unclosed_macro", format!("Macro `{name}` 缺少闭合符"))
            }
            ParseErrorKind::MismatchedMacroClosing { expected, found } => (
                "twee.mismatched_macro_closing",
                format!("Macro 闭合名称不匹配，预期 `{expected}`，实际 `{found}`"),
            ),
            ParseErrorKind::UnexpectedMacroClosing { name } => (
                "twee.unexpected_macro_closing",
                format!("Macro `{name}` 的闭合符没有对应开头"),
            ),
            ParseErrorKind::UngroupedMacroShift => (
                "twee.ungrouped_macro_shift",
                "Macro 参数中的位移运算符必须放在圆括号内".to_owned(),
            ),
            ParseErrorKind::UnclosedComment => {
                ("twee.unclosed_comment", "Twee 注释缺少 `%/`".to_owned())
            }
        };

        Diagnostic::new(code, DiagnosticSeverity::Error, &message).with_location(
            DiagnosticLocation {
                source: self.source.as_str().to_owned(),
                start: self.span.start,
                end: self.span.end,
                line: self.span.line,
                column: self.span.column,
            },
        )
    }
}

impl fmt::Display for ParseError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}: ",
            self.source.as_str(),
            self.span.line,
            self.span.column
        )?;

        match self.kind {
            ParseErrorKind::TextBeforeDeclaration => {
                formatter.write_str("首个 Passage 声明前不能出现文本")
            }
            ParseErrorKind::EmptyPassageName => formatter.write_str("Passage 名称不能为空"),
            ParseErrorKind::UnclosedMacro { name } => {
                write!(formatter, "Macro `{name}` 缺少闭合符")
            }
            ParseErrorKind::MismatchedMacroClosing { expected, found } => write!(
                formatter,
                "Macro 闭合名称不匹配，预期 `{expected}`，实际 `{found}`"
            ),
            ParseErrorKind::UnexpectedMacroClosing { name } => {
                write!(formatter, "Macro `{name}` 的闭合符没有对应开头")
            }
            ParseErrorKind::UngroupedMacroShift => {
                formatter.write_str("Macro 参数中的位移运算符必须放在圆括号内")
            }
            ParseErrorKind::UnclosedComment => formatter.write_str("Twee 注释缺少 `%/`"),
        }
    }
}

impl Error for ParseError<'_> {}

/// 当前最小语义检查能够报告的错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticErrorKind {
    DuplicatePassageName,
    SpecialPassageTags,
}

/// 带重复名称及其位置的语义错误。
#[derive(Debug, PartialEq, Eq)]
pub struct SemanticError<'source> {
    pub source: &'source SourcePath,
    pub name: &'source str,
    pub kind: SemanticErrorKind,
    pub span: Span,
}

impl SemanticError<'_> {
    /// 转换为指向重复声明位置的公共 Diagnostic。
    pub fn diagnostic(&self) -> Diagnostic {
        match self.kind {
            SemanticErrorKind::DuplicatePassageName => Diagnostic::new(
                "twee.duplicate_passage_name",
                DiagnosticSeverity::Error,
                &format!("Passage 名称 `{}` 重复", self.name),
            )
            .with_location(DiagnosticLocation {
                source: self.source.as_str().to_owned(),
                start: self.span.start,
                end: self.span.end,
                line: self.span.line,
                column: self.span.column,
            }),
            SemanticErrorKind::SpecialPassageTags => Diagnostic::new(
                "twee.special_passage_tags",
                DiagnosticSeverity::Error,
                &format!("特殊 Passage `{}` 不能带有 Tag", self.name),
            )
            .with_location(DiagnosticLocation {
                source: self.source.as_str().to_owned(),
                start: self.span.start,
                end: self.span.end,
                line: self.span.line,
                column: self.span.column,
            }),
        }
    }
}

impl fmt::Display for SemanticError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}: ",
            self.source.as_str(),
            self.span.line,
            self.span.column,
        )?;
        match self.kind {
            SemanticErrorKind::DuplicatePassageName => {
                write!(formatter, "Passage 名称 `{}` 重复", self.name)
            }
            SemanticErrorKind::SpecialPassageTags => {
                write!(formatter, "特殊 Passage `{}` 不能带有 Tag", self.name)
            }
        }
    }
}

impl Error for SemanticError<'_> {}

/// Story 构建时保留解析错误与语义错误的原始类型。
#[derive(Debug, PartialEq, Eq)]
pub enum StoryError<'source> {
    Parse(ParseError<'source>),
    Semantic(SemanticError<'source>),
}

impl StoryError<'_> {
    /// 无需调用方拆分错误阶段即可取得公共 Diagnostic。
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::Parse(error) => error.diagnostic(),
            Self::Semantic(error) => error.diagnostic(),
        }
    }
}

impl fmt::Display for StoryError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(formatter),
            Self::Semantic(error) => error.fmt(formatter),
        }
    }
}

impl Error for StoryError<'_> {}

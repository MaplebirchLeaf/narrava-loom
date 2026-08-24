//! Twee Lexer、Parser 与 Story 行为测试。

use std::path::Path;

use crate::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::source::{Source, SourceList, SourcePath};

use crate::twee::{
    BodyNode, BodyNodeKind, MacroNode, ParseError, ParseErrorKind, Passage, SemanticError,
    SemanticErrorKind, Story, StoryError, Token, TokenKind, lex, parse, parse_fragment, validate,
};

// 大型测试集按用例边界拆成物理分片；门面保留共享导入与模块说明。
// include 保持原模块命名、私有辅助项可见性和测试发现路径不变。
include!("twee/part_01.rs");
include!("twee/part_02.rs");

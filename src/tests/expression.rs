//! Expression Lexer 与 Parser 的行为测试。

use crate::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::expression::{
    AssignmentOperator, BetweenBounds, BinaryOperator, Expression, ExpressionKind, LexError,
    ObjectKey, ObjectProperty, OwnedExpression, ParseError, Span, Token, TokenKind, UnaryOperator,
    UpdateOperator, UpdatePosition, VariableScope, lex, parse, parse_list,
};

// 大型测试集按用例边界拆成物理分片；门面保留共享导入与模块说明。
// include 保持原模块命名、私有辅助项可见性和测试发现路径不变。
include!("expression/part_01.rs");
include!("expression/part_02.rs");

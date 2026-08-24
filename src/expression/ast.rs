//! Expression 的语法树数据结构，不负责解析或求值。

/// Expression 内容中的 UTF-8 字节范围。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// 变量前缀决定的运行时所有者。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VariableScope {
    Variables,
    Temporary,
    Local,
}

/// 一个带完整源码范围的表达式节点。
#[derive(Debug, PartialEq, Eq)]
pub struct Expression<'source> {
    pub kind: ExpressionKind<'source>,
    pub span: Span,
}

/// 首轮对象键只区分裸标识符与字符串。
#[derive(Debug, PartialEq, Eq)]
pub enum ObjectKey<'source> {
    Identifier(&'source str),
    String(&'source str),
}

/// 一个保留键和值位置的对象属性。
#[derive(Debug, PartialEq, Eq)]
pub struct ObjectProperty<'source> {
    pub key: ObjectKey<'source>,
    pub key_span: Span,
    pub value: Expression<'source>,
}

/// 一元运算符在 AST 中使用规范形式。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UnaryOperator {
    LogicalNot,
    BitwiseNot,
    Positive,
    Negative,
    TypeOf,
}

/// 二元 AST 使用规范运算符；英文别名不会进入这一层。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BinaryOperator {
    Add,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    Divide,
    Equal,
    Greater,
    GreaterEqual,
    In,
    InstanceOf,
    IntegerDivide,
    Less,
    LessEqual,
    LogicalAnd,
    LogicalOr,
    Multiply,
    NotEqual,
    NotIn,
    NullishCoalesce,
    Power,
    Remainder,
    ShiftLeft,
    ShiftRight,
    Subtract,
    StrictEqual,
    StrictNotEqual,
    ThreeWayCompare,
    UnsignedShiftRight,
}

/// `between` 左右边界的开闭组合。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BetweenBounds {
    OpenOpen,
    OpenClosed,
    ClosedOpen,
    ClosedClosed,
}

/// 赋值独立于普通二元运算，便于 VM 明确执行写入。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AssignmentOperator {
    Add,
    Assign,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    Divide,
    IntegerDivide,
    LogicalAnd,
    LogicalOr,
    Multiply,
    NullishCoalesce,
    Power,
    Remainder,
    ShiftLeft,
    ShiftRight,
    Subtract,
    UnsignedShiftRight,
}

/// 自增、自减共享的更新种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UpdateOperator {
    Increment,
    Decrement,
}

/// 更新位置决定表达式返回更新前还是更新后的值。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UpdatePosition {
    Prefix,
    Postfix,
}

/// Parser 生成的 Expression 节点种类。
#[derive(Debug, PartialEq, Eq)]
pub enum ExpressionKind<'source> {
    Array(Vec<Expression<'source>>),
    Assignment {
        operator: AssignmentOperator,
        target: Box<Expression<'source>>,
        value: Box<Expression<'source>>,
    },
    Between {
        bounds: BetweenBounds,
        value: Box<Expression<'source>>,
        lower: Box<Expression<'source>>,
        upper: Box<Expression<'source>>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<Expression<'source>>,
        right: Box<Expression<'source>>,
    },
    Boolean(bool),
    Call {
        callee: Box<Expression<'source>>,
        arguments: Vec<Expression<'source>>,
    },
    Conditional {
        condition: Box<Expression<'source>>,
        consequent: Box<Expression<'source>>,
        alternate: Box<Expression<'source>>,
    },
    Global(&'source str),
    Group(Box<Expression<'source>>),
    Index {
        target: Box<Expression<'source>>,
        index: Box<Expression<'source>>,
    },
    Member {
        target: Box<Expression<'source>>,
        property: &'source str,
        property_span: Span,
    },
    Null,
    Number(&'source str),
    Object(Vec<ObjectProperty<'source>>),
    OptionalCall {
        callee: Box<Expression<'source>>,
        arguments: Vec<Expression<'source>>,
    },
    OptionalIndex {
        target: Box<Expression<'source>>,
        index: Box<Expression<'source>>,
    },
    OptionalMember {
        target: Box<Expression<'source>>,
        property: &'source str,
        property_span: Span,
    },
    Setup,
    String(&'source str),
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression<'source>>,
    },
    Update {
        operator: UpdateOperator,
        position: UpdatePosition,
        target: Box<Expression<'source>>,
    },
    Undefined,
    Variable {
        scope: VariableScope,
        name: &'source str,
    },
}

impl Expression<'_> {
    /// 赋值只能写入名称、变量或没有可选链的属性位置。
    pub fn is_assignable_target(&self) -> bool {
        match &self.kind {
            ExpressionKind::Global(_) | ExpressionKind::Variable { .. } => true,
            ExpressionKind::Group(inner) => inner.is_assignable_target(),
            ExpressionKind::Index { target, .. } | ExpressionKind::Member { target, .. } => {
                !target.has_optional_chain()
            }
            _ => false,
        }
    }

    fn has_optional_chain(&self) -> bool {
        match &self.kind {
            ExpressionKind::OptionalCall { .. }
            | ExpressionKind::OptionalIndex { .. }
            | ExpressionKind::OptionalMember { .. } => true,
            ExpressionKind::Call { callee, .. } => callee.has_optional_chain(),
            ExpressionKind::Group(inner) => inner.has_optional_chain(),
            ExpressionKind::Index { target, .. } | ExpressionKind::Member { target, .. } => {
                target.has_optional_chain()
            }
            _ => false,
        }
    }
}

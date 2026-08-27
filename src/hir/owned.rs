//! 拥有型 HIR：与借用型 HIR 同构但持有自己的数据，可序列化。
//!
//! 供 Bytecode 与发布产物使用，不依赖源码生命周期；`as_hir` 提供回借视图。

use serde::{Deserialize, Serialize};

mod conversion;

use super::*;
use crate::expression::OwnedExpression;

/// 拥有型正文节点；与 [`HirBodyNode`] 同构，不依赖源码生命周期，可序列化。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirBodyNode {
    pub kind: OwnedHirBodyKind,
    pub span: twee::Span,
}

/// 拥有型正文语义；对应 [`HirBodyKind`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnedHirBodyKind {
    Text(String),
    HardBreak,
    Print(OwnedHirPrint),
    Silently(Vec<OwnedHirBodyNode>),
    If(OwnedHirIf),
    Switch(Box<OwnedHirSwitch>),
    For(Box<OwnedHirFor>),
    While(Box<OwnedHirWhile>),
    Break,
    Continue,
    Exit,
    Set(Box<OwnedExpression>),
    Unset(Box<OwnedExpression>),
    Run(Box<OwnedExpression>),
    Include(Box<OwnedExpression>),
    Goto(Box<OwnedExpression>),
    Widget(OwnedHirWidget),
    Return(Option<Box<OwnedExpression>>),
    Capture(OwnedHirCapture),
    Macro(OwnedHirMacro),
}

/// 拥有型 `print` 参数；对应 [`HirPrint`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnedHirPrint {
    Expression(OwnedExpression),
    Literal(String),
}

/// 拥有型 Widget 定义；对应 [`HirWidget`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirWidget {
    pub name: String,
    pub body: Vec<OwnedHirBodyNode>,
}

/// 拥有型 capture 结构；对应 [`HirCapture`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirCapture {
    pub locals: Vec<String>,
    pub body: Vec<OwnedHirBodyNode>,
}

/// 拥有型 switch 结构；对应 [`HirSwitch`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirSwitch {
    pub value: OwnedExpression,
    pub cases: Vec<OwnedHirSwitchCase>,
    pub default: Option<Vec<OwnedHirBodyNode>>,
}

/// 拥有型 switch case；对应 [`HirSwitchCase`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirSwitchCase {
    pub value: OwnedExpression,
    pub body: Vec<OwnedHirBodyNode>,
}

/// 拥有型 while 循环；对应 [`HirWhile`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirWhile {
    pub condition: OwnedExpression,
    pub body: Vec<OwnedHirBodyNode>,
}

/// 拥有型 for 循环；对应 [`HirFor`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirFor {
    pub target: OwnedHirForTarget,
    pub kind: OwnedHirForKind,
    pub body: Vec<OwnedHirBodyNode>,
}

/// 拥有型 for 写入目标；对应 [`HirForTarget`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirForTarget {
    pub value: OwnedExpression,
    pub span: twee::Span,
}

/// 拥有型 for 迭代模式；对应 [`HirForKind`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnedHirForKind {
    In {
        collection: OwnedExpression,
        span: twee::Span,
    },
    Of {
        collection: OwnedExpression,
        span: twee::Span,
    },
    Range {
        start: OwnedExpression,
        start_span: twee::Span,
        end: OwnedExpression,
        end_span: twee::Span,
        step: Option<Box<OwnedExpression>>,
        step_span: Option<twee::Span>,
    },
}

/// 拥有型 if 结构；对应 [`HirIf`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirIf {
    pub branches: Vec<OwnedHirIfBranch>,
    pub fallback: Option<Vec<OwnedHirBodyNode>>,
}

/// 拥有型 if 分支；对应 [`HirIfBranch`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirIfBranch {
    pub condition: OwnedExpression,
    pub body: Vec<OwnedHirBodyNode>,
}

/// 拥有型通用 Macro 调用；对应 [`HirMacro`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirMacro {
    pub name: String,
    pub arguments: OwnedHirMacroArguments,
    pub syntax_kind: twee::MacroSyntaxKind,
    pub body: Vec<OwnedHirBodyNode>,
}

/// 拥有型 Macro 参数类别；对应 [`HirMacroArguments`]。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnedHirMacroArguments {
    None,
    Raw(String),
    Expression(OwnedExpression),
}

impl OwnedHirBodyNode {
    /// 借出内部数据，构造与源码生命周期无关的借用型正文节点。
    pub fn as_hir(&self) -> HirBodyNode<'_> {
        HirBodyNode {
            kind: self.kind.as_hir(),
            span: self.span,
        }
    }
}

impl OwnedHirBodyKind {
    fn as_hir(&self) -> HirBodyKind<'_> {
        match self {
            Self::Text(text) => HirBodyKind::Text(text),
            Self::HardBreak => HirBodyKind::HardBreak,
            Self::Print(value) => HirBodyKind::Print(value.as_hir()),
            Self::Silently(body) => HirBodyKind::Silently(borrow_body(body)),
            Self::If(value) => HirBodyKind::If(value.as_hir()),
            Self::Switch(value) => HirBodyKind::Switch(Box::new(value.as_hir())),
            Self::For(value) => HirBodyKind::For(Box::new(value.as_hir())),
            Self::While(value) => HirBodyKind::While(Box::new(value.as_hir())),
            Self::Break => HirBodyKind::Break,
            Self::Continue => HirBodyKind::Continue,
            Self::Exit => HirBodyKind::Exit,
            Self::Set(value) => HirBodyKind::Set(Box::new(value.as_expression())),
            Self::Unset(value) => HirBodyKind::Unset(Box::new(value.as_expression())),
            Self::Run(value) => HirBodyKind::Run(Box::new(value.as_expression())),
            Self::Include(value) => HirBodyKind::Include(Box::new(value.as_expression())),
            Self::Goto(value) => HirBodyKind::Goto(Box::new(value.as_expression())),
            Self::Widget(value) => HirBodyKind::Widget(value.as_hir()),
            Self::Return(value) => HirBodyKind::Return(
                value
                    .as_deref()
                    .map(OwnedExpression::as_expression)
                    .map(Box::new),
            ),
            Self::Capture(value) => HirBodyKind::Capture(value.as_hir()),
            Self::Macro(value) => HirBodyKind::Macro(value.as_hir()),
        }
    }
}

impl OwnedHirPrint {
    fn as_hir(&self) -> HirPrint<'_> {
        match self {
            Self::Expression(value) => HirPrint::Expression(value.as_expression()),
            Self::Literal(value) => HirPrint::Literal(value),
        }
    }
}

impl OwnedHirWidget {
    fn as_hir(&self) -> HirWidget<'_> {
        HirWidget {
            name: &self.name,
            body: borrow_body(&self.body),
        }
    }
}

impl OwnedHirCapture {
    fn as_hir(&self) -> HirCapture<'_> {
        HirCapture {
            locals: self.locals.iter().map(String::as_str).collect(),
            body: borrow_body(&self.body),
        }
    }
}

impl OwnedHirSwitch {
    fn as_hir(&self) -> HirSwitch<'_> {
        HirSwitch {
            value: self.value.as_expression(),
            cases: self.cases.iter().map(OwnedHirSwitchCase::as_hir).collect(),
            default: self.default.as_deref().map(borrow_body),
        }
    }
}

impl OwnedHirSwitchCase {
    fn as_hir(&self) -> HirSwitchCase<'_> {
        HirSwitchCase {
            value: self.value.as_expression(),
            body: borrow_body(&self.body),
        }
    }
}

impl OwnedHirWhile {
    fn as_hir(&self) -> HirWhile<'_> {
        HirWhile {
            condition: self.condition.as_expression(),
            body: borrow_body(&self.body),
        }
    }
}

impl OwnedHirFor {
    fn as_hir(&self) -> HirFor<'_> {
        HirFor {
            target: self.target.as_hir(),
            kind: self.kind.as_hir(),
            body: borrow_body(&self.body),
        }
    }
}

impl OwnedHirForTarget {
    fn as_hir(&self) -> HirForTarget<'_> {
        HirForTarget {
            value: self.value.as_expression(),
            span: self.span,
        }
    }
}

impl OwnedHirForKind {
    fn as_hir(&self) -> HirForKind<'_> {
        match self {
            Self::In { collection, span } => HirForKind::In {
                collection: collection.as_expression(),
                span: *span,
            },
            Self::Of { collection, span } => HirForKind::Of {
                collection: collection.as_expression(),
                span: *span,
            },
            Self::Range {
                start,
                start_span,
                end,
                end_span,
                step,
                step_span,
            } => HirForKind::Range {
                start: start.as_expression(),
                start_span: *start_span,
                end: end.as_expression(),
                end_span: *end_span,
                step: step.as_deref().map(OwnedExpression::as_expression),
                step_span: *step_span,
            },
        }
    }
}

impl OwnedHirIf {
    fn as_hir(&self) -> HirIf<'_> {
        HirIf {
            branches: self.branches.iter().map(OwnedHirIfBranch::as_hir).collect(),
            fallback: self.fallback.as_deref().map(borrow_body),
        }
    }
}

impl OwnedHirIfBranch {
    fn as_hir(&self) -> HirIfBranch<'_> {
        HirIfBranch {
            condition: self.condition.as_expression(),
            body: borrow_body(&self.body),
        }
    }
}

impl OwnedHirMacro {
    /// 借出内部数据，构造借用型 Macro 调用。
    pub fn as_hir(&self) -> HirMacro<'_> {
        HirMacro {
            name: &self.name,
            arguments: self.arguments.as_hir(),
            syntax_kind: self.syntax_kind,
            body: borrow_body(&self.body),
        }
    }
}

impl OwnedHirMacroArguments {
    fn as_hir(&self) -> HirMacroArguments<'_> {
        match self {
            Self::None => HirMacroArguments::None,
            Self::Raw(raw) => HirMacroArguments::Raw(raw),
            Self::Expression(expression) => {
                HirMacroArguments::Expression(expression.as_expression())
            }
        }
    }
}

fn own_body(body: &[HirBodyNode<'_>]) -> Vec<OwnedHirBodyNode> {
    body.iter().map(OwnedHirBodyNode::from).collect()
}

fn borrow_body(body: &[OwnedHirBodyNode]) -> Vec<HirBodyNode<'_>> {
    body.iter().map(OwnedHirBodyNode::as_hir).collect()
}

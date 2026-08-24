//! Bytecode 与发布产物使用的拥有型 Macro HIR。

use serde::{Deserialize, Serialize};

use super::*;
use crate::expression::OwnedExpression;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirBodyNode {
    pub kind: OwnedHirBodyKind,
    pub span: twee::Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnedHirBodyKind {
    Text(String),
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnedHirPrint {
    Expression(OwnedExpression),
    Literal(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirWidget {
    pub name: String,
    pub body: Vec<OwnedHirBodyNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirCapture {
    pub locals: Vec<String>,
    pub body: Vec<OwnedHirBodyNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirSwitch {
    pub value: OwnedExpression,
    pub cases: Vec<OwnedHirSwitchCase>,
    pub default: Option<Vec<OwnedHirBodyNode>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirSwitchCase {
    pub value: OwnedExpression,
    pub body: Vec<OwnedHirBodyNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirWhile {
    pub condition: OwnedExpression,
    pub body: Vec<OwnedHirBodyNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirFor {
    pub target: OwnedHirForTarget,
    pub kind: OwnedHirForKind,
    pub body: Vec<OwnedHirBodyNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirForTarget {
    pub value: OwnedExpression,
    pub span: twee::Span,
}

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirIf {
    pub branches: Vec<OwnedHirIfBranch>,
    pub fallback: Option<Vec<OwnedHirBodyNode>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirIfBranch {
    pub condition: OwnedExpression,
    pub body: Vec<OwnedHirBodyNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedHirMacro {
    pub name: String,
    pub arguments: OwnedHirMacroArguments,
    pub syntax_kind: twee::MacroSyntaxKind,
    pub body: Vec<OwnedHirBodyNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnedHirMacroArguments {
    None,
    Raw(String),
    Expression(OwnedExpression),
}

impl From<&HirBodyNode<'_>> for OwnedHirBodyNode {
    fn from(node: &HirBodyNode<'_>) -> Self {
        Self {
            kind: OwnedHirBodyKind::from(&node.kind),
            span: node.span,
        }
    }
}

impl From<&HirBodyKind<'_>> for OwnedHirBodyKind {
    fn from(kind: &HirBodyKind<'_>) -> Self {
        match kind {
            HirBodyKind::Text(text) => Self::Text((*text).to_owned()),
            HirBodyKind::Print(print) => Self::Print(OwnedHirPrint::from(print)),
            HirBodyKind::Silently(body) => Self::Silently(own_body(body)),
            HirBodyKind::If(value) => Self::If(OwnedHirIf::from(value)),
            HirBodyKind::Switch(value) => {
                Self::Switch(Box::new(OwnedHirSwitch::from(value.as_ref())))
            }
            HirBodyKind::For(value) => Self::For(Box::new(OwnedHirFor::from(value.as_ref()))),
            HirBodyKind::While(value) => Self::While(Box::new(OwnedHirWhile::from(value.as_ref()))),
            HirBodyKind::Break => Self::Break,
            HirBodyKind::Continue => Self::Continue,
            HirBodyKind::Exit => Self::Exit,
            HirBodyKind::Set(value) => Self::Set(Box::new(OwnedExpression::from(value.as_ref()))),
            HirBodyKind::Unset(value) => {
                Self::Unset(Box::new(OwnedExpression::from(value.as_ref())))
            }
            HirBodyKind::Run(value) => Self::Run(Box::new(OwnedExpression::from(value.as_ref()))),
            HirBodyKind::Include(value) => {
                Self::Include(Box::new(OwnedExpression::from(value.as_ref())))
            }
            HirBodyKind::Goto(value) => Self::Goto(Box::new(OwnedExpression::from(value.as_ref()))),
            HirBodyKind::Widget(value) => Self::Widget(OwnedHirWidget::from(value)),
            HirBodyKind::Return(value) => {
                Self::Return(value.as_deref().map(OwnedExpression::from).map(Box::new))
            }
            HirBodyKind::Capture(value) => Self::Capture(OwnedHirCapture::from(value)),
            HirBodyKind::Macro(value) => Self::Macro(OwnedHirMacro::from(value)),
        }
    }
}

impl From<&HirPrint<'_>> for OwnedHirPrint {
    fn from(value: &HirPrint<'_>) -> Self {
        match value {
            HirPrint::Expression(expression) => Self::Expression(OwnedExpression::from(expression)),
            HirPrint::Literal(text) => Self::Literal((*text).to_owned()),
        }
    }
}

impl From<&HirWidget<'_>> for OwnedHirWidget {
    fn from(value: &HirWidget<'_>) -> Self {
        Self {
            name: value.name.to_owned(),
            body: own_body(&value.body),
        }
    }
}

impl From<&HirCapture<'_>> for OwnedHirCapture {
    fn from(value: &HirCapture<'_>) -> Self {
        Self {
            locals: value.locals.iter().map(|name| (*name).to_owned()).collect(),
            body: own_body(&value.body),
        }
    }
}

impl From<&HirSwitch<'_>> for OwnedHirSwitch {
    fn from(value: &HirSwitch<'_>) -> Self {
        Self {
            value: OwnedExpression::from(&value.value),
            cases: value.cases.iter().map(OwnedHirSwitchCase::from).collect(),
            default: value.default.as_deref().map(own_body),
        }
    }
}

impl From<&HirSwitchCase<'_>> for OwnedHirSwitchCase {
    fn from(value: &HirSwitchCase<'_>) -> Self {
        Self {
            value: OwnedExpression::from(&value.value),
            body: own_body(&value.body),
        }
    }
}

impl From<&HirWhile<'_>> for OwnedHirWhile {
    fn from(value: &HirWhile<'_>) -> Self {
        Self {
            condition: OwnedExpression::from(&value.condition),
            body: own_body(&value.body),
        }
    }
}

impl From<&HirFor<'_>> for OwnedHirFor {
    fn from(value: &HirFor<'_>) -> Self {
        Self {
            target: OwnedHirForTarget::from(&value.target),
            kind: OwnedHirForKind::from(&value.kind),
            body: own_body(&value.body),
        }
    }
}

impl From<&HirForTarget<'_>> for OwnedHirForTarget {
    fn from(value: &HirForTarget<'_>) -> Self {
        Self {
            value: OwnedExpression::from(&value.value),
            span: value.span,
        }
    }
}

impl From<&HirForKind<'_>> for OwnedHirForKind {
    fn from(value: &HirForKind<'_>) -> Self {
        match value {
            HirForKind::In { collection, span } => Self::In {
                collection: OwnedExpression::from(collection),
                span: *span,
            },
            HirForKind::Of { collection, span } => Self::Of {
                collection: OwnedExpression::from(collection),
                span: *span,
            },
            HirForKind::Range {
                start,
                start_span,
                end,
                end_span,
                step,
                step_span,
            } => Self::Range {
                start: OwnedExpression::from(start),
                start_span: *start_span,
                end: OwnedExpression::from(end),
                end_span: *end_span,
                step: step.as_ref().map(OwnedExpression::from).map(Box::new),
                step_span: *step_span,
            },
        }
    }
}

impl From<&HirIf<'_>> for OwnedHirIf {
    fn from(value: &HirIf<'_>) -> Self {
        Self {
            branches: value.branches.iter().map(OwnedHirIfBranch::from).collect(),
            fallback: value.fallback.as_deref().map(own_body),
        }
    }
}

impl From<&HirIfBranch<'_>> for OwnedHirIfBranch {
    fn from(value: &HirIfBranch<'_>) -> Self {
        Self {
            condition: OwnedExpression::from(&value.condition),
            body: own_body(&value.body),
        }
    }
}

impl From<&HirMacro<'_>> for OwnedHirMacro {
    fn from(value: &HirMacro<'_>) -> Self {
        Self {
            name: value.name.to_owned(),
            arguments: OwnedHirMacroArguments::from(&value.arguments),
            syntax_kind: value.syntax_kind,
            body: own_body(&value.body),
        }
    }
}

impl From<&HirMacroArguments<'_>> for OwnedHirMacroArguments {
    fn from(value: &HirMacroArguments<'_>) -> Self {
        match value {
            HirMacroArguments::None => Self::None,
            HirMacroArguments::Raw(raw) => Self::Raw((*raw).to_owned()),
            HirMacroArguments::Expression(expression) => {
                Self::Expression(OwnedExpression::from(expression))
            }
        }
    }
}

impl OwnedHirBodyNode {
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

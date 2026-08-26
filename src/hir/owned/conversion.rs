//! 借用型 HIR 到可序列化拥有型 HIR 的深复制。
//!
//! 所有递归转换集中在这里，确保新增 HIR 变体时只有一处需要同步拥有化规则；
//! 反向借用视图仍由父模块实现，二者不会混在同一段大型 impl 列表中。

use super::*;

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

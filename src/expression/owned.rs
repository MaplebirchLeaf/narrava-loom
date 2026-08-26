//! 可写入发布产物、并能重建借用 AST 的拥有型 Expression。

use serde::{Deserialize, Serialize};

use super::{
    AssignmentOperator, BetweenBounds, BinaryOperator, Expression, ExpressionKind, ObjectKey,
    ObjectProperty, Span, UnaryOperator, UpdateOperator, UpdatePosition, VariableScope,
};

/// 拥有所有权的 Expression；可序列化进发布产物，并可从借用 AST 转换而来。
/// 拥有所有权的 Expression；可序列化进发布产物，并可从借用 AST 转换而来。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedExpression {
    pub kind: OwnedExpressionKind,
    pub span: Span,
}

/// 对象键的拥有型版本，对应借用 AST 的 ObjectKey。
/// 对象键的拥有型版本，对应借用 AST 的 ObjectKey。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnedObjectKey {
    Identifier(String),
    String(String),
}

/// 对象属性的拥有型版本，保留键与值各自的源码位置。
/// 对象属性的拥有型版本，保留键与值各自的源码位置。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedObjectProperty {
    pub key: OwnedObjectKey,
    pub key_span: Span,
    pub value: OwnedExpression,
}

/// 与借用型 ExpressionKind 同构的拥有型节点种类。
/// 与借用型 ExpressionKind 同构的拥有型节点种类。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnedExpressionKind {
    Array(Vec<OwnedExpression>),
    Assignment {
        operator: AssignmentOperator,
        target: Box<OwnedExpression>,
        value: Box<OwnedExpression>,
    },
    Between {
        bounds: BetweenBounds,
        value: Box<OwnedExpression>,
        lower: Box<OwnedExpression>,
        upper: Box<OwnedExpression>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<OwnedExpression>,
        right: Box<OwnedExpression>,
    },
    Boolean(bool),
    Call {
        callee: Box<OwnedExpression>,
        arguments: Vec<OwnedExpression>,
    },
    Conditional {
        condition: Box<OwnedExpression>,
        consequent: Box<OwnedExpression>,
        alternate: Box<OwnedExpression>,
    },
    Global(String),
    Group(Box<OwnedExpression>),
    Index {
        target: Box<OwnedExpression>,
        index: Box<OwnedExpression>,
    },
    Member {
        target: Box<OwnedExpression>,
        property: String,
        property_span: Span,
    },
    Null,
    Number(String),
    Object(Vec<OwnedObjectProperty>),
    OptionalCall {
        callee: Box<OwnedExpression>,
        arguments: Vec<OwnedExpression>,
    },
    OptionalIndex {
        target: Box<OwnedExpression>,
        index: Box<OwnedExpression>,
    },
    OptionalMember {
        target: Box<OwnedExpression>,
        property: String,
        property_span: Span,
    },
    Setup,
    String(String),
    Unary {
        operator: UnaryOperator,
        operand: Box<OwnedExpression>,
    },
    Update {
        operator: UpdateOperator,
        position: UpdatePosition,
        target: Box<OwnedExpression>,
    },
    Undefined,
    Variable {
        scope: VariableScope,
        name: String,
    },
}

impl From<&Expression<'_>> for OwnedExpression {
    fn from(expression: &Expression<'_>) -> Self {
        Self {
            kind: OwnedExpressionKind::from(&expression.kind),
            span: expression.span,
        }
    }
}

impl From<&ExpressionKind<'_>> for OwnedExpressionKind {
    fn from(kind: &ExpressionKind<'_>) -> Self {
        match kind {
            ExpressionKind::Array(values) => Self::Array(owned_list(values)),
            ExpressionKind::Assignment {
                operator,
                target,
                value,
            } => Self::Assignment {
                operator: *operator,
                target: Box::new(OwnedExpression::from(target.as_ref())),
                value: Box::new(OwnedExpression::from(value.as_ref())),
            },
            ExpressionKind::Between {
                bounds,
                value,
                lower,
                upper,
            } => Self::Between {
                bounds: *bounds,
                value: Box::new(OwnedExpression::from(value.as_ref())),
                lower: Box::new(OwnedExpression::from(lower.as_ref())),
                upper: Box::new(OwnedExpression::from(upper.as_ref())),
            },
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => Self::Binary {
                operator: *operator,
                left: Box::new(OwnedExpression::from(left.as_ref())),
                right: Box::new(OwnedExpression::from(right.as_ref())),
            },
            ExpressionKind::Boolean(value) => Self::Boolean(*value),
            ExpressionKind::Call { callee, arguments } => Self::Call {
                callee: Box::new(OwnedExpression::from(callee.as_ref())),
                arguments: owned_list(arguments),
            },
            ExpressionKind::Conditional {
                condition,
                consequent,
                alternate,
            } => Self::Conditional {
                condition: Box::new(OwnedExpression::from(condition.as_ref())),
                consequent: Box::new(OwnedExpression::from(consequent.as_ref())),
                alternate: Box::new(OwnedExpression::from(alternate.as_ref())),
            },
            ExpressionKind::Global(name) => Self::Global((*name).to_owned()),
            ExpressionKind::Group(value) => {
                Self::Group(Box::new(OwnedExpression::from(value.as_ref())))
            }
            ExpressionKind::Index { target, index } => Self::Index {
                target: Box::new(OwnedExpression::from(target.as_ref())),
                index: Box::new(OwnedExpression::from(index.as_ref())),
            },
            ExpressionKind::Member {
                target,
                property,
                property_span,
            } => Self::Member {
                target: Box::new(OwnedExpression::from(target.as_ref())),
                property: (*property).to_owned(),
                property_span: *property_span,
            },
            ExpressionKind::Null => Self::Null,
            ExpressionKind::Number(value) => Self::Number((*value).to_owned()),
            ExpressionKind::Object(properties) => {
                Self::Object(properties.iter().map(OwnedObjectProperty::from).collect())
            }
            ExpressionKind::OptionalCall { callee, arguments } => Self::OptionalCall {
                callee: Box::new(OwnedExpression::from(callee.as_ref())),
                arguments: owned_list(arguments),
            },
            ExpressionKind::OptionalIndex { target, index } => Self::OptionalIndex {
                target: Box::new(OwnedExpression::from(target.as_ref())),
                index: Box::new(OwnedExpression::from(index.as_ref())),
            },
            ExpressionKind::OptionalMember {
                target,
                property,
                property_span,
            } => Self::OptionalMember {
                target: Box::new(OwnedExpression::from(target.as_ref())),
                property: (*property).to_owned(),
                property_span: *property_span,
            },
            ExpressionKind::Setup => Self::Setup,
            ExpressionKind::String(value) => Self::String((*value).to_owned()),
            ExpressionKind::Unary { operator, operand } => Self::Unary {
                operator: *operator,
                operand: Box::new(OwnedExpression::from(operand.as_ref())),
            },
            ExpressionKind::Update {
                operator,
                position,
                target,
            } => Self::Update {
                operator: *operator,
                position: *position,
                target: Box::new(OwnedExpression::from(target.as_ref())),
            },
            ExpressionKind::Undefined => Self::Undefined,
            ExpressionKind::Variable { scope, name } => Self::Variable {
                scope: *scope,
                name: (*name).to_owned(),
            },
        }
    }
}

impl From<&ObjectProperty<'_>> for OwnedObjectProperty {
    fn from(property: &ObjectProperty<'_>) -> Self {
        Self {
            key: match property.key {
                ObjectKey::Identifier(value) => OwnedObjectKey::Identifier(value.to_owned()),
                ObjectKey::String(value) => OwnedObjectKey::String(value.to_owned()),
            },
            key_span: property.key_span,
            value: OwnedExpression::from(&property.value),
        }
    }
}

impl OwnedExpression {
    /// 借用自身重建临时 AST；返回值的生命周期与自身绑定。
    pub fn as_expression(&self) -> Expression<'_> {
        Expression {
            kind: self.kind.as_expression_kind(),
            span: self.span,
        }
    }
}

impl OwnedExpressionKind {
    /// 把拥有型节点重建为借用节点，字符串与名称直接借用内部存储。
    fn as_expression_kind(&self) -> ExpressionKind<'_> {
        match self {
            Self::Array(values) => ExpressionKind::Array(borrowed_list(values)),
            Self::Assignment {
                operator,
                target,
                value,
            } => ExpressionKind::Assignment {
                operator: *operator,
                target: Box::new(target.as_expression()),
                value: Box::new(value.as_expression()),
            },
            Self::Between {
                bounds,
                value,
                lower,
                upper,
            } => ExpressionKind::Between {
                bounds: *bounds,
                value: Box::new(value.as_expression()),
                lower: Box::new(lower.as_expression()),
                upper: Box::new(upper.as_expression()),
            },
            Self::Binary {
                operator,
                left,
                right,
            } => ExpressionKind::Binary {
                operator: *operator,
                left: Box::new(left.as_expression()),
                right: Box::new(right.as_expression()),
            },
            Self::Boolean(value) => ExpressionKind::Boolean(*value),
            Self::Call { callee, arguments } => ExpressionKind::Call {
                callee: Box::new(callee.as_expression()),
                arguments: borrowed_list(arguments),
            },
            Self::Conditional {
                condition,
                consequent,
                alternate,
            } => ExpressionKind::Conditional {
                condition: Box::new(condition.as_expression()),
                consequent: Box::new(consequent.as_expression()),
                alternate: Box::new(alternate.as_expression()),
            },
            Self::Global(name) => ExpressionKind::Global(name),
            Self::Group(value) => ExpressionKind::Group(Box::new(value.as_expression())),
            Self::Index { target, index } => ExpressionKind::Index {
                target: Box::new(target.as_expression()),
                index: Box::new(index.as_expression()),
            },
            Self::Member {
                target,
                property,
                property_span,
            } => ExpressionKind::Member {
                target: Box::new(target.as_expression()),
                property,
                property_span: *property_span,
            },
            Self::Null => ExpressionKind::Null,
            Self::Number(value) => ExpressionKind::Number(value),
            Self::Object(properties) => ExpressionKind::Object(
                properties
                    .iter()
                    .map(OwnedObjectProperty::as_object_property)
                    .collect(),
            ),
            Self::OptionalCall { callee, arguments } => ExpressionKind::OptionalCall {
                callee: Box::new(callee.as_expression()),
                arguments: borrowed_list(arguments),
            },
            Self::OptionalIndex { target, index } => ExpressionKind::OptionalIndex {
                target: Box::new(target.as_expression()),
                index: Box::new(index.as_expression()),
            },
            Self::OptionalMember {
                target,
                property,
                property_span,
            } => ExpressionKind::OptionalMember {
                target: Box::new(target.as_expression()),
                property,
                property_span: *property_span,
            },
            Self::Setup => ExpressionKind::Setup,
            Self::String(value) => ExpressionKind::String(value),
            Self::Unary { operator, operand } => ExpressionKind::Unary {
                operator: *operator,
                operand: Box::new(operand.as_expression()),
            },
            Self::Update {
                operator,
                position,
                target,
            } => ExpressionKind::Update {
                operator: *operator,
                position: *position,
                target: Box::new(target.as_expression()),
            },
            Self::Undefined => ExpressionKind::Undefined,
            Self::Variable { scope, name } => ExpressionKind::Variable {
                scope: *scope,
                name,
            },
        }
    }
}

impl OwnedObjectProperty {
    /// 重建借用型对象属性。
    fn as_object_property(&self) -> ObjectProperty<'_> {
        ObjectProperty {
            key: match &self.key {
                OwnedObjectKey::Identifier(value) => ObjectKey::Identifier(value),
                OwnedObjectKey::String(value) => ObjectKey::String(value),
            },
            key_span: self.key_span,
            value: self.value.as_expression(),
        }
    }
}

/// 批量把借用节点列表转换为拥有型节点。
fn owned_list(values: &[Expression<'_>]) -> Vec<OwnedExpression> {
    values.iter().map(OwnedExpression::from).collect()
}

/// 批量把拥有型节点重建为借用节点列表。
fn borrowed_list(values: &[OwnedExpression]) -> Vec<Expression<'_>> {
    values.iter().map(OwnedExpression::as_expression).collect()
}

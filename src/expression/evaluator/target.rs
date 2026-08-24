//! 可写 Expression 目标的解析、读取与一次性根提交。

use super::chain::{
    canonical_index, index_property, native_function, native_namespace, property_name,
};
use super::{
    ContextWriteError, EvalError, EvaluationSession, WritableEvaluationContext, evaluate_in,
};
use crate::expression::{
    Expression, ExpressionKind, Span, VariableScope,
    value::{TextValue, Value},
};

/// Context 中真正承担一次性提交的根。
pub(super) enum AssignmentRoot {
    Global(String),
    Setup,
    Variable(VariableScope, String),
}

/// 根内部按源码顺序解析后的成员或索引。
pub(super) enum AssignmentSegment {
    Member { name: String, span: Span },
    Index { key: TextValue, span: Span },
}

/// 路径先完整解析，再读取和提交根，避免重复执行动态索引。
pub(super) struct AssignmentPath {
    pub(super) root: AssignmentRoot,
    pub(super) root_span: Span,
    pub(super) members: Vec<AssignmentSegment>,
}

impl AssignmentPath {
    pub(super) fn resolve(
        target: &Expression<'_>,
        session: &mut EvaluationSession<'_>,
    ) -> Result<Self, EvalError> {
        match &target.kind {
            ExpressionKind::Global(name) => {
                if native_function(name).is_some() || native_namespace(name).is_some() {
                    Err(EvalError::ReservedGlobal(target.span))
                } else {
                    Ok(Self {
                        root: AssignmentRoot::Global(String::from(*name)),
                        root_span: target.span,
                        members: Vec::new(),
                    })
                }
            }
            ExpressionKind::Variable { scope, name } => Ok(Self {
                root: AssignmentRoot::Variable(*scope, String::from(*name)),
                root_span: target.span,
                members: Vec::new(),
            }),
            ExpressionKind::Setup => Ok(Self {
                root: AssignmentRoot::Setup,
                root_span: target.span,
                members: Vec::new(),
            }),
            ExpressionKind::Group(inner) => Self::resolve(inner, session),
            ExpressionKind::Member {
                target,
                property,
                property_span,
            } => {
                let mut path: Self = Self::resolve(target, session)?;
                path.members.push(AssignmentSegment::Member {
                    name: String::from(*property),
                    span: *property_span,
                });
                Ok(path)
            }
            ExpressionKind::Index { target, index } => {
                let mut path: Self = Self::resolve(target, session)?;
                let index_value: Value = evaluate_in(index, session)?;
                let key: TextValue = index_property(&index_value, index.span)?;
                path.members.push(AssignmentSegment::Index {
                    key,
                    span: index.span,
                });
                Ok(path)
            }
            _ => Err(EvalError::UnsupportedExpression(target.span)),
        }
    }

    pub(super) fn read_root(&self, session: &EvaluationSession<'_>) -> Result<Value, EvalError> {
        match &self.root {
            AssignmentRoot::Global(name) => session
                .context
                .global(name)
                .cloned()
                .ok_or(EvalError::UnknownGlobal(self.root_span)),
            AssignmentRoot::Variable(scope, name) => Ok(session
                .context
                .variable(*scope, name)
                .cloned()
                .unwrap_or(Value::Undefined)),
            AssignmentRoot::Setup => session
                .context
                .setup()
                .cloned()
                .ok_or(EvalError::MissingSetup(self.root_span)),
        }
    }

    pub(super) fn read_value(&self, root: &Value) -> Result<Value, EvalError> {
        read_segments(root, &self.members)
    }

    pub(super) fn set_value(&self, root: &mut Value, value: Value) -> Result<(), EvalError> {
        if self.members.is_empty() {
            *root = value;
            return Ok(());
        }

        set_segments(root, &self.members, value)
    }

    /// 成员写入先验证并提交根，Context 接受后才修改共享集合。
    pub(super) fn commit_value(
        &self,
        root: &mut Value,
        value: Value,
        session: &mut EvaluationSession<'_>,
    ) -> Result<(), EvalError> {
        if self.members.is_empty() {
            return self.write_root(value, session);
        }

        validate_segments(root, &self.members)?;
        self.write_root(root.clone(), session)?;
        self.set_value(root, value)
    }

    pub(super) fn write_root(
        &self,
        value: Value,
        session: &mut EvaluationSession<'_>,
    ) -> Result<(), EvalError> {
        let writer: &mut dyn WritableEvaluationContext = session.context.writer(self.root_span)?;
        let written: Result<(), ContextWriteError> = match &self.root {
            AssignmentRoot::Global(name) => writer.set_global(name, value),
            AssignmentRoot::Setup => writer.set_setup(value),
            AssignmentRoot::Variable(scope, name) => writer.set_variable(*scope, name, value),
        };
        written
            .map_err(|ContextWriteError::Rejected| EvalError::ContextWriteRejected(self.root_span))
    }

    /// 删除根绑定或 Object 属性；Array 最终索引保持稠密，不能直接删除。
    pub(super) fn delete(
        &self,
        session: &mut EvaluationSession<'_>,
    ) -> Result<Option<Value>, EvalError> {
        if self.members.is_empty() {
            return self.delete_root(session);
        }

        let mut root: Value = self.read_root(session)?;
        validate_delete_segments(&root, &self.members)?;
        self.write_root(root.clone(), session)?;
        delete_segments(&mut root, &self.members)
    }

    fn delete_root(&self, session: &mut EvaluationSession<'_>) -> Result<Option<Value>, EvalError> {
        let writer: &mut dyn WritableEvaluationContext = session.context.writer(self.root_span)?;
        let deleted: Result<Option<Value>, ContextWriteError> = match &self.root {
            AssignmentRoot::Global(name) => writer.del_global(name),
            AssignmentRoot::Variable(scope, name) => writer.del_variable(*scope, name),
            AssignmentRoot::Setup => return Err(EvalError::InvalidDeleteTarget(self.root_span)),
        };
        deleted
            .map_err(|ContextWriteError::Rejected| EvalError::ContextWriteRejected(self.root_span))
    }
}

fn validate_delete_segments(
    current: &Value,
    segments: &[AssignmentSegment],
) -> Result<(), EvalError> {
    let Some((segment, remaining)) = segments.split_first() else {
        return Ok(());
    };
    let last: bool = remaining.is_empty();
    match (current, segment) {
        (Value::Object(properties), segment) => {
            let (name, span): (String, Span) = object_key(segment)?;
            properties.with(|values: &Vec<(String, Value)>| {
                if last {
                    return Ok(());
                }
                let value: &Value = values
                    .iter()
                    .find_map(|(stored, value): &(String, Value)| {
                        (stored == &name).then_some(value)
                    })
                    .ok_or(EvalError::UnknownMember(span))?;
                validate_delete_segments(value, remaining)
            })
        }
        (Value::Array(items), AssignmentSegment::Index { key, span }) => {
            let position: usize =
                canonical_index(key).ok_or(EvalError::InvalidArrayIndex(*span))?;
            if last {
                return Err(EvalError::InvalidDeleteTarget(*span));
            }
            items.with(|values: &Vec<Value>| {
                let value: &Value = values
                    .get(position)
                    .ok_or(EvalError::InvalidArrayIndex(*span))?;
                validate_delete_segments(value, remaining)
            })
        }
        (_, AssignmentSegment::Member { span, .. }) => Err(EvalError::InvalidObjectTarget(*span)),
        (_, AssignmentSegment::Index { span, .. }) => Err(EvalError::InvalidIndexTarget(*span)),
    }
}

fn validate_segments(current: &Value, segments: &[AssignmentSegment]) -> Result<(), EvalError> {
    let Some((segment, remaining)) = segments.split_first() else {
        return Ok(());
    };
    let last: bool = remaining.is_empty();
    match (current, segment) {
        (Value::Object(properties), segment) => {
            let (name, span): (String, Span) = object_key(segment)?;
            properties.with(|values: &Vec<(String, Value)>| {
                let existing: Option<&Value> =
                    values.iter().find_map(|(stored, value): &(String, Value)| {
                        (stored == &name).then_some(value)
                    });
                if last {
                    Ok(())
                } else {
                    validate_segments(existing.ok_or(EvalError::UnknownMember(span))?, remaining)
                }
            })
        }
        (Value::Array(items), AssignmentSegment::Index { key, span }) => {
            let position: usize =
                canonical_index(key).ok_or(EvalError::InvalidArrayIndex(*span))?;
            items.with(|values: &Vec<Value>| {
                if position > values.len() || (!last && position == values.len()) {
                    return Err(EvalError::InvalidArrayIndex(*span));
                }
                if last {
                    Ok(())
                } else {
                    validate_segments(&values[position], remaining)
                }
            })
        }
        (_, AssignmentSegment::Member { span, .. }) => Err(EvalError::InvalidObjectTarget(*span)),
        (_, AssignmentSegment::Index { span, .. }) => Err(EvalError::InvalidIndexTarget(*span)),
    }
}

fn read_segments(current: &Value, segments: &[AssignmentSegment]) -> Result<Value, EvalError> {
    let Some((segment, remaining)) = segments.split_first() else {
        return Ok(current.clone());
    };
    match (current, segment) {
        (Value::Object(properties), segment) => {
            let (name, span): (String, Span) = object_key(segment)?;
            properties.with(|values: &Vec<(String, Value)>| {
                let value: &Value = values
                    .iter()
                    .find_map(|(stored, value): &(String, Value)| {
                        (stored == &name).then_some(value)
                    })
                    .ok_or(EvalError::UnknownMember(span))?;
                read_segments(value, remaining)
            })
        }
        (Value::Array(items), AssignmentSegment::Index { key, span }) => {
            let position: usize =
                canonical_index(key).ok_or(EvalError::InvalidArrayIndex(*span))?;
            items.with(|values: &Vec<Value>| {
                let value: &Value = values
                    .get(position)
                    .ok_or(EvalError::InvalidArrayIndex(*span))?;
                read_segments(value, remaining)
            })
        }
        (_, AssignmentSegment::Member { span, .. }) => Err(EvalError::InvalidObjectTarget(*span)),
        (_, AssignmentSegment::Index { span, .. }) => Err(EvalError::InvalidIndexTarget(*span)),
    }
}

fn set_segments(
    current: &mut Value,
    segments: &[AssignmentSegment],
    value: Value,
) -> Result<(), EvalError> {
    let Some((segment, remaining)) = segments.split_first() else {
        *current = value;
        return Ok(());
    };
    let last: bool = remaining.is_empty();
    match (current, segment) {
        (Value::Object(properties), segment) => {
            let (name, span): (String, Span) = object_key(segment)?;
            properties.with_mut(|values: &mut Vec<(String, Value)>| {
                let existing: Option<usize> = values
                    .iter()
                    .position(|(stored, _value): &(String, Value)| stored == &name);
                if last {
                    if let Some(position) = existing {
                        values[position].1 = value;
                    } else {
                        values.push((name, value));
                    }
                    return Ok(());
                }
                let position: usize = existing.ok_or(EvalError::UnknownMember(span))?;
                set_segments(&mut values[position].1, remaining, value)
            })
        }
        (Value::Array(items), AssignmentSegment::Index { key, span }) => {
            let position: usize =
                canonical_index(key).ok_or(EvalError::InvalidArrayIndex(*span))?;
            items.with_mut(|values: &mut Vec<Value>| {
                if position > values.len() || (!last && position == values.len()) {
                    return Err(EvalError::InvalidArrayIndex(*span));
                }
                if last {
                    if position == values.len() {
                        values.push(value);
                    } else {
                        values[position] = value;
                    }
                    return Ok(());
                }
                set_segments(&mut values[position], remaining, value)
            })
        }
        (_, AssignmentSegment::Member { span, .. }) => Err(EvalError::InvalidObjectTarget(*span)),
        (_, AssignmentSegment::Index { span, .. }) => Err(EvalError::InvalidIndexTarget(*span)),
    }
}

fn delete_segments(
    current: &mut Value,
    segments: &[AssignmentSegment],
) -> Result<Option<Value>, EvalError> {
    let (segment, remaining): (&AssignmentSegment, &[AssignmentSegment]) =
        segments.split_first().expect("成员删除只在非空路径上执行");
    let last: bool = remaining.is_empty();
    match (current, segment) {
        (Value::Object(properties), segment) => {
            let (name, span): (String, Span) = object_key(segment)?;
            properties.with_mut(|values: &mut Vec<(String, Value)>| {
                let position: Option<usize> = values
                    .iter()
                    .position(|(stored, _value): &(String, Value)| stored == &name);
                if last {
                    return Ok(position.map(|position: usize| values.remove(position).1));
                }
                let position: usize = position.ok_or(EvalError::UnknownMember(span))?;
                delete_segments(&mut values[position].1, remaining)
            })
        }
        (Value::Array(items), AssignmentSegment::Index { key, span }) => {
            let position: usize =
                canonical_index(key).ok_or(EvalError::InvalidArrayIndex(*span))?;
            if last {
                return Err(EvalError::InvalidDeleteTarget(*span));
            }
            items.with_mut(|values: &mut Vec<Value>| {
                let value: &mut Value = values
                    .get_mut(position)
                    .ok_or(EvalError::InvalidArrayIndex(*span))?;
                delete_segments(value, remaining)
            })
        }
        (_, AssignmentSegment::Member { span, .. }) => Err(EvalError::InvalidObjectTarget(*span)),
        (_, AssignmentSegment::Index { span, .. }) => Err(EvalError::InvalidIndexTarget(*span)),
    }
}

fn object_key(segment: &AssignmentSegment) -> Result<(String, Span), EvalError> {
    match segment {
        AssignmentSegment::Member { name, span } => Ok((name.clone(), *span)),
        AssignmentSegment::Index { key, span } => Ok((property_name(key, *span)?, *span)),
    }
}

//! Expression Evaluator 的共享测试上下文与分组入口。

use crate::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::expression::evaluator::{
    ContextWriteError, EvalError, EvaluationContext, RandomSource, WritableEvaluationContext,
    delete_with_mut, evaluate, evaluate_with, evaluate_with_mut, evaluate_with_random,
};
use crate::expression::{
    Expression, Span, VariableScope, parse,
    value::{TextValue, Value},
};

mod assignments;
mod builtins;
mod chains_and_comparisons;
mod collection_methods;
mod collection_mutation;
mod context_and_random;
mod values_and_operators;

struct SingleGlobalContext {
    name: String,
    value: Value,
}

impl EvaluationContext for SingleGlobalContext {
    fn global(&self, name: &str) -> Option<&Value> {
        (self.name == name).then_some(&self.value)
    }
}

struct ScopedContext {
    setup: Value,
    variables: Value,
    temporary: Value,
    local: Value,
}

struct FixedRandomSource {
    values: std::vec::IntoIter<f64>,
}

struct WritableContext {
    global: Option<(String, Value)>,
    variables: Vec<(VariableScope, String, Value)>,
}

struct WritableSetupContext {
    setup: Value,
}

struct RejectingGlobalContext {
    name: String,
    value: Value,
}

impl EvaluationContext for RejectingGlobalContext {
    fn global(&self, name: &str) -> Option<&Value> {
        (self.name == name).then_some(&self.value)
    }
}

impl WritableEvaluationContext for RejectingGlobalContext {
    fn set_global(&mut self, _name: &str, _value: Value) -> Result<(), ContextWriteError> {
        Err(ContextWriteError::Rejected)
    }

    fn set_variable(
        &mut self,
        _scope: VariableScope,
        _name: &str,
        _value: Value,
    ) -> Result<(), ContextWriteError> {
        Err(ContextWriteError::Rejected)
    }
}

impl EvaluationContext for WritableSetupContext {
    fn global(&self, _name: &str) -> Option<&Value> {
        None
    }

    fn setup(&self) -> Option<&Value> {
        Some(&self.setup)
    }
}

impl WritableEvaluationContext for WritableSetupContext {
    fn set_global(&mut self, _name: &str, _value: Value) -> Result<(), ContextWriteError> {
        Err(ContextWriteError::Rejected)
    }

    fn set_variable(
        &mut self,
        _scope: VariableScope,
        _name: &str,
        _value: Value,
    ) -> Result<(), ContextWriteError> {
        Err(ContextWriteError::Rejected)
    }

    fn set_setup(&mut self, value: Value) -> Result<(), ContextWriteError> {
        self.setup = value;
        Ok(())
    }
}

impl EvaluationContext for WritableContext {
    fn global(&self, name: &str) -> Option<&Value> {
        self.global
            .as_ref()
            .and_then(|(stored, value): &(String, Value)| (stored == name).then_some(value))
    }

    fn variable(&self, scope: VariableScope, name: &str) -> Option<&Value> {
        self.variables.iter().find_map(
            |(stored_scope, stored_name, value): &(VariableScope, String, Value)| {
                (*stored_scope == scope && stored_name == name).then_some(value)
            },
        )
    }
}

impl WritableEvaluationContext for WritableContext {
    fn authorize_reference_write(&mut self) -> Result<(), ContextWriteError> {
        Ok(())
    }

    fn set_global(&mut self, name: &str, value: Value) -> Result<(), ContextWriteError> {
        self.global = Some((String::from(name), value));
        Ok(())
    }

    fn set_variable(
        &mut self,
        scope: VariableScope,
        name: &str,
        value: Value,
    ) -> Result<(), ContextWriteError> {
        if let Some((_, _, stored)) = self.variables.iter_mut().find(
            |(stored_scope, stored_name, _): &&mut (VariableScope, String, Value)| {
                *stored_scope == scope && stored_name == name
            },
        ) {
            *stored = value;
        } else {
            self.variables.push((scope, String::from(name), value));
        }
        Ok(())
    }

    fn del_global(&mut self, name: &str) -> Result<Option<Value>, ContextWriteError> {
        if self
            .global
            .as_ref()
            .is_some_and(|(stored, _value)| stored == name)
        {
            return Ok(self.global.take().map(|(_name, value)| value));
        }
        Ok(None)
    }

    fn del_variable(
        &mut self,
        scope: VariableScope,
        name: &str,
    ) -> Result<Option<Value>, ContextWriteError> {
        let position: Option<usize> = self.variables.iter().position(
            |(stored_scope, stored_name, _value): &(VariableScope, String, Value)| {
                *stored_scope == scope && stored_name == name
            },
        );
        Ok(position.map(|position: usize| self.variables.remove(position).2))
    }
}

impl RandomSource for FixedRandomSource {
    fn next_unit(&mut self) -> f64 {
        self.values.next().expect("测试随机序列不应耗尽")
    }
}

impl EvaluationContext for ScopedContext {
    fn global(&self, _name: &str) -> Option<&Value> {
        None
    }

    fn setup(&self) -> Option<&Value> {
        Some(&self.setup)
    }

    fn variable(&self, scope: VariableScope, name: &str) -> Option<&Value> {
        let (expected, value): (&str, &Value) = match scope {
            VariableScope::Variables => ("score", &self.variables),
            VariableScope::Temporary => ("turn", &self.temporary),
            VariableScope::Local => ("index", &self.local),
        };
        (name == expected).then_some(value)
    }
}

//! Script Contract 与具体 ECMAScript 引擎之间的最小 Runtime adapter。

use narrava_loom_core::state::State;

use crate::{EcmaBinding, ScriptError, ScriptMacroOutcome, ScriptPending};

/// RuntimeSession 所需的脚本能力；Boa/Oxc 只是一种实现。
pub trait ScriptAdapter {
    fn has_macro(&self, name: &str) -> Result<bool, ScriptError>;
    fn call_macro(
        &self,
        name: &str,
        arguments: &str,
        state: &mut State,
    ) -> Result<ScriptMacroOutcome, ScriptError>;
    fn resume_macro(
        &self,
        pending: ScriptPending,
        state: &mut State,
    ) -> Result<ScriptMacroOutcome, ScriptError>;
    fn emit_builtin_event(
        &self,
        name: &str,
        payload: &serde_json::Value,
    ) -> Result<u64, ScriptError>;
    fn take_save(&self) -> Result<Option<(String, String)>, ScriptError>;
    fn complete_save(
        &self,
        operation: &str,
        target: &str,
        result: Result<(), &str>,
    ) -> Result<(), ScriptError>;
    fn sync_variables(&self, state: &State) -> Result<(), ScriptError>;
    fn select_locale(&self, locale: &str) -> Result<(), ScriptError>;
}

impl ScriptAdapter for EcmaBinding {
    fn has_macro(&self, name: &str) -> Result<bool, ScriptError> {
        self.has_macro(name)
    }
    fn call_macro(
        &self,
        name: &str,
        arguments: &str,
        state: &mut State,
    ) -> Result<ScriptMacroOutcome, ScriptError> {
        self.call_macro(name, arguments, state)
    }
    fn resume_macro(
        &self,
        pending: ScriptPending,
        state: &mut State,
    ) -> Result<ScriptMacroOutcome, ScriptError> {
        self.resume_macro(pending, state)
    }
    fn emit_builtin_event(
        &self,
        name: &str,
        payload: &serde_json::Value,
    ) -> Result<u64, ScriptError> {
        self.emit_builtin_event(name, payload)
    }
    fn take_save(&self) -> Result<Option<(String, String)>, ScriptError> {
        self.take_save()
    }
    fn complete_save(
        &self,
        operation: &str,
        target: &str,
        result: Result<(), &str>,
    ) -> Result<(), ScriptError> {
        self.complete_save(operation, target, result)
    }
    fn sync_variables(&self, state: &State) -> Result<(), ScriptError> {
        self.sync_variables(state)
    }
    fn select_locale(&self, locale: &str) -> Result<(), ScriptError> {
        self.select_locale(locale)
    }
}

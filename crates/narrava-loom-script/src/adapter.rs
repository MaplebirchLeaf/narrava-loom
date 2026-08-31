//! Script Contract 与具体 ECMAScript 引擎之间的最小 Runtime adapter。

use narrava_loom_core::hir::HirPassage;
use narrava_loom_core::reaction::{ReactionEffect, ReactionRuntimeState};
use narrava_loom_core::state::{State, StateSnapshot};

use crate::{EcmaBinding, QueuedAuthorEvent, ScriptError, ScriptMacroOutcome, ScriptPending};

/// RuntimeSession 所需的脚本能力；Boa/Oxc 只是一种实现。
pub trait ScriptAdapter {
    fn reaction_state(&self) -> Vec<ReactionRuntimeState> {
        Vec::new()
    }
    fn restore_reaction_state(&self, _state: &[ReactionRuntimeState]) -> Result<(), ScriptError> {
        Ok(())
    }
    fn drain_author_events(&self) -> Result<Vec<QueuedAuthorEvent>, ScriptError> {
        Ok(Vec::new())
    }
    fn resolve_queued_event_reactions(
        &self,
        _passage: Option<&HirPassage<'_>>,
        _state: &mut State,
    ) -> Result<Vec<ReactionEffect>, ScriptError> {
        Ok(Vec::new())
    }
    fn resolve_state_reactions(
        &self,
        _passage: Option<&HirPassage<'_>>,
        _command_before: &StateSnapshot,
        _state: &mut State,
    ) -> Result<Vec<ReactionEffect>, ScriptError> {
        Ok(Vec::new())
    }
    fn resolve_lifecycle_reactions(
        &self,
        _passage: &HirPassage<'_>,
        _state: &mut State,
    ) -> Result<Vec<ReactionEffect>, ScriptError> {
        Ok(Vec::new())
    }
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
    fn reaction_state(&self) -> Vec<ReactionRuntimeState> {
        self.reaction_state()
    }
    fn restore_reaction_state(&self, state: &[ReactionRuntimeState]) -> Result<(), ScriptError> {
        self.restore_reaction_state(state)
    }
    fn drain_author_events(&self) -> Result<Vec<QueuedAuthorEvent>, ScriptError> {
        self.drain_author_events()
    }
    fn resolve_queued_event_reactions(
        &self,
        passage: Option<&HirPassage<'_>>,
        state: &mut State,
    ) -> Result<Vec<ReactionEffect>, ScriptError> {
        self.resolve_queued_event_reactions(passage, state)
    }
    fn resolve_state_reactions(
        &self,
        passage: Option<&HirPassage<'_>>,
        command_before: &StateSnapshot,
        state: &mut State,
    ) -> Result<Vec<ReactionEffect>, ScriptError> {
        self.resolve_state_reactions(passage, command_before, state)
    }
    fn resolve_lifecycle_reactions(
        &self,
        passage: &HirPassage<'_>,
        state: &mut State,
    ) -> Result<Vec<ReactionEffect>, ScriptError> {
        self.resolve_lifecycle_reactions(passage, state)
    }
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

//! Macro 创建的延迟 Interaction 所有权。

use std::collections::HashMap;

use crate::{expression::value::Value, hir::HirBodyNode, presentation::InteractionId};

use super::CapturedMacroLocals;

/// 玩家激活后由 Macro Runtime 执行的一次性动作。
#[derive(Debug, PartialEq)]
pub struct MacroInteraction<'hir, 'source> {
    target: String,
    body: &'hir [HirBodyNode<'source>],
    captures: CapturedMacroLocals<Value>,
}

impl<'hir, 'source> MacroInteraction<'hir, 'source> {
    pub fn new(
        target: &str,
        body: &'hir [HirBodyNode<'source>],
        captures: CapturedMacroLocals<Value>,
    ) -> Self {
        Self {
            target: target.to_owned(),
            body,
            captures,
        }
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn body(&self) -> &'hir [HirBodyNode<'source>] {
        self.body
    }

    pub fn into_parts(
        self,
    ) -> (
        String,
        &'hir [HirBodyNode<'source>],
        CapturedMacroLocals<Value>,
    ) {
        (self.target, self.body, self.captures)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroInteractionError {
    Duplicate,
    Missing,
}

/// 当前有效 Presentation 对应的延迟 Macro 动作。
pub struct MacroInteractions<'hir, 'source> {
    entries: HashMap<InteractionId, MacroInteraction<'hir, 'source>>,
}

impl<'hir, 'source> MacroInteractions<'hir, 'source> {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn add(
        &mut self,
        id: InteractionId,
        interaction: MacroInteraction<'hir, 'source>,
    ) -> Result<(), MacroInteractionError> {
        if self.entries.contains_key(&id) {
            return Err(MacroInteractionError::Duplicate);
        }
        let previous: Option<MacroInteraction<'hir, 'source>> =
            self.entries.insert(id, interaction);
        debug_assert!(previous.is_none());
        Ok(())
    }

    pub fn update(
        &mut self,
        id: &InteractionId,
        interaction: MacroInteraction<'hir, 'source>,
    ) -> Result<MacroInteraction<'hir, 'source>, MacroInteractionError> {
        let current: &mut MacroInteraction<'hir, 'source> = self
            .entries
            .get_mut(id)
            .ok_or(MacroInteractionError::Missing)?;
        Ok(std::mem::replace(current, interaction))
    }

    pub fn get(&self, id: &InteractionId) -> Option<&MacroInteraction<'hir, 'source>> {
        self.entries.get(id)
    }

    pub fn has(&self, id: &InteractionId) -> bool {
        self.entries.contains_key(id)
    }

    pub fn del(&mut self, id: &InteractionId) -> Option<MacroInteraction<'hir, 'source>> {
        self.entries.remove(id)
    }

    /// 激活时取走动作，保证同一 Interaction 不会被重复执行。
    pub fn take(&mut self, id: &InteractionId) -> Option<MacroInteraction<'hir, 'source>> {
        self.del(id)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl<'hir, 'source> Default for MacroInteractions<'hir, 'source> {
    fn default() -> Self {
        Self::new()
    }
}

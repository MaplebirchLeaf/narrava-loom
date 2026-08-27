//! Macro 创建的延迟 Interaction 所有权。

use std::collections::HashMap;

use crate::{expression::value::Value, hir::HirBodyNode, protocol::InteractionId};

use super::CapturedMacroLocals;

/// 玩家激活后由 Macro Runtime 执行的一次性动作。
#[derive(Debug, PartialEq)]
pub struct MacroInteraction<'hir, 'source> {
    target: String,
    body: &'hir [HirBodyNode<'source>],
    captures: CapturedMacroLocals<Value>,
}

impl<'hir, 'source> MacroInteraction<'hir, 'source> {
    /// 组合导航目标、延迟正文与显式捕获的局部绑定。
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

    /// 激活后要导航的 Passage 目标。
    pub fn target(&self) -> &str {
        &self.target
    }

    /// 玩家激活后执行的延迟正文。
    pub fn body(&self) -> &'hir [HirBodyNode<'source>] {
        self.body
    }

    /// 一次性取回目标的全部所有权组件。
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

/// 延迟 Interaction 登记操作失败的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroInteractionError {
    /// 相同 Interaction ID 已经登记。
    Duplicate,
    /// 要更新或取走的 ID 不存在。
    Missing,
}

/// 当前有效 Surface 对应的延迟 Macro 动作。
pub struct MacroInteractions<'hir, 'source> {
    entries: HashMap<InteractionId, MacroInteraction<'hir, 'source>>,
}

impl<'hir, 'source> MacroInteractions<'hir, 'source> {
    /// 建立空动作表。
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// 登记新动作；ID 已存在时拒绝并报告 Duplicate。
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

    /// 替换已有动作并返回旧值；ID 不存在时报 Missing。
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

    /// 读取动作的只读引用。
    pub fn get(&self, id: &InteractionId) -> Option<&MacroInteraction<'hir, 'source>> {
        self.entries.get(id)
    }

    /// 判断动作是否已经登记。
    pub fn has(&self, id: &InteractionId) -> bool {
        self.entries.contains_key(id)
    }

    /// 删除动作并在存在时返回被删除的值。
    pub fn del(&mut self, id: &InteractionId) -> Option<MacroInteraction<'hir, 'source>> {
        self.entries.remove(id)
    }

    /// 激活时取走动作，保证同一 Interaction 不会被重复执行。
    pub fn take(&mut self, id: &InteractionId) -> Option<MacroInteraction<'hir, 'source>> {
        self.del(id)
    }

    /// 判断动作表是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 清空全部登记动作。
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl<'hir, 'source> Default for MacroInteractions<'hir, 'source> {
    fn default() -> Self {
        Self::new()
    }
}

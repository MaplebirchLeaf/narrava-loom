//! ECMAScript Binding 可见的 Macro 注册与生命周期控制面。
//!
//! 这里只保存 callable 身份和 Core Macro 定义，不持有 JavaScript 函数对象；实际
//! 函数表始终属于 Binding，因此 Core 的 Script API 仍然可以序列化和回滚自身状态。

use super::*;

/// scripts 注册的 Macro Handler；实际函数仍由 Binding 的函数表拥有。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptMacroHandler {
    callable: ScriptCallable,
}

impl ScriptMacroHandler {
    pub fn new(callable: ScriptCallable) -> Self {
        Self { callable }
    }

    pub fn callable(&self) -> &ScriptCallable {
        &self.callable
    }
}

/// scripts 注册的 before／after Hook 身份。
pub type ScriptMacroHook = ScriptCallable;

/// Script API 使用的 Macro 定义集合。
pub type ScriptMacroDefinitions = MacroDefinitions<MacroDefinition<ScriptMacroHandler>>;

/// Script API 使用的 Macro 生命周期订阅集合。
pub type ScriptMacroHooks = MacroLifecycleSubscriptions<ScriptMacroHook>;

/// scripts 可见的 Macro 增删查改与生命周期控制面。
pub struct ScriptMacroApi<'macro_api> {
    definitions: &'macro_api mut ScriptMacroDefinitions,
    hooks: &'macro_api mut ScriptMacroHooks,
}

impl<'macro_api> ScriptMacroApi<'macro_api> {
    pub fn new(
        definitions: &'macro_api mut ScriptMacroDefinitions,
        hooks: &'macro_api mut ScriptMacroHooks,
    ) -> Self {
        Self { definitions, hooks }
    }

    pub fn add(
        &mut self,
        name: &str,
        definition: MacroDefinition<ScriptMacroHandler>,
    ) -> Option<MacroDefinition<ScriptMacroHandler>> {
        self.definitions.add(name, definition)
    }

    pub fn update(
        &mut self,
        name: &str,
        definition: MacroDefinition<ScriptMacroHandler>,
    ) -> Result<MacroDefinition<ScriptMacroHandler>, MacroDefinitionError> {
        self.definitions.update(name, definition)
    }

    pub fn del(&mut self, name: &str) -> Option<MacroDefinition<ScriptMacroHandler>> {
        self.definitions.del(name)
    }

    pub fn get(&self, name: &str) -> Option<&MacroDefinition<ScriptMacroHandler>> {
        self.definitions.get(name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.definitions.has(name)
    }

    pub fn before(
        &mut self,
        name: &str,
        hook: ScriptMacroHook,
    ) -> Result<MacroLifecycleSubscriptionId, MacroLifecycleSubscriptionError> {
        self.hooks.before(name, hook)
    }

    pub fn after(
        &mut self,
        name: &str,
        hook: ScriptMacroHook,
    ) -> Result<MacroLifecycleSubscriptionId, MacroLifecycleSubscriptionError> {
        self.hooks.after(name, hook)
    }

    pub fn off(&mut self, id: MacroLifecycleSubscriptionId) -> Option<ScriptMacroHook> {
        self.hooks.off(id)
    }
}

/// 建立 scripts 最常用的 Macro 定义，避免 Binding 重复拼装字段。
pub fn script_macro_definition(
    callable: ScriptCallable,
    body_kind: MacroBodyKind,
    argument_kind: MacroArgumentKind,
    execution_kind: MacroExecutionKind,
) -> MacroDefinition<ScriptMacroHandler> {
    MacroDefinition::new(
        body_kind,
        argument_kind,
        execution_kind,
        ScriptMacroHandler::new(callable),
    )
}

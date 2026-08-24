//! Host 无关的脚本源码输入；实际编译与执行由 Script Binding 提供。

use crate::{
    Source, SourceKind, SourceList,
    diagnostic::Diagnostic,
    events::Event,
    expression::{
        VariableScope,
        evaluator::{
            ContextWriteError, EvaluationContext, ScriptCallError, WritableEvaluationContext,
        },
        value::{ScriptCallable, Value},
    },
    i18n::I18nCatalog,
    logger::Logger,
    macro_runtime::{
        MacroArgumentKind, MacroBodyKind, MacroDefinition, MacroDefinitionError, MacroDefinitions,
        MacroExecutionKind, MacroLifecycleSubscriptionError, MacroLifecycleSubscriptionId,
        MacroLifecycleSubscriptions,
    },
    resource::ResourceCatalog,
    state::{GlobalImportReport, State},
    story::Story,
};

/// State 在 VM 求值期间把 ScriptCallable 交还给 Binding 的瞬时路由。
///
/// 路由不进入 State checkpoint、Save 或 Value 图；真实函数对象仍归 Binding。
pub trait ScriptCallDispatcher {
    fn call(
        &self,
        callable: &ScriptCallable,
        arguments: Vec<Value>,
        state: &mut State,
    ) -> Result<Value, ScriptCallError>;
}

/// Binding 为作者侧 `Engine` 单例实现的事务控制契约。
pub trait ScriptEngineHost {
    fn started(&self) -> bool;
    fn goto(&mut self, target: &str) -> Result<(), Diagnostic>;
    fn back(&mut self) -> Result<(), Diagnostic>;
    fn forward(&mut self) -> Result<(), Diagnostic>;
    fn restart(&mut self) -> Result<(), Diagnostic>;
}

/// scripts 可读取的 Passage 元数据快照。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptPassageInfo {
    pub name: String,
    pub tags: Vec<String>,
}

/// Binding 为作者侧只读 `Story` 单例实现的查询契约。
pub trait ScriptStoryHost {
    fn has(&self, name: &str) -> bool;
    fn current(&self) -> Option<ScriptPassageInfo>;
    fn get(&self, name: &str) -> Option<ScriptPassageInfo>;
    fn visits(&self, name: &str) -> usize;
}

/// 直接复用 Core Story 查询语义的作者侧只读门面。
pub struct ScriptStoryApi<'story, 'hir, 'source> {
    story: &'story Story<'hir, 'source>,
}

impl<'story, 'hir, 'source> ScriptStoryApi<'story, 'hir, 'source> {
    pub fn new(story: &'story Story<'hir, 'source>) -> Self {
        Self { story }
    }

    fn passage_info(name: &str, tags: &[&str]) -> ScriptPassageInfo {
        ScriptPassageInfo {
            name: name.to_owned(),
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        }
    }
}

impl ScriptStoryHost for ScriptStoryApi<'_, '_, '_> {
    fn has(&self, name: &str) -> bool {
        self.story.has(name)
    }

    fn current(&self) -> Option<ScriptPassageInfo> {
        self.story
            .current()
            .map(|passage| Self::passage_info(passage.name, &passage.tags))
    }

    fn get(&self, name: &str) -> Option<ScriptPassageInfo> {
        self.story
            .get(name)
            .map(|passage| Self::passage_info(passage.name, &passage.tags))
    }

    fn visits(&self, name: &str) -> usize {
        self.story.visits(name)
    }
}

/// Save 文档由 Core 生成和恢复；实际文件选择与写入仍由 Binding 拥有。
pub trait ScriptSaveHost {
    fn capture_json(&mut self) -> Result<String, Diagnostic>;
    fn restore_json(&mut self, json: &str) -> Result<(), Diagnostic>;
    fn request_export(&mut self) -> Result<(), Diagnostic>;
    fn request_import(&mut self) -> Result<(), Diagnostic>;
}

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

/// Binding 侧真实函数表的最小调用接口。
///
/// 实现可以连接 JavaScript、TypeScript 转译产物或其他 ECMAScript Runtime。
pub trait ScriptFunctionHost {
    fn call(
        &mut self,
        callable: &ScriptCallable,
        arguments: Vec<Value>,
        state: &mut State,
    ) -> Result<Value, ScriptCallError>;
}

/// 把 State 查询写入能力与 Script 函数调用能力组合给 VM。
pub struct ScriptRuntimeContext<'runtime, Host> {
    state: &'runtime mut State,
    host: &'runtime mut Host,
}

impl<'runtime, Host> ScriptRuntimeContext<'runtime, Host> {
    pub fn new(state: &'runtime mut State, host: &'runtime mut Host) -> Self {
        Self { state, host }
    }

    pub fn state(&self) -> &State {
        self.state
    }

    pub fn state_mut(&mut self) -> &mut State {
        self.state
    }
}

impl<Host> EvaluationContext for ScriptRuntimeContext<'_, Host> {
    fn global(&self, name: &str) -> Option<&Value> {
        self.state.global_get(name)
    }

    fn setup(&self) -> Option<&Value> {
        Some(self.state.setup_get())
    }

    fn variable(&self, scope: VariableScope, name: &str) -> Option<&Value> {
        self.state.variable(scope, name)
    }
}

impl<Host: ScriptFunctionHost> WritableEvaluationContext for ScriptRuntimeContext<'_, Host> {
    fn set_global(&mut self, name: &str, value: Value) -> Result<(), ContextWriteError> {
        self.state.set_global(name, value)
    }

    fn set_variable(
        &mut self,
        scope: VariableScope,
        name: &str,
        value: Value,
    ) -> Result<(), ContextWriteError> {
        self.state.set_variable(scope, name, value)
    }

    fn set_setup(&mut self, value: Value) -> Result<(), ContextWriteError> {
        self.state.set_setup(value)
    }

    fn del_global(&mut self, name: &str) -> Result<Option<Value>, ContextWriteError> {
        self.state.del_global(name)
    }

    fn del_variable(
        &mut self,
        scope: VariableScope,
        name: &str,
    ) -> Result<Option<Value>, ContextWriteError> {
        self.state.del_variable(scope, name)
    }

    fn authorize_reference_write(&mut self) -> Result<(), ContextWriteError> {
        self.state.authorize_reference_write()
    }

    fn call_script(
        &mut self,
        callable: &ScriptCallable,
        arguments: Vec<Value>,
    ) -> Result<Value, ScriptCallError> {
        self.host.call(callable, arguments, self.state)
    }
}

/// Script Binding 在启动阶段可以使用的 Core 能力集合。
///
/// 字段保持私有，后续增加 Macro、Logger 等能力时不改变 Binding 方法签名。
pub struct ScriptLoadContext<'core> {
    state: &'core mut State,
    macro_definitions: Option<&'core mut ScriptMacroDefinitions>,
    macro_hooks: Option<&'core mut ScriptMacroHooks>,
    logger: Option<&'core mut Logger>,
    events: Option<&'core mut Event>,
    resources: Option<&'core ResourceCatalog>,
    i18n: Option<ScriptI18nApi<'core>>,
}

impl<'core> ScriptLoadContext<'core> {
    pub fn new(state: &'core mut State) -> Self {
        Self {
            state,
            macro_definitions: None,
            macro_hooks: None,
            logger: None,
            events: None,
            resources: None,
            i18n: None,
        }
    }

    /// 为本次加载显式开放 Macro API；无脚本 Macro 的游戏无需创建这些容器。
    pub fn with_macro(
        mut self,
        definitions: &'core mut ScriptMacroDefinitions,
        hooks: &'core mut ScriptMacroHooks,
    ) -> Self {
        self.macro_definitions = Some(definitions);
        self.macro_hooks = Some(hooks);
        self
    }

    /// 为本次加载显式开放结构化 Logger。
    pub fn with_logger(mut self, logger: &'core mut Logger) -> Self {
        self.logger = Some(logger);
        self
    }

    pub fn with_events(mut self, events: &'core mut Event) -> Self {
        self.events = Some(events);
        self
    }

    pub fn with_resources(mut self, resources: &'core ResourceCatalog) -> Self {
        self.resources = Some(resources);
        self
    }

    pub fn with_i18n(
        mut self,
        catalog: &'core I18nCatalog,
        default_locale: &'core str,
        locale: &'core str,
    ) -> Self {
        self.i18n = Some(ScriptI18nApi::new(catalog, default_locale, locale));
        self
    }

    pub fn state(&mut self) -> &mut State {
        self.state
    }

    pub fn macro_api(&mut self) -> Option<ScriptMacroApi<'_>> {
        let definitions: &mut ScriptMacroDefinitions = self.macro_definitions.as_deref_mut()?;
        let hooks: &mut ScriptMacroHooks = self.macro_hooks.as_deref_mut()?;
        Some(ScriptMacroApi::new(definitions, hooks))
    }

    pub fn logger(&mut self) -> Option<&mut Logger> {
        self.logger.as_deref_mut()
    }

    pub fn events(&mut self) -> Option<&mut Event> {
        self.events.as_deref_mut()
    }

    pub fn resources(&self) -> Option<&ResourceCatalog> {
        self.resources
    }

    pub fn i18n(&self) -> Option<ScriptI18nApi<'_>> {
        self.i18n
    }

    /// 导入一个普通名称；Twee 通过无前缀标识符读取。
    pub fn global_set(&mut self, name: &str, value: Value) -> Option<Value> {
        self.state.global_set(name, value)
    }

    /// 一次导入多个普通名称，适合脚本初始化阶段建立 API 集合。
    pub fn global_extend(
        &mut self,
        values: impl IntoIterator<Item = (String, Value)>,
    ) -> GlobalImportReport {
        self.state.global_extend(values)
    }

    /// 把 Binding 已登记的函数句柄显式暴露给 Twee。
    pub fn global_function(&mut self, name: &str, callable: ScriptCallable) -> Option<Value> {
        self.global_set(name, Value::ScriptCallable(callable))
    }
}

/// Binding 映射为作者侧 `I18n` 单例所需的只读 Core 数据。
#[derive(Clone, Copy)]
pub struct ScriptI18nApi<'core> {
    catalog: &'core I18nCatalog,
    default_locale: &'core str,
    locale: &'core str,
}

impl<'core> ScriptI18nApi<'core> {
    pub fn new(
        catalog: &'core I18nCatalog,
        default_locale: &'core str,
        locale: &'core str,
    ) -> Self {
        Self {
            catalog,
            default_locale,
            locale,
        }
    }

    pub fn catalog(&self) -> &'core I18nCatalog {
        self.catalog
    }

    pub fn default_locale(&self) -> &'core str {
        self.default_locale
    }

    pub fn locale(&self) -> &'core str {
        self.locale
    }
}

/// 由 Tauri、Web 或嵌入式 ECMAScript 环境实现的启动加载边界。
pub trait ScriptBinding {
    type Error;

    fn load(
        &mut self,
        bundle: &ScriptBundle<'_>,
        context: &mut ScriptLoadContext<'_>,
    ) -> Result<(), Self::Error>;
}

/// Binding 选择编译或执行路径所需的 ECMAScript 源码语言。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptLanguage {
    TypeScript,
    JavaScript,
}

/// 一份借用现有 Source 的脚本模块，不复制源码文本。
pub struct ScriptModule<'source> {
    source: &'source Source,
    language: ScriptLanguage,
}

impl ScriptModule<'_> {
    pub fn path(&self) -> &str {
        self.source.path.as_str()
    }

    pub fn language(&self) -> ScriptLanguage {
        self.language
    }

    pub fn source(&self) -> &str {
        &self.source.content
    }
}

/// 按 Source 顺序交给同一个 Script Binding 的模块集合。
#[derive(Default)]
pub struct ScriptBundle<'source> {
    modules: Vec<ScriptModule<'source>>,
}

impl<'source> ScriptBundle<'source> {
    /// Twee 继续进入叙事编译器，只有 `.ts/.js` 进入脚本边界。
    pub fn from_sources(sources: &'source SourceList) -> Self {
        let modules: Vec<ScriptModule<'source>> = sources
            .items
            .iter()
            .filter_map(|source: &Source| {
                let language: ScriptLanguage = match source.kind {
                    SourceKind::TypeScript => ScriptLanguage::TypeScript,
                    SourceKind::JavaScript => ScriptLanguage::JavaScript,
                    SourceKind::Twee => return None,
                };
                Some(ScriptModule { source, language })
            })
            .collect();
        Self { modules }
    }

    pub fn modules(&self) -> &[ScriptModule<'source>] {
        &self.modules
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

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

mod bundle;
mod macro_api;

pub use bundle::*;
pub use macro_api::*;

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
    /// Engine 是否已经启动。
    fn started(&self) -> bool;
    /// 导航到指定 Passage。
    fn goto(&mut self, target: &str) -> Result<(), Diagnostic>;
    /// 回退到历史中的前一个 Passage。
    fn back(&mut self) -> Result<(), Diagnostic>;
    /// 前进到历史中的后一个 Passage。
    fn forward(&mut self) -> Result<(), Diagnostic>;
    /// 重新开始当前游戏。
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
    /// Passage 是否存在。
    fn has(&self, name: &str) -> bool;
    /// 当前已确认的 Passage。
    fn current(&self) -> Option<ScriptPassageInfo>;
    /// 精确查询 Passage 元数据。
    fn get(&self, name: &str) -> Option<ScriptPassageInfo>;
    /// 该 Passage 的历史访问次数。
    fn visits(&self, name: &str) -> usize;
}

/// 直接复用 Core Story 查询语义的作者侧只读门面。
pub struct ScriptStoryApi<'story, 'hir, 'source> {
    story: &'story Story<'hir, 'source>,
}

impl<'story, 'hir, 'source> ScriptStoryApi<'story, 'hir, 'source> {
    /// 借用 Story 建立只读门面。
    pub fn new(story: &'story Story<'hir, 'source>) -> Self {
        Self { story }
    }

    /// 把 HIR Passage 字段转换为脚本可见的快照。
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
    /// 生成当前 Save 文档的 JSON。
    fn capture_json(&mut self) -> Result<String, Diagnostic>;
    /// 从 JSON 恢复 Save 文档。
    fn restore_json(&mut self, json: &str) -> Result<(), Diagnostic>;
    /// 向 Host 请求一次导出。
    fn request_export(&mut self) -> Result<(), Diagnostic>;
    /// 向 Host 请求一次导入。
    fn request_import(&mut self) -> Result<(), Diagnostic>;
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
    /// 组合 State 与 Binding 函数宿主。
    pub fn new(state: &'runtime mut State, host: &'runtime mut Host) -> Self {
        Self { state, host }
    }

    /// 只读访问 State。
    pub fn state(&self) -> &State {
        self.state
    }

    /// 可变访问 State。
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
    /// 建立只带 State 的最小加载上下文。
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

    /// 为本次加载显式开放结构化事件总线。
    pub fn with_events(mut self, events: &'core mut Event) -> Self {
        self.events = Some(events);
        self
    }

    /// 为本次加载显式开放资源目录。
    pub fn with_resources(mut self, resources: &'core ResourceCatalog) -> Self {
        self.resources = Some(resources);
        self
    }

    /// 为本次加载显式开放 I18n 只读 API。
    pub fn with_i18n(
        mut self,
        catalog: &'core I18nCatalog,
        default_locale: &'core str,
        locale: &'core str,
    ) -> Self {
        self.i18n = Some(ScriptI18nApi::new(catalog, default_locale, locale));
        self
    }

    /// 可变访问 State。
    pub fn state(&mut self) -> &mut State {
        self.state
    }

    /// 取 Macro 控制面；未用 `with_macro` 开放时返回 `None`。
    pub fn macro_api(&mut self) -> Option<ScriptMacroApi<'_>> {
        let definitions: &mut ScriptMacroDefinitions = self.macro_definitions.as_deref_mut()?;
        let hooks: &mut ScriptMacroHooks = self.macro_hooks.as_deref_mut()?;
        Some(ScriptMacroApi::new(definitions, hooks))
    }

    /// 取结构化 Logger；未开放时返回 `None`。
    pub fn logger(&mut self) -> Option<&mut Logger> {
        self.logger.as_deref_mut()
    }

    /// 取事件总线；未开放时返回 `None`。
    pub fn events(&mut self) -> Option<&mut Event> {
        self.events.as_deref_mut()
    }

    /// 取资源目录；未开放时返回 `None`。
    pub fn resources(&self) -> Option<&ResourceCatalog> {
        self.resources
    }

    /// 取 I18n 只读 API；未开放时返回 `None`。
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
    /// 绑定目录与默认／当前语言。
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

    /// 当前编译目录。
    pub fn catalog(&self) -> &'core I18nCatalog {
        self.catalog
    }

    /// 游戏默认语言标签。
    pub fn default_locale(&self) -> &'core str {
        self.default_locale
    }

    /// 当前生效语言标签。
    pub fn locale(&self) -> &'core str {
        self.locale
    }
}

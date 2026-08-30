//! Narrava 游戏的 ECMAScript 脚本执行。
//!
//! 宿主无关：本 crate 提供 `EcmaBinding`（Boa 引擎 + Oxc 转译）、State/Resource
//! 桥与 Macro/Save/Event 桥，Host（Tauri/TUI）只需提供脚本源码与资源目录。

use std::{cell::RefCell, error::Error, fmt, path::Path, rc::Rc, time::Duration};

use boa_engine::{Context, JsValue, Source};

/// 官方 Runtime 单个 ECMAScript 循环允许执行的最大迭代次数。
const DEFAULT_SCRIPT_LOOP_LIMIT: u64 = 1_000_000;

/// 建立带明确执行上限的 Boa Context，避免作者脚本永久占住 Runtime Worker。
fn runtime_context(loop_limit: u64) -> Context {
    let mut context = Context::default();
    context
        .runtime_limits_mut()
        .set_loop_iteration_limit(loop_limit);
    context
}

use narrava_loom_core::{
    SourceList,
    expression::{
        evaluator::ScriptCallError,
        value::{ScriptCallable, Value},
    },
    i18n::I18nCatalog,
    resource::ResourceCatalog,
    script::{ScriptBundle, ScriptCallDispatcher, ScriptFunctionHost},
    state::State,
};
use oxc::{
    allocator::Allocator,
    codegen::Codegen,
    parser::Parser,
    semantic::SemanticBuilder,
    span::SourceType,
    transformer::{TransformOptions, Transformer, TypeScriptOptions},
};

mod adapter;
pub mod dispatch;
pub mod protocol_adapter;
mod resource_bridge;
mod session;
mod session_handle;
mod state_bridge;

pub use adapter::ScriptAdapter;
pub use session::{RuntimeServices, RuntimeSession};
pub use session_handle::{RuntimeSessionDriver, RuntimeSessionHandle};

/// ECMAScript 装载、桥接或执行失败。
///
/// Script crate 使用自己的稳定错误边界，不把 Tauri IPC DTO 泄漏给 TUI 或未来 Binding。
/// 具体 Host 在最外层决定如何把 `code` 与 `message` 编码到本平台的错误协议。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    /// 与具体 Host 无关的稳定错误码。
    pub code: String,
    /// 面向开发者的错误说明。
    pub message: String,
}

impl ScriptError {
    /// 构造一条脚本运行时错误。
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for ScriptError {}

impl From<narrava_loom_protocol::HostErrorDto> for ScriptError {
    fn from(error: narrava_loom_protocol::HostErrorDto) -> Self {
        Self {
            code: error.code,
            message: error.message,
        }
    }
}

/// 注入到 Boa 的绑定启动脚本：定义 State、V/T/setup 与其余扁平全局 API，
/// 并把跨语言数据经 `__narrava*` 桥接函数交还给 Rust。
const BOOTSTRAP: &str = r#"
(() => {
  const contract = globalThis.__narravaContract;
  delete globalThis.__narravaContract;
  const functions = new Map();
  let nextFunction = 1;
  const events = [];
  const eventSubscriptions = new Map();
  let nextEventSequence = 1;
  const builtinEvents = new Set(contract.builtinEvents);
  const logs = [];
  const macros = new Map();
  const subscriptions = new Map();
  const saveHooks = new Map();
  let nextSubscription = 1;
  let nextHostOperation = 1;
  const hostOperations = new Map();
  const configuration = { story: { passages: [], current: null, visits: {} }, defaultLocale: "und", locale: "und", i18nExport: "{}" };
  const toHostValue = (name, value) => {
    if (typeof value !== "function") return value;
    const id = nextFunction++;
    functions.set(id, value);
    return { __narravaCallable: id, name };
  };
  const fromHostValue = (value) => value?.__narravaCallable === undefined
    ? value
    : functions.get(value.__narravaCallable);
  const api = (name) => ({
    get: (key) => fromHostValue(__narravaStateGet(name, key)),
    has: (key) => __narravaStateHas(name, key),
    set: (key, value) => fromHostValue(__narravaStateSet(name, key, toHostValue(key, value))),
    del: (key) => fromHostValue(__narravaStateDel(name, key)),
    extend: (values) => {
      let inserted = 0, replaced = 0;
      for (const [key, value] of Object.entries(values)) {
        __narravaStateHas(name, key) ? replaced++ : inserted++;
        __narravaStateSet(name, key, toHostValue(key, value));
      }
      return { inserted, replaced };
    },
  });
  // V/T/setup 只把自然的属性语法映射到活动 Rust State；Proxy 不缓存值，
  // 因此脚本函数每次调用都能看到 Engine、存档恢复或其他脚本写入后的最新状态。
  const propertyApi = (name) => new Proxy(Object.create(null), {
    get: (_target, key) => typeof key === "string"
      ? fromHostValue(__narravaStateGet(name, key))
      : undefined,
    set: (_target, key, value) => {
      if (typeof key !== "string") return false;
      __narravaStateSet(name, key, toHostValue(key, value));
      return true;
    },
    deleteProperty: (_target, key) => {
      if (typeof key !== "string") return false;
      __narravaStateDel(name, key);
      return true;
    },
    has: (_target, key) => typeof key === "string" && __narravaStateHas(name, key),
    ownKeys: () => Object.keys(__narravaStateSnapshot(name)),
    getOwnPropertyDescriptor: (_target, key) => {
      if (typeof key !== "string" || !__narravaStateHas(name, key)) return undefined;
      return {
        configurable: true,
        enumerable: true,
        writable: true,
        value: fromHostValue(__narravaStateGet(name, key)),
      };
    },
  });
  globalThis.State = Object.seal({
    global: api("global"), variables: api("variables"), temporary: api("temporary"),
    setup: {
      get: () => fromHostValue(__narravaStateGet("setup", "")),
      set: (value) => fromHostValue(__narravaStateSet("setup", "", toHostValue("setup", value))),
    },
  });
  globalThis.V = propertyApi("variables");
  globalThis.T = propertyApi("temporary");
  globalThis.setup = propertyApi("setup-properties");
  globalThis.Macro = Object.seal({
    add: (name, value) => { const old = macros.get(name); macros.set(name, value); return old },
    update: (name, value) => { if (!macros.has(name)) throw new Error(`Macro 不存在：${name}`); const old = macros.get(name); macros.set(name, value); return old },
    del: (name) => { const old = macros.get(name); macros.delete(name); return old },
    get: (name) => macros.get(name), has: (name) => macros.has(name),
    before: (name, hook) => { const id = nextSubscription++; subscriptions.set(id, { kind: "before", name, hook }); return id },
    after: (name, hook) => { const id = nextSubscription++; subscriptions.set(id, { kind: "after", name, hook }); return id },
    off: (id) => subscriptions.delete(id),
  });
  const logger = Object.fromEntries(["trace", "debug", "info", "warn", "error"].map(level =>
    [level, (target, message) => logs.push({ level, target, message })]
  ));
  Object.assign(logger, {
    subscribe: () => nextSubscription++, take: () => [], unsubscribe: () => false,
  });
  globalThis.Logger = Object.seal(logger);
  const emitEvent = (name, payload) => {
    const record = { sequence: nextEventSequence++, name, payload };
    events.push(record);
    for (const subscription of eventSubscriptions.values()) {
      if (subscription.name === undefined || subscription.name === name) subscription.pending.push(record);
    }
    return record.sequence;
  };
  globalThis.Event = Object.seal({
    emit: (name, payload = undefined) => {
      if (typeof name !== "string" || name.length === 0 || /\s/u.test(name)) throw new TypeError("Event 名称不能为空或包含空白")
      if (builtinEvents.has(name)) throw new TypeError(`Event 内置名称只能由 Engine 发出：${name}`)
      return emitEvent(name, payload);
    },
    subscribe: (filter = {}) => {
      const id = nextSubscription++;
      eventSubscriptions.set(id, { name: filter.name, pending: [] });
      return id;
    },
    take: (id) => {
      const subscription = eventSubscriptions.get(id);
      if (subscription === undefined) return undefined;
      return subscription.pending.splice(0);
    },
    unsubscribe: (id) => eventSubscriptions.delete(id),
  });
  globalThis.Host = Object.freeze({
    delay: (milliseconds) => {
      if (!Number.isFinite(milliseconds) || milliseconds < 0 || milliseconds > 86_400_000) {
        throw new RangeError("Host.delay 毫秒数必须在 0 到 86400000 之间");
      }
      const id = nextHostOperation++;
      return new Promise(resolve => hostOperations.set(id, {
        id, kind: "delay", milliseconds: Math.trunc(milliseconds), resolve, taken: false,
      }));
    },
  });
  globalThis.Engine = Object.seal({
    started: false, goto: (target) => { globalThis.__narrava.engine = { kind: "goto", target } },
    back: () => { globalThis.__narrava.engine = { kind: "back" } },
    forward: () => { globalThis.__narrava.engine = { kind: "forward" } },
    restart: () => { globalThis.__narrava.engine = { kind: "restart" } },
  });
  globalThis.Story = Object.seal({
    has: (name) => configuration.story.passages.some(passage => passage.name === name),
    current: () => configuration.story.current ?? undefined,
    get: (name) => configuration.story.passages.find(passage => passage.name === name),
    visits: (name) => configuration.story.visits[name] ?? 0,
  });
  const saveBefore = (operation, target) => {
    let nextTarget = target;
    for (const hook of saveHooks.values()) {
      if (hook.stage !== "before" || hook.operation !== operation) continue;
      const rewritten = hook.callback(Object.freeze({ operation, target: nextTarget }));
      if (typeof rewritten === "string") nextTarget = rewritten;
    }
    return nextTarget;
  };
  const saveAfter = (completion) => {
    const frozen = Object.freeze({ ...completion });
    for (const hook of saveHooks.values()) {
      if (hook.stage === "after" && hook.operation === completion.operation) hook.callback(frozen);
    }
  };
  const saveSubscribe = (stage, operation, callback) => {
    if (!["capture", "restore", "export", "import"].includes(operation)) throw new TypeError(`未知 Save 操作：${operation}`);
    if (typeof callback !== "function") throw new TypeError("Save Hook 必须是函数");
    const id = nextSubscription++;
    saveHooks.set(id, { stage, operation, callback });
    return id;
  };
  globalThis.Save = Object.seal({
    capture: () => {
      saveBefore("capture", undefined);
      try {
        const json = JSON.stringify({ variables: __narravaStateSnapshot("variables") });
        saveAfter({ operation: "capture", succeeded: true });
        return json;
      } catch (error) {
        saveAfter({ operation: "capture", succeeded: false, error: String(error) });
        throw error;
      }
    },
    restore: (json) => {
      saveBefore("restore", undefined);
      try {
        __narravaStateReplace("variables", JSON.parse(json).variables ?? {});
        saveAfter({ operation: "restore", succeeded: true });
      } catch (error) {
        saveAfter({ operation: "restore", succeeded: false, error: String(error) });
        throw error;
      }
    },
    export: (target = "manual") => {
      const rewritten = saveBefore("export", target);
      globalThis.__narrava.save = { operation: "export", target: rewritten };
    },
    import: (target = "manual") => {
      const rewritten = saveBefore("import", target);
      globalThis.__narrava.save = { operation: "import", target: rewritten };
    },
    before: (operation, hook) => saveSubscribe("before", operation, hook),
    after: (operation, hook) => saveSubscribe("after", operation, hook),
    off: (id) => saveHooks.delete(id),
  });
  globalThis.Resource = Object.seal({
    paths: () => __narravaResourcePaths(),
    has: (path) => __narravaResourceHas(path),
    pick: (paths) => paths.find(path => __narravaResourceHas(path)),
    info: (path) => __narravaResourceInfo(path),
    read: (path) => { const bytes = __narravaResourceRead(path); return bytes === undefined ? undefined : Uint8Array.from(bytes) },
    text: (path) => __narravaResourceText(path),
  });
  globalThis.I18n = Object.freeze({
    get defaultLocale() { return configuration.defaultLocale },
    get locale() { return configuration.locale },
    export: () => configuration.i18nExport,
  });
  const surfaceNode = (kind, value) => Object.freeze({ __narravaSurface: kind, ...value });
  globalThis.Surface = Object.freeze({
    text: (text, options = {}) => surfaceNode("text", {
      text: String(text),
      key: options.key,
      styles: Object.freeze([...(options.styles ?? [])]),
      color: options.color ?? 0,
      delay: options.delay,
      heading: options.heading,
    }),
    hardBreak: () => surfaceNode("hard-break", {}),
    image: (resource, options = {}) => surfaceNode("image", {
      resource: String(resource), key: options.key,
      alt: options.alt ?? "", caption: options.caption,
    }),
    region: (region, children, options = {}) => surfaceNode("region", {
      region, key: options.key, children: Object.freeze([...children]),
    }),
    component: (capability, version, properties, fallback, options = {}) => surfaceNode("component", {
      capability, version, properties: Object.freeze({ ...properties }),
      children: Object.freeze([...fallback]), key: options.key,
    }),
    action: (label, action, options = {}) => surfaceNode("action", {
      label: String(label), action, role: options.role ?? "default", key: options.key,
    }),
    fragment: (...children) => surfaceNode("fragment", { children: Object.freeze(children) }),
  });
  globalThis.__narrava = {
    engine: null, save: null, events, logs, macros,
    configure(value) { Object.assign(configuration, value) },
    emitBuiltin(name, payload) {
      if (!builtinEvents.has(name)) throw new TypeError(`未知 Event 内置名称：${name}`);
      return emitEvent(name, payload);
    },
    completeSave(completion) { saveAfter(completion) },
    takeSave() { const request = this.save; this.save = null; return request },
    hasMacro(name) { return macros.has(name) },
    invokeMacro(name, call) { return macros.get(name).handler(call) },
    takeHostOperation() {
      const pending = [...hostOperations.values()].filter(operation => !operation.taken);
      if (pending.length !== 1) return pending.length === 0 ? null : { kind: "invalid-count", count: pending.length };
      pending[0].taken = true;
      const { id, kind, milliseconds } = pending[0];
      return { id, kind, milliseconds };
    },
    resolveHostOperation(id) {
      const operation = hostOperations.get(id);
      if (operation === undefined) throw new Error(`Host 异步操作不存在：${id}`);
      hostOperations.delete(id);
      operation.resolve();
    },
    call(id, arguments_) { return functions.get(id)(...arguments_) },
  };
  for (const name of contract.globals) {
    if (globalThis[name] === undefined) throw new Error(`Script Contract 缺少全局：${name}`);
  }
  for (const name of contract.surfaceBuilders) {
    if (typeof globalThis.Surface[name] !== "function") throw new Error(`Script Contract 缺少 Surface builder：${name}`);
  }
})();
"#;

const SCRIPT_CONTRACT_JSON: &str = include_str!("../../../bindings/script-contract.json");

fn bootstrap_source() -> String {
    format!("globalThis.__narravaContract = {SCRIPT_CONTRACT_JSON};\n{BOOTSTRAP}")
}

/// 持有 Boa 引擎上下文的脚本运行时（一次启动一个）。
pub struct EcmaRuntime {
    context: Context,
}

/// 对运行时上下文的借用访问边界：宏、保存、内置事件与脚本函数调用都经由它。
pub struct EcmaBinding {
    runtime: RefCell<EcmaRuntime>,
}

/// 脚本宏挂起等待 Host 操作的凭据（当前只有 `Host.delay`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptPending {
    id: u64,
    delay: Duration,
}

impl ScriptPending {
    /// 建立一个 delay 挂起凭据。Adapter 选择 ID，Runtime 只将其视为不透明值。
    pub fn delay_operation(id: u64, milliseconds: u64) -> Self {
        Self {
            id,
            delay: Duration::from_millis(milliseconds),
        }
    }

    /// Runtime 用来映射 resume/cancel 命令的不透明操作身份。
    pub fn id(&self) -> u64 {
        self.id
    }

    /// 需等待的时长。
    pub fn delay(&self) -> Duration {
        self.delay
    }

    /// Protocol 使用整数毫秒传输等待时长，避免泄漏 Rust `Duration`。
    pub fn milliseconds(&self) -> u64 {
        u64::try_from(self.delay.as_millis()).unwrap_or(u64::MAX)
    }
}

/// 脚本宏的一次调用结果：立即完成或挂起等待 Host 操作。
#[derive(Debug, PartialEq)]
pub enum ScriptMacroOutcome {
    /// 宏已完成并返回 Core 值。
    Complete(Value),
    /// 宏挂起，需在 `ScriptPending::delay` 后由 Host 恢复。
    Pending(ScriptPending),
}

impl EcmaBinding {
    /// 取出脚本登记的 Save 请求（operation, target）；无请求时返回 `None`。
    pub fn take_save(&self) -> Result<Option<(String, String)>, ScriptError> {
        let mut runtime = self.runtime.borrow_mut();
        let value = runtime
            .context
            .eval(Source::from_bytes("JSON.stringify(__narrava.takeSave())"))
            .map_err(|error| script_error("script.save_request", error))?;
        if value.is_undefined() {
            return Ok(None);
        }
        let json = js_string(&value, &mut runtime.context)?;
        let request: serde_json::Value = serde_json::from_str(&json)
            .map_err(|error| ScriptError::new("script.save_request", error.to_string()))?;
        if request.is_null() {
            return Ok(None);
        }
        let operation = request["operation"]
            .as_str()
            .ok_or_else(|| ScriptError::new("script.save_request", "Save operation 无效"))?;
        let target = request["target"]
            .as_str()
            .ok_or_else(|| ScriptError::new("script.save_request", "Save target 无效"))?;
        Ok(Some((operation.to_owned(), target.to_owned())))
    }

    /// 把存档结果回传给脚本的 `Save.after` 钩子。
    pub fn complete_save(
        &self,
        operation: &str,
        target: &str,
        outcome: Result<(), &str>,
    ) -> Result<(), ScriptError> {
        let completion = match outcome {
            Ok(()) => {
                serde_json::json!({ "operation": operation, "target": target, "succeeded": true })
            }
            Err(error) => {
                serde_json::json!({ "operation": operation, "target": target, "succeeded": false, "error": error })
            }
        };
        let expression = format!("__narrava.completeSave({completion})");
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map(|_| ())
            .map_err(|error| script_error("script.save_after", error))
    }

    /// 恢复存档后同步脚本变量视图。
    ///
    /// State API 直接读取活动 Rust State，恢复存档后不再维护或刷新 JS 镜像。
    pub fn sync_variables(&self, _state: &State) -> Result<(), ScriptError> {
        Ok(())
    }

    /// 装载脚本源码与桥接并返回绑定；返回 `Rc` 供 State 的脚本分发器共享。
    pub fn load(
        sources: &SourceList,
        resources: &ResourceCatalog,
        i18n: &I18nCatalog,
        default_locale: &str,
        state: &mut State,
    ) -> Result<Rc<Self>, ScriptError> {
        Ok(Rc::new(Self {
            runtime: RefCell::new(EcmaRuntime::load(
                sources,
                resources,
                i18n,
                default_locale,
                state,
            )?),
        }))
    }

    /// 查询脚本是否注册了指定 Macro。
    pub fn has_macro(&self, name: &str) -> Result<bool, ScriptError> {
        let expression = format!(
            "__narrava.hasMacro({})",
            serde_json::to_string(name).expect("字符串必须可序列化")
        );
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map(|value| value.as_boolean().unwrap_or(false))
            .map_err(|error| script_error("script.macro", error))
    }

    /// 调用脚本 Macro；handler 未决时返回 Pending 等待 Host 操作。
    pub fn call_macro(
        &self,
        name: &str,
        arguments: &str,
        state: &mut State,
    ) -> Result<ScriptMacroOutcome, ScriptError> {
        let call = serde_json::json!({ "name": name, "arguments": arguments });
        let expression = format!(
            "globalThis.__narravaMacroResult = undefined; Promise.resolve(__narrava.invokeMacro({}, {})).then(value => {{ globalThis.__narravaMacroResult = {{ ok: true, value: JSON.stringify(value) }} }}, error => {{ globalThis.__narravaMacroResult = {{ ok: false, value: String(error) }} }});",
            serde_json::to_string(name).expect("字符串必须可序列化"),
            call,
        );
        let mut runtime = self.runtime.borrow_mut();
        state_bridge::with_state(&mut runtime.context, state, |context| {
            context
                .eval(Source::from_bytes(expression.as_bytes()))
                .map_err(|error| script_error("script.macro", error))?;
            context
                .run_jobs()
                .map_err(|error| script_error("script.macro_jobs", error))?;
            macro_outcome(context)
        })
    }

    /// 解决挂起的 Host 操作（如 `Host.delay` 到期）并继续运行宏。
    pub fn resume_macro(
        &self,
        pending: ScriptPending,
        state: &mut State,
    ) -> Result<ScriptMacroOutcome, ScriptError> {
        let expression = format!("__narrava.resolveHostOperation({})", pending.id);
        let mut runtime = self.runtime.borrow_mut();
        state_bridge::with_state(&mut runtime.context, state, |context| {
            context
                .eval(Source::from_bytes(expression.as_bytes()))
                .map_err(|error| script_error("script.host_operation", error))?;
            context
                .run_jobs()
                .map_err(|error| script_error("script.macro_jobs", error))?;
            macro_outcome(context)
        })
    }

    /// 由 Host 把已经进入的 Core 生命周期事实投递给游戏脚本 Event。
    pub fn emit_builtin_event(
        &self,
        name: &str,
        payload: &serde_json::Value,
    ) -> Result<u64, ScriptError> {
        let expression = format!(
            "__narrava.emitBuiltin({}, {})",
            serde_json::to_string(name).expect("事件名必须可序列化"),
            payload
        );
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map_err(|error| script_error("script.event", error))?
            .as_number()
            .and_then(|sequence| {
                (sequence.is_finite() && sequence >= 0.0).then_some(sequence as u64)
            })
            .ok_or_else(|| ScriptError::new("script.event", "Event.emit 没有返回有效序号"))
    }

    /// 同步 Host 已确认的运行语言，使脚本侧 `I18n.locale` 与实际渲染语言一致。
    pub fn select_locale(&self, locale: &str) -> Result<(), ScriptError> {
        let configuration = serde_json::json!({ "locale": locale });
        let expression: String = format!("__narrava.configure({configuration})");
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map_err(|error| script_error("script.i18n_locale", error))?;
        Ok(())
    }
}

impl ScriptCallDispatcher for EcmaBinding {
    /// 供 Core 表达式求值调用脚本函数。
    fn call(
        &self,
        callable: &ScriptCallable,
        arguments: Vec<Value>,
        state: &mut State,
    ) -> Result<Value, ScriptCallError> {
        self.runtime.borrow_mut().call(callable, arguments, state)
    }
}

impl EcmaRuntime {
    /// 安装 State/Resource 桥接、注入启动脚本并执行全部转译后的模块。
    pub fn load(
        sources: &SourceList,
        resources: &ResourceCatalog,
        i18n: &I18nCatalog,
        default_locale: &str,
        state: &mut State,
    ) -> Result<Self, ScriptError> {
        let mut context = runtime_context(DEFAULT_SCRIPT_LOOP_LIMIT);
        state_bridge::install(&mut context)
            .map_err(|error| script_error("script.state_bridge", error))?;
        resource_bridge::install(&mut context, resources.clone())
            .map_err(|error| script_error("script.resource_bridge", error))?;
        let configuration = serde_json::json!({
            "defaultLocale": default_locale,
            "locale": default_locale,
            "i18nExport": serde_json::to_string_pretty(&i18n.template(default_locale))
                .map_err(|error| ScriptError::new("script.i18n_export", error.to_string()))?,
        });
        let configure = format!("__narrava.configure({configuration})");
        let modules = ScriptBundle::from_sources(sources)
            .modules()
            .iter()
            .map(|module| transpile(module.path(), module.source()))
            .collect::<Result<Vec<_>, _>>()?;
        state_bridge::with_state(&mut context, state, |context| {
            let bootstrap: String = bootstrap_source();
            context
                .eval(Source::from_bytes(&bootstrap))
                .map_err(|error| script_error("script.bootstrap", error))?;
            context
                .eval(Source::from_bytes(configure.as_bytes()))
                .map_err(|error| script_error("script.configure", error))?;
            for javascript in &modules {
                context
                    .eval(Source::from_bytes(javascript))
                    .map_err(|error| script_error("script.execute", error))?;
                context
                    .run_jobs()
                    .map_err(|error| script_error("script.jobs", error))?;
            }
            Ok::<(), ScriptError>(())
        })?;
        Ok(Self { context })
    }
}

impl ScriptFunctionHost for EcmaRuntime {
    /// 供 Core 以 JSON 形式调用脚本函数并读回结果。
    fn call(
        &mut self,
        callable: &ScriptCallable,
        arguments: Vec<Value>,
        state: &mut State,
    ) -> Result<Value, ScriptCallError> {
        let arguments = arguments
            .iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ScriptCallError::Failed)?;
        let expression = format!(
            "JSON.stringify(__narrava.call({}, {}))",
            callable.id(),
            serde_json::to_string(&arguments).expect("JSON Value 必须可序列化")
        );
        state_bridge::with_state(&mut self.context, state, |context| {
            let result = context
                .eval(Source::from_bytes(expression.as_bytes()))
                .map_err(|_| ScriptCallError::Failed)?;
            context.run_jobs().map_err(|_| ScriptCallError::Failed)?;
            if result.is_undefined() {
                return Ok(Value::Undefined);
            }
            let json = result
                .to_string(context)
                .map_err(|_| ScriptCallError::Failed)?
                .to_std_string_escaped();
            let value: serde_json::Value =
                serde_json::from_str(&json).map_err(|_| ScriptCallError::Failed)?;
            json_to_value(&value).map_err(|_| ScriptCallError::Failed)
        })
    }
}

/// 构造脚本错误。
fn script_error(code: &str, error: impl std::fmt::Display) -> ScriptError {
    ScriptError::new(code, error.to_string())
}

/// JsValue → Rust 字符串。
fn js_string(value: &JsValue, context: &mut Context) -> Result<String, ScriptError> {
    value
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
        .map_err(|error| script_error("script.value", error))
}

/// 读宏调用结果：已结算取返回值；未结算则把等待的 Host 操作转成 Pending。
fn macro_outcome(context: &mut Context) -> Result<ScriptMacroOutcome, ScriptError> {
    let settled = context
        .eval(Source::from_bytes(
            "globalThis.__narravaMacroResult !== undefined",
        ))
        .map_err(|error| script_error("script.macro", error))?;
    if settled.as_boolean() != Some(true) {
        let operation = context
            .eval(Source::from_bytes(
                "JSON.stringify(__narrava.takeHostOperation())",
            ))
            .map_err(|error| script_error("script.host_operation", error))?;
        if operation.is_undefined() {
            return Err(ScriptError::new(
                "script.macro_unmanaged_promise",
                "Macro 返回了未决 Promise，但没有等待 Host 操作",
            ));
        }
        let operation: serde_json::Value =
            serde_json::from_str(&js_string(&operation, context)?)
                .map_err(|error| ScriptError::new("script.host_operation", error.to_string()))?;
        if operation.is_null() {
            return Err(ScriptError::new(
                "script.macro_unmanaged_promise",
                "Macro 返回了未决 Promise，但没有等待 Host 操作",
            ));
        }
        if operation["kind"] == "invalid-count" {
            return Err(ScriptError::new(
                "script.host_operation_count",
                "一个 Macro 同时只能等待一个 Host 操作",
            ));
        }
        if operation["kind"] != "delay" {
            return Err(ScriptError::new(
                "script.host_operation_kind",
                "Host 返回了未知异步操作",
            ));
        }
        let id: u64 = operation["id"]
            .as_u64()
            .ok_or_else(|| ScriptError::new("script.host_operation", "Host 操作 ID 无效"))?;
        let milliseconds: u64 = operation["milliseconds"]
            .as_u64()
            .ok_or_else(|| ScriptError::new("script.host_operation", "Host.delay 毫秒数无效"))?;
        return Ok(ScriptMacroOutcome::Pending(ScriptPending {
            id,
            delay: Duration::from_millis(milliseconds),
        }));
    }

    let result = context
        .eval(Source::from_bytes("JSON.stringify(__narravaMacroResult)"))
        .map_err(|error| script_error("script.macro", error))?;
    let result: serde_json::Value = serde_json::from_str(&js_string(&result, context)?)
        .map_err(|error| ScriptError::new("script.macro", error.to_string()))?;
    if result["ok"] != true {
        return Err(ScriptError::new(
            "script.macro_rejected",
            result["value"].as_str().unwrap_or("Promise 被拒绝"),
        ));
    }
    let value: Value = match result["value"].as_str() {
        None => Value::Undefined,
        Some(json) => json_to_value(
            &serde_json::from_str(json)
                .map_err(|error| ScriptError::new("script.macro_value", error.to_string()))?,
        )?,
    };
    Ok(ScriptMacroOutcome::Complete(value))
}

/// JSON → Core 值（供 Input 与宏返回值转换）。
pub fn json_to_value(value: &serde_json::Value) -> Result<Value, ScriptError> {
    match value {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(value) => Ok(Value::Boolean(*value)),
        serde_json::Value::Number(value) => value
            .as_f64()
            .map(Value::Number)
            .ok_or_else(|| ScriptError::new("script.value", "数值超出范围")),
        serde_json::Value::String(value) => Ok(Value::string(value.as_str())),
        serde_json::Value::Array(values) => values
            .iter()
            .map(json_to_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::array),
        serde_json::Value::Object(values) => values
            .iter()
            .map(|(name, value)| Ok((name.clone(), json_to_value(value)?)))
            .collect::<Result<Vec<_>, ScriptError>>()
            .map(Value::object),
    }
}

/// Core 值 → JSON；函数/命名空间不可序列化。
fn value_to_json(value: &Value) -> Result<serde_json::Value, ()> {
    match value {
        Value::Undefined | Value::Null => Ok(serde_json::Value::Null),
        Value::Boolean(value) => Ok(serde_json::Value::Bool(*value)),
        Value::Number(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .ok_or(()),
        Value::String(value) => value
            .to_unicode_string()
            .map(serde_json::Value::String)
            .ok_or(()),
        Value::Array(value) => value
            .snapshot()
            .iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        Value::Object(value) => value
            .snapshot()
            .into_iter()
            .map(|(name, value)| Ok((name, value_to_json(&value)?)))
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(serde_json::Value::Object),
        Value::Callable(_) | Value::ScriptCallable(_) | Value::Namespace(_) => Err(()),
    }
}

/// 按扩展名转译脚本：`.js` 原样返回，`.ts` 走 oxc 解析/语义/转换/代码生成。
pub fn transpile(path: &str, source: &str) -> Result<String, ScriptError> {
    if path.ends_with(".js") {
        return Ok(source.to_owned());
    }
    let source_type = SourceType::from_path(path)
        .map_err(|error| ScriptError::new("script.language", error.to_string()))?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(ScriptError::new(
            "script.parse",
            parsed.diagnostics[0].to_string(),
        ));
    }
    let mut program = parsed.program;
    let semantic = SemanticBuilder::new().build(&program);
    if !semantic.diagnostics.is_empty() {
        return Err(ScriptError::new(
            "script.semantic",
            semantic.diagnostics[0].to_string(),
        ));
    }
    let options = TransformOptions {
        typescript: TypeScriptOptions::default(),
        ..TransformOptions::default()
    };
    let transformed = Transformer::new(&allocator, Path::new(path), &options)
        .build_with_scoping(semantic.semantic.into_scoping(), &mut program);
    if !transformed.diagnostics.is_empty() {
        return Err(ScriptError::new(
            "script.transform",
            transformed.diagnostics[0].to_string(),
        ));
    }
    Ok(Codegen::new().build(&program).code)
}

#[cfg(test)]
mod tests;

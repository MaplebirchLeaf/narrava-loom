use std::{cell::RefCell, path::Path, rc::Rc, time::Duration};

use boa_engine::{Context, JsValue, Source};
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

use crate::HostErrorDto;

mod resource_bridge;
mod state_bridge;

const BOOTSTRAP: &str = r#"
(() => {
  const functions = new Map();
  let nextFunction = 1;
  const events = [];
  const eventSubscriptions = new Map();
  let nextEventSequence = 1;
  const builtinEvents = new Set(["passage:init", "passage:start", "passage:render", "passage:display", "passage:end"]);
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
  globalThis.State = Object.seal({
    global: api("global"), variables: api("variables"), temporary: api("temporary"),
    setup: {
      get: () => fromHostValue(__narravaStateGet("setup", "")),
      set: (value) => fromHostValue(__narravaStateSet("setup", "", toHostValue("setup", value))),
    },
  });
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
  const presentationNode = (kind, value) => Object.freeze({ __narravaPresentation: kind, ...value });
  globalThis.Presentation = Object.freeze({
    text: (text, options = {}) => presentationNode("text", {
      text: String(text),
      key: options.key,
      styles: Object.freeze([...(options.styles ?? [])]),
      tone: options.tone ?? "default",
    }),
    image: (resource, options = {}) => presentationNode("image", {
      resource: String(resource), key: options.key,
      alt: options.alt ?? "", caption: options.caption,
    }),
    region: (region, children, options = {}) => presentationNode("region", {
      region, key: options.key, children: Object.freeze([...children]),
    }),
    component: (capability, version, properties, fallback, options = {}) => presentationNode("component", {
      capability, version, properties: Object.freeze({ ...properties }),
      children: Object.freeze([...fallback]), key: options.key,
    }),
    action: (label, action, options = {}) => presentationNode("action", {
      label: String(label), action, role: options.role ?? "default", key: options.key,
    }),
    fragment: (...children) => presentationNode("fragment", { children: Object.freeze(children) }),
  });
  globalThis.narrava = Object.freeze({
    Engine: globalThis.Engine, State: globalThis.State, Macro: globalThis.Macro,
    Story: globalThis.Story, Logger: globalThis.Logger, Event: globalThis.Event,
    Host: globalThis.Host,
    Save: globalThis.Save, Resource: globalThis.Resource, I18n: globalThis.I18n,
    Presentation: globalThis.Presentation,
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
})();
"#;

pub struct EcmaRuntime {
    context: Context,
}

pub struct EcmaBinding {
    runtime: RefCell<EcmaRuntime>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptPending {
    id: u64,
    delay: Duration,
}

impl ScriptPending {
    pub fn delay(&self) -> Duration {
        self.delay
    }
}

#[derive(Debug, PartialEq)]
pub enum ScriptMacroOutcome {
    Complete(Value),
    Pending(ScriptPending),
}

impl EcmaBinding {
    pub fn take_save(&self) -> Result<Option<(String, String)>, HostErrorDto> {
        let mut runtime = self.runtime.borrow_mut();
        let value = runtime
            .context
            .eval(Source::from_bytes("JSON.stringify(__narrava.takeSave())"))
            .map_err(|error| script_error("tauri_host.save_request", error))?;
        if value.is_undefined() {
            return Ok(None);
        }
        let json = js_string(&value, &mut runtime.context)?;
        let request: serde_json::Value = serde_json::from_str(&json)
            .map_err(|error| HostErrorDto::new("tauri_host.save_request", error.to_string()))?;
        if request.is_null() {
            return Ok(None);
        }
        let operation = request["operation"]
            .as_str()
            .ok_or_else(|| HostErrorDto::new("tauri_host.save_request", "Save operation 无效"))?;
        let target = request["target"]
            .as_str()
            .ok_or_else(|| HostErrorDto::new("tauri_host.save_request", "Save target 无效"))?;
        Ok(Some((operation.to_owned(), target.to_owned())))
    }

    pub fn complete_save(
        &self,
        operation: &str,
        target: &str,
        outcome: Result<(), &str>,
    ) -> Result<(), HostErrorDto> {
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
            .map_err(|error| script_error("tauri_host.save_after", error))
    }

    pub fn sync_variables(&self, _state: &State) -> Result<(), HostErrorDto> {
        // State API 直接读取活动 Rust State，恢复存档后不再维护或刷新 JS 镜像。
        Ok(())
    }

    pub fn load(
        sources: &SourceList,
        resources: &ResourceCatalog,
        i18n: &I18nCatalog,
        default_locale: &str,
        state: &mut State,
    ) -> Result<Rc<Self>, HostErrorDto> {
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

    pub fn has_macro(&self, name: &str) -> Result<bool, HostErrorDto> {
        let expression = format!(
            "__narrava.hasMacro({})",
            serde_json::to_string(name).expect("字符串必须可序列化")
        );
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map(|value| value.as_boolean().unwrap_or(false))
            .map_err(|error| script_error("tauri_host.script_macro", error))
    }

    pub fn call_macro(
        &self,
        name: &str,
        arguments: &str,
        state: &mut State,
    ) -> Result<ScriptMacroOutcome, HostErrorDto> {
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
                .map_err(|error| script_error("tauri_host.script_macro", error))?;
            context
                .run_jobs()
                .map_err(|error| script_error("tauri_host.script_macro_jobs", error))?;
            macro_outcome(context)
        })
    }

    pub fn resume_macro(
        &self,
        pending: ScriptPending,
        state: &mut State,
    ) -> Result<ScriptMacroOutcome, HostErrorDto> {
        let expression = format!("__narrava.resolveHostOperation({})", pending.id);
        let mut runtime = self.runtime.borrow_mut();
        state_bridge::with_state(&mut runtime.context, state, |context| {
            context
                .eval(Source::from_bytes(expression.as_bytes()))
                .map_err(|error| script_error("tauri_host.host_operation", error))?;
            context
                .run_jobs()
                .map_err(|error| script_error("tauri_host.script_macro_jobs", error))?;
            macro_outcome(context)
        })
    }

    /// 由 Host 把已经进入的 Core 生命周期事实投递给游戏脚本 Event。
    pub fn emit_builtin_event(
        &self,
        name: &str,
        payload: &serde_json::Value,
    ) -> Result<u64, HostErrorDto> {
        let expression = format!(
            "__narrava.emitBuiltin({}, {})",
            serde_json::to_string(name).expect("事件名必须可序列化"),
            payload
        );
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map_err(|error| script_error("tauri_host.script_event", error))?
            .as_number()
            .and_then(|sequence| {
                (sequence.is_finite() && sequence >= 0.0).then_some(sequence as u64)
            })
            .ok_or_else(|| {
                HostErrorDto::new("tauri_host.script_event", "Event.emit 没有返回有效序号")
            })
    }
}

impl ScriptCallDispatcher for EcmaBinding {
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
    pub fn load(
        sources: &SourceList,
        resources: &ResourceCatalog,
        i18n: &I18nCatalog,
        default_locale: &str,
        state: &mut State,
    ) -> Result<Self, HostErrorDto> {
        let mut context = Context::default();
        state_bridge::install(&mut context)
            .map_err(|error| script_error("tauri_host.script_state_bridge", error))?;
        resource_bridge::install(&mut context, resources.clone())
            .map_err(|error| script_error("tauri_host.script_resource_bridge", error))?;
        let configuration = serde_json::json!({
            "defaultLocale": default_locale,
            "locale": default_locale,
            "i18nExport": serde_json::to_string_pretty(&i18n.template(default_locale))
                .map_err(|error| HostErrorDto::new("tauri_host.i18n_export", error.to_string()))?,
        });
        let configure = format!("__narrava.configure({configuration})");
        let modules = ScriptBundle::from_sources(sources)
            .modules()
            .iter()
            .map(|module| transpile(module.path(), module.source()))
            .collect::<Result<Vec<_>, _>>()?;
        state_bridge::with_state(&mut context, state, |context| {
            context
                .eval(Source::from_bytes(BOOTSTRAP))
                .map_err(|error| script_error("tauri_host.script_bootstrap", error))?;
            context
                .eval(Source::from_bytes(configure.as_bytes()))
                .map_err(|error| script_error("tauri_host.script_configure", error))?;
            for javascript in &modules {
                context
                    .eval(Source::from_bytes(javascript))
                    .map_err(|error| script_error("tauri_host.script_execute", error))?;
                context
                    .run_jobs()
                    .map_err(|error| script_error("tauri_host.script_jobs", error))?;
            }
            Ok::<(), HostErrorDto>(())
        })?;
        Ok(Self { context })
    }
}

impl ScriptFunctionHost for EcmaRuntime {
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

fn script_error(code: &str, error: impl std::fmt::Display) -> HostErrorDto {
    HostErrorDto::new(code, error.to_string())
}

fn js_string(value: &JsValue, context: &mut Context) -> Result<String, HostErrorDto> {
    value
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
        .map_err(|error| script_error("tauri_host.script_value", error))
}

fn macro_outcome(context: &mut Context) -> Result<ScriptMacroOutcome, HostErrorDto> {
    let settled = context
        .eval(Source::from_bytes(
            "globalThis.__narravaMacroResult !== undefined",
        ))
        .map_err(|error| script_error("tauri_host.script_macro", error))?;
    if settled.as_boolean() != Some(true) {
        let operation = context
            .eval(Source::from_bytes(
                "JSON.stringify(__narrava.takeHostOperation())",
            ))
            .map_err(|error| script_error("tauri_host.host_operation", error))?;
        if operation.is_undefined() {
            return Err(HostErrorDto::new(
                "tauri_host.script_macro_unmanaged_promise",
                "Macro 返回了未决 Promise，但没有等待 Host 操作",
            ));
        }
        let operation: serde_json::Value = serde_json::from_str(&js_string(&operation, context)?)
            .map_err(|error| {
            HostErrorDto::new("tauri_host.host_operation", error.to_string())
        })?;
        if operation.is_null() {
            return Err(HostErrorDto::new(
                "tauri_host.script_macro_unmanaged_promise",
                "Macro 返回了未决 Promise，但没有等待 Host 操作",
            ));
        }
        if operation["kind"] == "invalid-count" {
            return Err(HostErrorDto::new(
                "tauri_host.host_operation_count",
                "一个 Macro 同时只能等待一个 Host 操作",
            ));
        }
        if operation["kind"] != "delay" {
            return Err(HostErrorDto::new(
                "tauri_host.host_operation_kind",
                "Host 返回了未知异步操作",
            ));
        }
        let id: u64 = operation["id"]
            .as_u64()
            .ok_or_else(|| HostErrorDto::new("tauri_host.host_operation", "Host 操作 ID 无效"))?;
        let milliseconds: u64 = operation["milliseconds"].as_u64().ok_or_else(|| {
            HostErrorDto::new("tauri_host.host_operation", "Host.delay 毫秒数无效")
        })?;
        return Ok(ScriptMacroOutcome::Pending(ScriptPending {
            id,
            delay: Duration::from_millis(milliseconds),
        }));
    }

    let result = context
        .eval(Source::from_bytes("JSON.stringify(__narravaMacroResult)"))
        .map_err(|error| script_error("tauri_host.script_macro", error))?;
    let result: serde_json::Value = serde_json::from_str(&js_string(&result, context)?)
        .map_err(|error| HostErrorDto::new("tauri_host.script_macro", error.to_string()))?;
    if result["ok"] != true {
        return Err(HostErrorDto::new(
            "tauri_host.script_macro_rejected",
            result["value"].as_str().unwrap_or("Promise 被拒绝"),
        ));
    }
    let value: Value = match result["value"].as_str() {
        None => Value::Undefined,
        Some(json) => json_to_value(&serde_json::from_str(json).map_err(|error| {
            HostErrorDto::new("tauri_host.script_macro_value", error.to_string())
        })?)?,
    };
    Ok(ScriptMacroOutcome::Complete(value))
}

pub(crate) fn json_to_value(value: &serde_json::Value) -> Result<Value, HostErrorDto> {
    match value {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(value) => Ok(Value::Boolean(*value)),
        serde_json::Value::Number(value) => value
            .as_f64()
            .map(Value::Number)
            .ok_or_else(|| HostErrorDto::new("tauri_host.script_value", "数值超出范围")),
        serde_json::Value::String(value) => Ok(Value::string(value.as_str())),
        serde_json::Value::Array(values) => values
            .iter()
            .map(json_to_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::array),
        serde_json::Value::Object(values) => values
            .iter()
            .map(|(name, value)| Ok((name.clone(), json_to_value(value)?)))
            .collect::<Result<Vec<_>, HostErrorDto>>()
            .map(Value::object),
    }
}

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

pub fn transpile(path: &str, source: &str) -> Result<String, HostErrorDto> {
    if path.ends_with(".js") {
        return Ok(source.to_owned());
    }
    let source_type = SourceType::from_path(path)
        .map_err(|error| HostErrorDto::new("tauri_host.script_language", error.to_string()))?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(HostErrorDto::new(
            "tauri_host.script_parse",
            parsed.diagnostics[0].to_string(),
        ));
    }
    let mut program = parsed.program;
    let semantic = SemanticBuilder::new().build(&program);
    if !semantic.diagnostics.is_empty() {
        return Err(HostErrorDto::new(
            "tauri_host.script_semantic",
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
        return Err(HostErrorDto::new(
            "tauri_host.script_transform",
            transformed.diagnostics[0].to_string(),
        ));
    }
    Ok(Codegen::new().build(&program).code)
}

#[cfg(test)]
mod tests;

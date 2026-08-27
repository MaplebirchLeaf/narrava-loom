use boa_engine::{Context, Source};
use narrava_loom_core::{
    SourceList,
    expression::value::Value,
    i18n::I18nCatalog,
    resource::ResourceCatalog,
    script::{ScriptCallDispatcher, ScriptFunctionHost},
    state::State,
};

use super::{BOOTSTRAP, EcmaBinding, EcmaRuntime, ScriptMacroOutcome, state_bridge, transpile};

/// 启动脚本只暴露扁平全局 API，且不存在 `window`/`narrava` 浏览器对象。
#[test]
fn bootstrap_exposes_only_flat_script_globals_without_browser_window() {
    let mut context = Context::default();
    context
        .eval(Source::from_bytes(BOOTSTRAP))
        .expect("绑定启动脚本应可执行");
    let result = context
        .eval(Source::from_bytes(
            r#"
                (() => {
                  const all = Event.subscribe();
                  const quest = Event.subscribe({ name: "quest:completed" });
                  const sequence = Event.emit("quest:completed", { id: 7 });
                  Event.emit("ui:hint", null);
                  return JSON.stringify({
                    windowType: typeof window,
                    narravaType: typeof narrava,
                    sequence,
                    quest: Event.take(quest),
                    all: Event.take(all),
                    drained: Event.take(quest),
                    removed: Event.unsubscribe(quest),
                    missing: Event.take(quest),
                  });
                })()
                "#,
        ))
        .expect("Event 示例应可执行");
    let json = result
        .to_string(&mut context)
        .expect("结果应为字符串")
        .to_std_string_escaped();
    let value: serde_json::Value = serde_json::from_str(&json).expect("结果应为 JSON");

    assert_eq!(value["windowType"], "undefined");
    assert_eq!(value["narravaType"], "undefined");
    assert_eq!(value["sequence"], 1);
    assert_eq!(value["quest"].as_array().unwrap().len(), 1);
    assert_eq!(value["all"].as_array().unwrap().len(), 2);
    assert_eq!(value["drained"].as_array().unwrap().len(), 0);
    assert_eq!(value["removed"], true);
    assert!(value.get("missing").is_none());
}

/// Surface builder 是冻结且 Host 中立的（不依赖 `narrava` 全局）。
#[test]
fn bootstrap_exposes_frozen_host_neutral_surface_builders() {
    let mut context = Context::default();
    context
        .eval(Source::from_bytes(BOOTSTRAP))
        .expect("绑定启动脚本应可执行");
    let result = context
        .eval(Source::from_bytes(
            r#"
              JSON.stringify({
                narravaType: typeof narrava,
                presentationType: typeof Presentation,
                surfaceType: typeof Surface,
                frozen: Object.isFrozen(Surface),
                value: Surface.region("bar", [
                  Surface.text("体力不足", { key: "stamina", styles: ["strong"], color: "warning" }),
                  Surface.image("images/hero.png", { alt: "主角" }),
                ], { key: "status" }),
              })
            "#,
        ))
        .expect("Surface builder 应可执行");
    let json = result
        .to_string(&mut context)
        .unwrap()
        .to_std_string_escaped();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["narravaType"], "undefined");
    assert_eq!(value["presentationType"], "undefined");
    assert_eq!(value["surfaceType"], "object");
    assert_eq!(value["frozen"], true);
    assert_eq!(value["value"]["__narravaSurface"], "region");
    assert_eq!(value["value"]["children"][0]["color"], "warning");
}

/// Save 钩子按注册顺序执行，可改写目标，且 after 等待完成结果。
#[test]
fn bootstrap_save_hooks_preserve_order_rewrite_targets_and_wait_for_completion() {
    let mut context = Context::default();
    state_bridge::install(&mut context).expect("State bridge 应可安装");
    let mut state = State::new();
    let result = state_bridge::with_state(&mut context, &mut state, |context| {
            context
                .eval(Source::from_bytes(BOOTSTRAP))
                .expect("绑定启动脚本应可执行");
            context.eval(Source::from_bytes(
                r#"
                (() => {
                  const stages = [];
                  Save.before("capture", () => stages.push("before:capture"));
                  Save.after("capture", result => stages.push(`after:capture:${result.succeeded}`));
                  Save.before("export", ({ target }) => { stages.push(`before:export:${target}`); return `${target}-backup` });
                  Save.after("export", result => stages.push(`after:export:${result.succeeded}`));
                  Save.capture();
                  Save.export("quick");
                  const beforeCompletion = [...stages];
                  const request = { ...__narrava.save };
                  __narrava.completeSave({ ...request, succeeded: true });
                  return JSON.stringify({ beforeCompletion, afterCompletion: stages, request });
                })()
                "#,
            ))
        })
        .expect("Save Hook 示例应可执行");
    let json = result
        .to_string(&mut context)
        .expect("结果应为字符串")
        .to_std_string_escaped();
    let value: serde_json::Value = serde_json::from_str(&json).expect("结果应为 JSON");

    assert_eq!(
        value["beforeCompletion"],
        serde_json::json!([
            "before:capture",
            "after:capture:true",
            "before:export:quick"
        ])
    );
    assert_eq!(value["request"]["target"], "quick-backup");
    assert_eq!(
        value["afterCompletion"],
        serde_json::json!([
            "before:capture",
            "after:capture:true",
            "before:export:quick",
            "after:export:true"
        ])
    );
}

/// TypeScript 源码被转译为真实 ECMAScript 并由 Boa 执行。
#[test]
fn typescript_is_transformed_and_executed_by_boa() {
    let javascript = transpile(
        "scripts/main.ts",
        "const answer: number = 40 + 2; globalThis.result = answer",
    )
    .expect("TypeScript 应可转译");
    assert!(!javascript.contains(": number"));

    let mut context = Context::default();
    context
        .eval(Source::from_bytes(&javascript))
        .expect("转译结果应为真实 ECMAScript");
    let result = context
        .eval(Source::from_bytes("globalThis.result"))
        .expect("脚本结果应可读取");
    assert_eq!(result.as_number(), Some(42.0));
}

/// 纯 JavaScript 源不经改写原样返回。
#[test]
fn javascript_is_not_rewritten() {
    assert_eq!(
        transpile("scripts/main.js", "globalThis.ready = true").unwrap(),
        "globalThis.ready = true"
    );
}

/// 模块顶层登记的数据与函数进入 Rust State（含 I18n 模板导出）。
#[test]
fn ecma_runtime_imports_data_and_callable_into_core_state() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-tauri-script-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
            root.join("contents/scripts/main.ts"),
            "const answer: number = 42; State.global.extend({ answer, i18nTemplate: I18n.export(), twice: (value: number) => value * 2 })",
        )
        .unwrap();
    let sources = SourceList::discover(&root).unwrap();
    let mut state = State::new();
    let resources = ResourceCatalog::default();
    let mut runtime = EcmaRuntime::load(
        &sources,
        &resources,
        &I18nCatalog::default(),
        "zh-CN",
        &mut state,
    )
    .expect("脚本应真实加载");
    assert_eq!(state.global_get("answer"), Some(&Value::Number(42.0)));
    let Value::String(template) = state
        .global_get("i18nTemplate")
        .expect("I18n.export 应导入模板字符串")
    else {
        panic!("I18n.export 应返回 string")
    };
    let template: serde_json::Value =
        serde_json::from_str(&template.to_unicode_string().unwrap()).expect("模板应为 JSON");
    assert_eq!(template["language"], "zh-CN");
    assert!(template["passages"].is_object());
    let Value::ScriptCallable(callable) = state.global_get("twice").cloned().expect("函数应导入")
    else {
        panic!("twice 应为 ScriptCallable")
    };

    assert_eq!(
        runtime
            .call(&callable, vec![Value::Number(6.0)], &mut state)
            .unwrap(),
        Value::Number(12.0)
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// 脚本函数读取的是活动 Rust State，无需 JS 侧镜像。
#[test]
fn script_reads_the_current_rust_state_without_a_javascript_mirror() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-tauri-authoritative-state-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
        root.join("contents/scripts/main.js"),
        "State.global.set('readHealth', () => State.variables.get('health'))",
    )
    .unwrap();
    let sources = SourceList::discover(&root).unwrap();
    let mut state = State::new();
    let mut runtime = EcmaRuntime::load(
        &sources,
        &ResourceCatalog::default(),
        &I18nCatalog::default(),
        "zh-CN",
        &mut state,
    )
    .expect("脚本应真实加载");
    let Value::ScriptCallable(callable) = state
        .global_get("readHealth")
        .cloned()
        .expect("函数应直接登记到 Rust State")
    else {
        panic!("readHealth 应为 ScriptCallable")
    };

    state.variables_set("health", Value::Number(73.0));

    assert_eq!(
        runtime.call(&callable, Vec::new(), &mut state).unwrap(),
        Value::Number(73.0)
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// Resource 读取由原生桥按需解析，不预载全部字节。
#[test]
fn script_resource_reads_are_resolved_lazily_by_the_native_bridge() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-tauri-lazy-resource-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
            root.join("contents/scripts/main.js"),
            "State.global.set('readGuide', () => ({ info: Resource.info('data/guide.txt'), text: Resource.text('data/guide.txt'), first: Resource.read('data/guide.txt')[0] }))",
        )
        .unwrap();
    let sources = SourceList::discover(&root).unwrap();
    let resources = ResourceCatalog::new([
        narrava_loom_core::resource::ResourceInput::new("data/guide.txt", b"Guide".to_vec()),
        narrava_loom_core::resource::ResourceInput::new("unused.bin", vec![9; 4096]),
    ])
    .unwrap();
    let mut state = State::new();
    let mut runtime = EcmaRuntime::load(
        &sources,
        &resources,
        &I18nCatalog::default(),
        "zh-CN",
        &mut state,
    )
    .expect("Resource bridge 应可加载");
    let Value::ScriptCallable(callable) = state
        .global_get("readGuide")
        .cloned()
        .expect("脚本应导出读取函数")
    else {
        panic!("readGuide 应为 ScriptCallable")
    };

    let Value::Object(result) = runtime.call(&callable, Vec::new(), &mut state).unwrap() else {
        panic!("读取函数应返回对象")
    };
    let values = result.snapshot();
    assert_eq!(
        values
            .iter()
            .find_map(|(name, value)| (name == "text").then_some(value)),
        Some(&Value::string("Guide"))
    );
    assert_eq!(
        values
            .iter()
            .find_map(|(name, value)| (name == "first").then_some(value)),
        Some(&Value::Number(71.0))
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// 异步宏 handler 立即结算时直接返回完成值。
#[test]
fn ecma_binding_resolves_async_macro_handlers() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-tauri-macro-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
            root.join("contents/scripts/main.ts"),
            "Macro.add('answer', { body: 'inline', arguments: 'raw', execution: 'async', handler: async () => 42 })",
        )
        .unwrap();
    let sources = SourceList::discover(&root).unwrap();
    let mut state = State::new();
    let binding = EcmaBinding::load(
        &sources,
        &ResourceCatalog::default(),
        &I18nCatalog::default(),
        "zh-CN",
        &mut state,
    )
    .expect("异步 Macro 脚本应加载");

    assert!(binding.has_macro("answer").unwrap());
    assert_eq!(
        binding.call_macro("answer", "", &mut state).unwrap(),
        ScriptMacroOutcome::Complete(Value::Number(42.0))
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// 未等待 Host 操作而悬空的 Promise 被报告为未管理错误。
#[test]
fn ecma_binding_reports_promises_that_need_external_async_work() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-tauri-pending-macro-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
            root.join("contents/scripts/main.js"),
            "Macro.add('waiting', { body: 'inline', arguments: 'raw', execution: 'async', handler: () => new Promise(() => {}) })",
        )
        .unwrap();
    let sources = SourceList::discover(&root).unwrap();
    let mut state = State::new();
    let binding = EcmaBinding::load(
        &sources,
        &ResourceCatalog::default(),
        &I18nCatalog::default(),
        "zh-CN",
        &mut state,
    )
    .expect("异步 Macro 脚本应加载");

    let error = binding
        .call_macro("waiting", "", &mut state)
        .expect_err("未完成的 Promise 不能被伪装成普通值");
    assert_eq!(error.code, "script.macro_unmanaged_promise");
    assert!(
        !error.code.contains("tauri_host") && !error.code.contains("script.script_"),
        "共享 Script Binding 不得泄漏 Host 身份或重复 script 前缀"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// `Host.delay` 产生 Pending 凭据，恢复后宏继续并返回结果。
#[test]
fn ecma_binding_suspends_and_resumes_host_delay_promises() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-tauri-host-delay-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
        root.join("contents/scripts/main.js"),
        "Macro.add('waiting', { body: 'inline', arguments: 'raw', execution: 'async', handler: async () => { await Host.delay(2); return 'ready' } })",
    )
    .unwrap();
    let sources = SourceList::discover(&root).unwrap();
    let mut state = State::new();
    let binding = EcmaBinding::load(
        &sources,
        &ResourceCatalog::default(),
        &I18nCatalog::default(),
        "zh-CN",
        &mut state,
    )
    .expect("异步 Macro 脚本应加载");

    let ScriptMacroOutcome::Pending(pending) =
        binding.call_macro("waiting", "", &mut state).unwrap()
    else {
        panic!("Host.delay 应暂停 Macro")
    };
    assert_eq!(pending.delay(), std::time::Duration::from_millis(2));
    assert_eq!(
        binding.resume_macro(pending, &mut state).unwrap(),
        ScriptMacroOutcome::Complete(Value::string("ready"))
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// Host 投递的 `passage:init` 内置事件到达脚本订阅。
#[test]
fn host_builtin_passage_event_reaches_a_script_subscription() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-tauri-event-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
            root.join("contents/scripts/main.js"),
            "const passageInit = Event.subscribe({ name: 'passage:init' }); State.global.set('takePassageInit', () => Event.take(passageInit))",
        )
        .unwrap();
    let sources = SourceList::discover(&root).unwrap();
    let mut state = State::new();
    let binding = EcmaBinding::load(
        &sources,
        &ResourceCatalog::default(),
        &I18nCatalog::default(),
        "zh-CN",
        &mut state,
    )
    .expect("事件订阅脚本应加载");
    binding
        .emit_builtin_event(
            "passage:init",
            &serde_json::json!({ "passage": "Start", "tags": [] }),
        )
        .expect("Host 应可投递内置事件");
    let Value::ScriptCallable(callable) = state
        .global_get("takePassageInit")
        .cloned()
        .expect("脚本应导出排空函数")
    else {
        panic!("排空函数应为 ScriptCallable")
    };
    let Value::Array(records) = binding.call(&callable, Vec::new(), &mut state).unwrap() else {
        panic!("Event.take 应返回记录数组")
    };
    assert_eq!(records.len(), 1);
    let Value::Object(record) = records.snapshot().into_iter().next().expect("应有一条事件")
    else {
        panic!("事件记录应为对象")
    };
    assert_eq!(
        record
            .snapshot()
            .into_iter()
            .find_map(|(name, value)| (name == "name").then_some(value)),
        Some(Value::string("passage:init"))
    );
    std::fs::remove_dir_all(root).unwrap();
}

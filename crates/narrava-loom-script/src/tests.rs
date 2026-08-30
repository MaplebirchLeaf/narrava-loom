use boa_engine::{Context, Source};
use narrava_loom_core::{
    SourceList,
    expression::value::Value,
    i18n::I18nCatalog,
    resource::ResourceCatalog,
    script::{ScriptCallDispatcher, ScriptFunctionHost},
    state::State,
};

use super::{
    EcmaBinding, EcmaRuntime, ScriptMacroOutcome, bootstrap_source, runtime_context, state_bridge,
    transpile,
};

#[test]
fn runtime_context_rejects_unbounded_javascript_loops() {
    let mut context = runtime_context(8);
    let error = context
        .eval(Source::from_bytes("while (true) {}"))
        .expect_err("无限循环必须被 Boa 执行预算终止");
    assert!(error.to_string().contains("loop iteration limit 8"));
}

const SCRIPT_CONTRACT: &str = include_str!("../../../bindings/script-contract.json");
const TYPESCRIPT_API: &str = include_str!("../../../bindings/typescript/narrava.d.ts");
const GENERATED_TYPESCRIPT_API: &str =
    include_str!("../../../bindings/typescript/narrava-contract.generated.d.ts");

fn contract_names(field: &str) -> Vec<String> {
    let contract: serde_json::Value = serde_json::from_str(SCRIPT_CONTRACT).unwrap();
    contract[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn typescript_covers_the_canonical_script_contract() {
    for name in contract_names("globals") {
        assert!(
            TYPESCRIPT_API.contains(&format!("const {name}:")),
            "TypeScript: {name}"
        );
    }
    for builder in contract_names("surfaceBuilders") {
        assert!(
            TYPESCRIPT_API.contains(&format!("{builder}(")),
            "TypeScript Surface: {builder}"
        );
    }
    for event in contract_names("builtinEvents") {
        assert!(
            GENERATED_TYPESCRIPT_API.contains(&event),
            "generated TypeScript event: {event}"
        );
    }
}

/// 启动脚本只暴露扁平全局 API，且不存在 `window`/`narrava` 浏览器对象。
#[test]
fn bootstrap_exposes_only_flat_script_globals_without_browser_window() {
    let mut context = Context::default();
    let bootstrap = bootstrap_source();
    context
        .eval(Source::from_bytes(&bootstrap))
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

#[test]
fn bootstrap_drains_only_author_events_for_reaction_resolution() {
    let mut context = Context::default();
    context
        .eval(Source::from_bytes(bootstrap_source()))
        .expect("绑定启动脚本应可执行");
    let result = context
        .eval(Source::from_bytes(
            r#"
              Event.emit("quest:completed", { id: 7 });
              __narrava.emitBuiltin("passage:start", { passage: "Start" });
              JSON.stringify({
                first: __narrava.takeAuthorEvents(),
                second: __narrava.takeAuthorEvents(),
              })
            "#,
        ))
        .expect("内部 Runtime 应能取走作者 Event");
    let json = result
        .to_string(&mut context)
        .unwrap()
        .to_std_string_escaped();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["first"].as_array().unwrap().len(), 1);
    assert_eq!(value["first"][0]["name"], "quest:completed");
    assert_eq!(value["first"][0]["payload"]["id"], 7);
    assert_eq!(value["second"].as_array().unwrap().len(), 0);
}

/// Surface builder 是冻结且 Host 中立的（不依赖 `narrava` 全局）。
#[test]
fn bootstrap_exposes_frozen_host_neutral_surface_builders() {
    let mut context = Context::default();
    let bootstrap = bootstrap_source();
    context
        .eval(Source::from_bytes(&bootstrap))
        .expect("绑定启动脚本应可执行");
    let result = context
        .eval(Source::from_bytes(
            r#"
              JSON.stringify({
                narravaType: typeof narrava,
                presentationType: typeof Presentation,
                surfaceType: typeof Surface,
                frozen: Object.isFrozen(Surface),
                hardBreakHasKey: "key" in Surface.hardBreak({ key: "ignored" }),
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
    assert_eq!(value["hardBreakHasKey"], false);
    assert_eq!(value["value"]["__narravaSurface"], "region");
    assert_eq!(value["value"]["children"][0]["color"], "warning");
}

#[test]
fn reaction_api_registers_native_rules_without_a_javascript_registry_mirror() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-reaction-api-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
        root.join("contents/scripts/main.js"),
        r#"Reaction.add({
          id: "alice.quest.complete",
          event: "quest:completed",
          cond: ({ quest }) => quest === "old_mine",
          emit: { name: "alice:friendship", payload: { stage: "friend" } },
          limit: 2,
          tags: ["character:alice"],
        });"#,
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
    .unwrap();
    let value = runtime
        .context
        .eval(Source::from_bytes(
            r#"JSON.stringify({ before: Reaction.get("alice.quest.complete"), changed: Reaction.disable("alice.quest.complete"), after: Reaction.get("alice.quest.complete") })"#,
        ))
        .unwrap();
    let json = value
        .to_string(&mut runtime.context)
        .unwrap()
        .to_std_string_escaped();
    let result: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(result["before"]["enabled"], true);
    assert_eq!(result["before"]["triggered"], 0);
    assert_eq!(result["before"]["tags"][0], "character:alice");
    assert_eq!(result["changed"], true);
    assert_eq!(result["after"]["enabled"], false);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ecma_binding_exposes_author_events_as_owned_core_values() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-reaction-events-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
        root.join("contents/scripts/main.js"),
        r#"Event.emit("quest:completed", { quest: "old_mine", reward: 500 });"#,
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
    .unwrap();

    let events = binding.take_author_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "quest:completed");
    assert_eq!(
        events[0].payload,
        Value::object(vec![
            (String::from("quest"), Value::string("old_mine")),
            (String::from("reward"), Value::Number(500.0)),
        ])
    );
    assert!(binding.take_author_events().unwrap().is_empty());
}

#[test]
fn ecma_binding_resolves_event_conditions_and_publishes_emitted_events() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-reaction-resolver-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
        root.join("contents/scripts/main.js"),
        r#"
          globalThis.friendshipEvents = Event.subscribe({ name: "alice:friendship" });
          Reaction.add({
            id: "alice.quest.complete",
            event: "quest:completed",
            cond: ({ quest }) => quest === "old_mine" && V.reactions_enabled === true,
            emit: { name: "alice:friendship", payload: { stage: "friend" } },
            limit: 1,
          });
          Reaction.add({
            id: "alice.friendship.notice",
            event: "alice:friendship",
            cond: () => V.friendship_notice_enabled === true,
            widget: '<<friendshipNotice>>',
          });
          Event.emit("quest:completed", { quest: "old_mine" });
        "#,
    )
    .unwrap();
    let sources = SourceList::discover(&root).unwrap();
    let mut state = State::new();
    state.variables_set("reactions_enabled", Value::Boolean(true));
    state.variables_set("friendship_notice_enabled", Value::Boolean(true));
    let binding = EcmaBinding::load(
        &sources,
        &ResourceCatalog::default(),
        &I18nCatalog::default(),
        "zh-CN",
        &mut state,
    )
    .unwrap();

    let resolved = binding.resolve_author_reactions(&mut state).unwrap();
    assert_eq!(resolved.len(), 2);
    assert!(
        binding
            .resolve_author_reactions(&mut state)
            .unwrap()
            .is_empty()
    );
    let mut runtime = binding.runtime.borrow_mut();
    let result = runtime
        .context
        .eval(Source::from_bytes(
            r#"JSON.stringify({
              status: Reaction.get("alice.quest.complete"),
              emitted: Event.take(globalThis.friendshipEvents),
            })"#,
        ))
        .unwrap();
    let json = result
        .to_string(&mut runtime.context)
        .unwrap()
        .to_std_string_escaped();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["status"]["triggered"], 1);
    assert_eq!(value["status"]["enabled"], false);
    assert_eq!(value["emitted"][0]["name"], "alice:friendship");
    assert_eq!(value["emitted"][0]["payload"]["stage"], "friend");
}

#[test]
fn ecma_binding_resolves_committed_state_path_changes() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-reaction-state-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
        root.join("contents/scripts/main.js"),
        r#"Reaction.add({
          id: "alice.friendship",
          state: "$alice.affection",
          cond: ({ before, after }) => before < 50 && after >= 50,
          emit: { name: "alice:friendship", payload: { stage: "friend" } },
          limit: 1,
        });"#,
    )
    .unwrap();
    let sources = SourceList::discover(&root).unwrap();
    let mut state = State::new();
    state.variables_set(
        "alice",
        Value::object(vec![(String::from("affection"), Value::Number(40.0))]),
    );
    let binding = EcmaBinding::load(
        &sources,
        &ResourceCatalog::default(),
        &I18nCatalog::default(),
        "zh-CN",
        &mut state,
    )
    .unwrap();
    let before = state.snapshot();
    state.variables_set(
        "alice",
        Value::object(vec![(String::from("affection"), Value::Number(50.0))]),
    );

    let resolved = binding
        .resolve_state_reactions(&before, &mut state)
        .unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(binding.reaction_state()[0].triggered, 1);
}

/// Save 钩子按注册顺序执行，可改写目标，且 after 等待完成结果。
#[test]
fn bootstrap_save_hooks_preserve_order_rewrite_targets_and_wait_for_completion() {
    let mut context = Context::default();
    state_bridge::install(&mut context).expect("State bridge 应可安装");
    let mut state = State::new();
    let result = state_bridge::with_state(&mut context, &mut state, |context| {
            let bootstrap = bootstrap_source();
            context
                .eval(Source::from_bytes(&bootstrap))
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

/// V/T 是 Rust State 的属性代理：赋值、读取、枚举、in 与 delete 均不建立 JS 镜像。
#[test]
fn script_variable_proxies_mutate_the_authoritative_rust_state() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-tauri-variable-proxies-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
        root.join("contents/scripts/main.js"),
        r#"State.global.set("mutateState", () => {
            V.health += 2;
            T.route = "quiet";
            setup.difficulty = "normal";
            const visible = "health" in V && Object.keys(V).includes("health");
            delete V.removed;
            return visible;
        })"#,
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
        .global_get("mutateState")
        .cloned()
        .expect("函数应直接登记到 Rust State")
    else {
        panic!("mutateState 应为 ScriptCallable")
    };
    state.variables_set("health", Value::Number(40.0));
    state.variables_set("removed", Value::Boolean(true));

    assert_eq!(
        runtime.call(&callable, Vec::new(), &mut state).unwrap(),
        Value::Boolean(true)
    );
    assert_eq!(state.variables_get("health"), Some(&Value::Number(42.0)));
    assert_eq!(state.temporary_get("route"), Some(&Value::string("quiet")));
    assert!(!state.variables_has("removed"));
    let Value::Object(setup) = state.setup_get() else {
        panic!("setup 应保持为对象")
    };
    assert_eq!(setup.get("difficulty"), Some(Value::string("normal")));
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

#[test]
fn runtime_session_executes_lifecycle_event_state_and_reaction_goto_as_one_chain() {
    use narrava_loom_core::{bytecode::BytecodeProgram, lir::LirProgram, mir::MirStory, twee};
    use narrava_loom_protocol::{RuntimeCommand, RuntimeUpdate};

    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-reaction-session-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::create_dir_all(root.join("contents/story")).unwrap();
    std::fs::write(
        root.join("contents/scripts/main.js"),
        r#"
          Reaction.add({ id: "start.guard", lifecycle: true, passage: "Start", widget: "guard<br>", exit: true });
          Reaction.add({ id: "boot.goto", event: "boot:ready", goto: "Target" });
          Reaction.add({ id: "score.50", state: "$score", cond: ({ before, after }) => before < 50 && after >= 50, include: "StateNotice", replace: "state-notice", once: true });
          Event.emit("boot:ready");
        "#,
    )
    .unwrap();
    std::fs::write(
        root.join("contents/story/main.twee"),
        r#":: Start
forbidden-start-body

:: Target
<<set $score to 50>>target-body<br><<slot "state-notice">><</slot>>

:: StateNotice
state-notice<br>"#,
    )
    .unwrap();
    let sources = SourceList::discover(&root).unwrap();
    let ast = twee::Story::build(&sources.items).unwrap();
    let hir = narrava_loom_core::hir::HirStory::lower(&ast).unwrap();
    let mir = MirStory::lower(&hir).unwrap();
    let lir = LirProgram::lower(&mir).unwrap();
    let bytecode = BytecodeProgram::compile(&lir);
    let mut state = State::new();
    let binding = EcmaBinding::load(
        &sources,
        &ResourceCatalog::default(),
        &I18nCatalog::default(),
        "zh-CN",
        &mut state,
    )
    .unwrap();
    let mut session = crate::RuntimeSession::new(&hir, &bytecode, binding.clone(), state);

    let RuntimeUpdate::Ready { update } = session.execute(RuntimeCommand::Start).unwrap() else {
        panic!("Reaction 导航链应同步完成")
    };
    let json = serde_json::to_string(&update).unwrap();
    assert_eq!(update.current, "Target");
    assert!(json.contains("target-body"));
    assert!(json.contains("state-notice"));
    assert!(!json.contains("guard"));
    assert!(!json.contains("forbidden-start-body"));
    assert_eq!(binding.reaction_state().len(), 3);
    assert!(
        binding
            .reaction_state()
            .iter()
            .any(|state| state.id == "start.guard" && state.triggered == 1)
    );
    assert!(
        binding
            .reaction_state()
            .iter()
            .any(|state| state.id == "score.50" && state.destroyed)
    );
    std::fs::remove_dir_all(root).unwrap();
}

mod runtime_session_state_machine {
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};

    use narrava_loom_core::{
        SourceList,
        bytecode::BytecodeProgram,
        expression::{
            evaluator::ScriptCallError,
            value::{ScriptCallable, Value},
        },
        hir::HirStory,
        lir::LirProgram,
        mir::MirStory,
        script::ScriptCallDispatcher,
        state::State,
        story::Story,
        twee,
    };
    use narrava_loom_protocol::{
        HostErrorDto, HostNodeDto, RuntimeCommand, RuntimeRequest, RuntimeSessionId, RuntimeUpdate,
        SaveOperation,
    };

    use crate::session::RuntimePlatform;
    use crate::{
        RuntimeSession, RuntimeSessionDriver, ScriptAdapter, ScriptError, ScriptMacroOutcome,
        ScriptPending,
    };

    enum ResumeStep {
        Pending(u64),
        Complete,
    }

    struct FakeAdapter {
        calls: RefCell<VecDeque<u64>>,
        resumes: RefCell<VecDeque<ResumeStep>>,
    }

    #[derive(Clone, Default)]
    struct PlatformCalls(Rc<RefCell<Vec<String>>>);

    struct FakePlatform(PlatformCalls);

    struct ImportPlatform {
        observed: Rc<RefCell<Vec<Option<Value>>>>,
    }

    impl<'hir, 'source> RuntimePlatform<'hir, 'source> for ImportPlatform {
        fn prepare_save(
            &mut self,
            operation: SaveOperation,
            _target: &str,
            state: &State,
            _story: &Story<'hir, 'source>,
            _reactions: &[narrava_loom_core::reaction::ReactionRuntimeState],
        ) -> Result<Option<String>, HostErrorDto> {
            if operation == SaveOperation::Export {
                self.observed
                    .borrow_mut()
                    .push(state.variables_get("route").cloned());
            }
            Ok(None)
        }

        fn complete_save(
            &mut self,
            operation: SaveOperation,
            _target: &str,
            _document: Option<String>,
            state: &mut State,
            _story: &mut Story<'hir, 'source>,
        ) -> Result<Option<Vec<narrava_loom_core::reaction::ReactionRuntimeState>>, HostErrorDto>
        {
            if operation == SaveOperation::Import {
                state.variables_set("route", Value::String("imported".into()));
            }
            Ok(None)
        }

        fn select_language(
            &mut self,
            _locale: &str,
        ) -> Result<Option<narrava_loom_core::i18n::I18nRuntimeLanguage>, HostErrorDto> {
            Ok(None)
        }
    }

    struct FailingSyncAdapter {
        remaining_failures: RefCell<usize>,
    }

    impl ScriptCallDispatcher for FailingSyncAdapter {
        fn call(
            &self,
            _callable: &ScriptCallable,
            _arguments: Vec<Value>,
            _state: &mut State,
        ) -> Result<Value, ScriptCallError> {
            Err(ScriptCallError::Unavailable)
        }
    }

    impl ScriptAdapter for FailingSyncAdapter {
        fn has_macro(&self, _name: &str) -> Result<bool, ScriptError> {
            Ok(false)
        }
        fn call_macro(
            &self,
            _name: &str,
            _arguments: &str,
            _state: &mut State,
        ) -> Result<ScriptMacroOutcome, ScriptError> {
            Err(ScriptError::new("test.macro", "unexpected"))
        }
        fn resume_macro(
            &self,
            _pending: ScriptPending,
            _state: &mut State,
        ) -> Result<ScriptMacroOutcome, ScriptError> {
            Err(ScriptError::new("test.resume", "unexpected"))
        }
        fn emit_builtin_event(
            &self,
            _name: &str,
            _payload: &serde_json::Value,
        ) -> Result<u64, ScriptError> {
            Ok(1)
        }
        fn take_save(&self) -> Result<Option<(String, String)>, ScriptError> {
            Ok(None)
        }
        fn complete_save(
            &self,
            _operation: &str,
            _target: &str,
            _result: Result<(), &str>,
        ) -> Result<(), ScriptError> {
            Ok(())
        }
        fn sync_variables(&self, _state: &State) -> Result<(), ScriptError> {
            let mut remaining = self.remaining_failures.borrow_mut();
            if *remaining > 0 {
                *remaining -= 1;
                return Err(ScriptError::new("test.sync", "sync failed"));
            }
            Ok(())
        }
        fn select_locale(&self, _locale: &str) -> Result<(), ScriptError> {
            Ok(())
        }
    }

    impl<'hir, 'source> RuntimePlatform<'hir, 'source> for FakePlatform {
        fn prepare_save(
            &mut self,
            operation: SaveOperation,
            target: &str,
            _state: &State,
            _story: &Story<'hir, 'source>,
            _reactions: &[narrava_loom_core::reaction::ReactionRuntimeState],
        ) -> Result<Option<String>, HostErrorDto> {
            self.0
                .0
                .borrow_mut()
                .push(format!("save:{}:{target}", operation.as_str()));
            Ok(None)
        }

        fn complete_save(
            &mut self,
            _operation: SaveOperation,
            _target: &str,
            _document: Option<String>,
            _state: &mut State,
            _story: &mut Story<'hir, 'source>,
        ) -> Result<Option<Vec<narrava_loom_core::reaction::ReactionRuntimeState>>, HostErrorDto>
        {
            Ok(None)
        }

        fn select_language(
            &mut self,
            locale: &str,
        ) -> Result<Option<narrava_loom_core::i18n::I18nRuntimeLanguage>, HostErrorDto> {
            self.0.0.borrow_mut().push(format!("language:{locale}"));
            Ok(None)
        }
    }

    impl FakeAdapter {
        fn new(calls: impl IntoIterator<Item = u64>, resumes: Vec<ResumeStep>) -> Rc<Self> {
            Rc::new(Self {
                calls: RefCell::new(calls.into_iter().collect()),
                resumes: RefCell::new(resumes.into()),
            })
        }

        fn pending(id: u64) -> ScriptMacroOutcome {
            ScriptMacroOutcome::Pending(ScriptPending::delay_operation(id, 1))
        }
    }

    impl ScriptCallDispatcher for FakeAdapter {
        fn call(
            &self,
            _callable: &ScriptCallable,
            _arguments: Vec<Value>,
            _state: &mut State,
        ) -> Result<Value, ScriptCallError> {
            Err(ScriptCallError::Unavailable)
        }
    }

    impl ScriptAdapter for FakeAdapter {
        fn has_macro(&self, name: &str) -> Result<bool, ScriptError> {
            Ok(name == "wait")
        }

        fn call_macro(
            &self,
            _name: &str,
            _arguments: &str,
            _state: &mut State,
        ) -> Result<ScriptMacroOutcome, ScriptError> {
            self.calls
                .borrow_mut()
                .pop_front()
                .map(Self::pending)
                .ok_or_else(|| ScriptError::new("test.calls", "unexpected macro call"))
        }

        fn resume_macro(
            &self,
            _pending: ScriptPending,
            _state: &mut State,
        ) -> Result<ScriptMacroOutcome, ScriptError> {
            match self.resumes.borrow_mut().pop_front() {
                Some(ResumeStep::Pending(id)) => Ok(Self::pending(id)),
                Some(ResumeStep::Complete) => Ok(ScriptMacroOutcome::Complete(Value::Null)),
                None => Err(ScriptError::new("test.resumes", "unexpected resume")),
            }
        }

        fn emit_builtin_event(
            &self,
            _name: &str,
            _payload: &serde_json::Value,
        ) -> Result<u64, ScriptError> {
            Ok(1)
        }

        fn take_save(&self) -> Result<Option<(String, String)>, ScriptError> {
            Ok(None)
        }

        fn complete_save(
            &self,
            _operation: &str,
            _target: &str,
            _result: Result<(), &str>,
        ) -> Result<(), ScriptError> {
            Ok(())
        }

        fn sync_variables(&self, _state: &State) -> Result<(), ScriptError> {
            Ok(())
        }

        fn select_locale(&self, _locale: &str) -> Result<(), ScriptError> {
            Ok(())
        }
    }

    fn runtime_fixture() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);
        let root = std::path::PathBuf::from(format!(
            "target/test-projects/narrava-loom-runtime-session-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("contents/story")).unwrap();
        std::fs::write(
            root.join("contents/story/runtime.twee"),
            ":: Start\n<<wait>>Main ready.\n\n:: Bar\n<<wait>>Bar ready.\n",
        )
        .unwrap();
        root
    }

    fn with_runtime(
        adapter: Rc<FakeAdapter>,
        test: impl for<'hir, 'source> FnOnce(&mut RuntimeSession<'hir, 'source, FakeAdapter>),
    ) {
        let root = runtime_fixture();
        let sources = SourceList::discover(&root).expect("RuntimeSession fixture should load");
        let ast = twee::Story::build(&sources.items).expect("fixture Twee should compile");
        let hir = HirStory::lower(&ast).expect("fixture should lower to HIR");
        let mir = MirStory::lower(&hir).expect("fixture should lower to MIR");
        let lir = LirProgram::lower(&mir).expect("fixture should lower to LIR");
        let bytecode = BytecodeProgram::compile(&lir);
        let mut runtime = RuntimeSession::new(&hir, &bytecode, adapter, State::new());
        test(&mut runtime);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn with_runtime_platform(
        calls: PlatformCalls,
        test: impl for<'hir, 'source> FnOnce(&mut RuntimeSession<'hir, 'source, FakeAdapter>),
    ) {
        let root = runtime_fixture();
        let sources = SourceList::discover(&root).expect("RuntimeSession fixture should load");
        let ast = twee::Story::build(&sources.items).expect("fixture Twee should compile");
        let hir = HirStory::lower(&ast).expect("fixture should lower to HIR");
        let mir = MirStory::lower(&hir).expect("fixture should lower to MIR");
        let lir = LirProgram::lower(&mir).expect("fixture should lower to LIR");
        let bytecode = BytecodeProgram::compile(&lir);
        let mut runtime = RuntimeSession::with_platform(
            &hir,
            &bytecode,
            FakeAdapter::new([], Vec::new()),
            State::new(),
            Box::new(FakePlatform(calls)),
        );
        test(&mut runtime);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn pending_id(update: RuntimeUpdate) -> u64 {
        let RuntimeUpdate::Pending { operation } = update else {
            panic!("expected pending update")
        };
        operation.id()
    }

    #[test]
    fn opaque_session_handle_rejects_a_request_routed_to_another_session() {
        let root = runtime_fixture();
        let sources = SourceList::discover(&root).expect("RuntimeSession fixture should load");
        let ast = twee::Story::build(&sources.items).expect("fixture Twee should compile");
        let hir = HirStory::lower(&ast).expect("fixture should lower to HIR");
        let mir = MirStory::lower(&hir).expect("fixture should lower to MIR");
        let lir = LirProgram::lower(&mir).expect("fixture should lower to LIR");
        let bytecode = BytecodeProgram::compile(&lir);
        let session = RuntimeSession::new(
            &hir,
            &bytecode,
            FakeAdapter::new([], Vec::new()),
            State::new(),
        );
        let mut handle =
            RuntimeSessionDriver::new(RuntimeSessionId::new("first").unwrap(), session);

        let error = handle
            .dispatch(RuntimeRequest::new(
                RuntimeSessionId::new("second").unwrap(),
                RuntimeCommand::Start,
            ))
            .unwrap_err();
        assert_eq!(error.code, "runtime_session.id_mismatch");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_driver_rejects_an_unknown_runtime_protocol_version() {
        let root = runtime_fixture();
        let sources = SourceList::discover(&root).unwrap();
        let ast = twee::Story::build(&sources.items).unwrap();
        let hir = HirStory::lower(&ast).unwrap();
        let mir = MirStory::lower(&hir).unwrap();
        let lir = LirProgram::lower(&mir).unwrap();
        let bytecode = BytecodeProgram::compile(&lir);
        let session = RuntimeSession::new(
            &hir,
            &bytecode,
            FakeAdapter::new([], Vec::new()),
            State::new(),
        );
        let mut driver = RuntimeSessionDriver::new(RuntimeSessionId::new("main").unwrap(), session);
        let mut request = RuntimeRequest::new(
            RuntimeSessionId::new("main").unwrap(),
            RuntimeCommand::Start,
        );
        request.protocol_version += 1;

        let error = driver.dispatch(request).unwrap_err();
        assert_eq!(error.code, "runtime_session.protocol_version");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn commands_requiring_a_presented_surface_reject_the_unstarted_state() {
        with_runtime(FakeAdapter::new([], Vec::new()), |runtime| {
            let activate = runtime
                .execute(RuntimeCommand::Activate {
                    interaction: String::from("navigation:missing"),
                })
                .unwrap_err();
            assert_eq!(activate.code, "runtime_session.not_started");

            let input = runtime
                .execute(RuntimeCommand::Input {
                    interaction: String::from("input:missing"),
                    value: serde_json::Value::Null,
                })
                .unwrap_err();
            assert_eq!(input.code, "runtime_session.not_started");
        });
    }

    #[test]
    fn save_and_language_commands_are_routed_through_the_runtime_platform() {
        let calls = PlatformCalls::default();
        with_runtime_platform(calls.clone(), |runtime| {
            let pending = runtime
                .execute(RuntimeCommand::Save {
                    operation: SaveOperation::Export,
                    target: String::from("quick"),
                })
                .unwrap();
            let save_id = pending_id(pending);
            assert_eq!(
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation: save_id,
                        result: Some(narrava_loom_protocol::PendingResult::Save { document: None }),
                    })
                    .unwrap(),
                RuntimeUpdate::Applied
            );
            let pending = runtime
                .execute(RuntimeCommand::SelectLanguage {
                    locale: String::from("en"),
                })
                .unwrap();
            let language_id = pending_id(pending);
            assert_eq!(
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation: language_id,
                        result: Some(narrava_loom_protocol::PendingResult::SelectLanguage),
                    })
                    .unwrap(),
                RuntimeUpdate::Applied
            );
        });
        assert_eq!(
            *calls.0.borrow(),
            [
                String::from("save:export:quick"),
                String::from("language:en")
            ]
        );
    }

    #[test]
    fn platform_resume_rejects_a_result_for_another_operation_kind() {
        let calls = PlatformCalls::default();
        with_runtime_platform(calls, |runtime| {
            let pending = runtime
                .execute(RuntimeCommand::SelectLanguage {
                    locale: String::from("en"),
                })
                .unwrap();
            let error = runtime
                .execute(RuntimeCommand::Resume {
                    operation: pending_id(pending),
                    result: Some(narrava_loom_protocol::PendingResult::Save { document: None }),
                })
                .unwrap_err();
            assert_eq!(error.code, "runtime_session.platform_result_mismatch");
        });
    }

    #[test]
    fn failed_script_sync_rolls_back_an_import_before_the_next_command() {
        let root = runtime_fixture();
        let sources = SourceList::discover(&root).unwrap();
        let ast = twee::Story::build(&sources.items).unwrap();
        let hir = HirStory::lower(&ast).unwrap();
        let mir = MirStory::lower(&hir).unwrap();
        let lir = LirProgram::lower(&mir).unwrap();
        let bytecode = BytecodeProgram::compile(&lir);
        let observed = Rc::new(RefCell::new(Vec::new()));
        let adapter = Rc::new(FailingSyncAdapter {
            remaining_failures: RefCell::new(1),
        });
        let mut state = State::new();
        state.variables_set("route", Value::String("original".into()));
        let mut runtime = RuntimeSession::with_platform(
            &hir,
            &bytecode,
            adapter,
            state,
            Box::new(ImportPlatform {
                observed: observed.clone(),
            }),
        );

        let pending = runtime
            .execute(RuntimeCommand::Save {
                operation: SaveOperation::Import,
                target: String::from("quick"),
            })
            .unwrap();
        let error = runtime
            .execute(RuntimeCommand::Resume {
                operation: pending_id(pending),
                result: Some(narrava_loom_protocol::PendingResult::Save {
                    document: Some(String::from("{}")),
                }),
            })
            .unwrap_err();
        assert_eq!(error.code, "test.sync");
        let pending = runtime
            .execute(RuntimeCommand::Save {
                operation: SaveOperation::Export,
                target: String::from("inspect"),
            })
            .unwrap();
        runtime
            .execute(RuntimeCommand::Resume {
                operation: pending_id(pending),
                result: Some(narrava_loom_protocol::PendingResult::Save { document: None }),
            })
            .unwrap();
        assert_eq!(
            observed.borrow().as_slice(),
            [Some(Value::String("original".into()))]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_session_cannot_start_again_after_presenting_its_first_frame() {
        with_runtime(
            FakeAdapter::new([11, 22], vec![ResumeStep::Complete, ResumeStep::Complete]),
            |runtime| {
                assert_eq!(
                    pending_id(runtime.execute(RuntimeCommand::Start).unwrap()),
                    11
                );
                assert_eq!(
                    pending_id(
                        runtime
                            .execute(RuntimeCommand::Resume {
                                operation: 11,
                                result: None
                            })
                            .unwrap()
                    ),
                    22
                );
                let RuntimeUpdate::Ready { .. } = runtime
                    .execute(RuntimeCommand::Resume {
                        operation: 22,
                        result: None,
                    })
                    .unwrap()
                else {
                    panic!("the fixture should present its first frame")
                };

                let error = runtime.execute(RuntimeCommand::Start).unwrap_err();
                assert_eq!(error.code, "runtime_session.already_started");
            },
        );
    }

    #[test]
    fn pending_rejects_other_commands_preserves_mismatch_and_can_be_cancelled() {
        with_runtime(
            FakeAdapter::new([11], vec![ResumeStep::Complete]),
            |runtime| {
                assert_eq!(
                    pending_id(runtime.execute(RuntimeCommand::Start).unwrap()),
                    11
                );

                let busy = runtime.execute(RuntimeCommand::Start).unwrap_err();
                assert_eq!(busy.code, "runtime_session.pending");

                let mismatch = runtime
                    .execute(RuntimeCommand::Resume {
                        operation: 99,
                        result: None,
                    })
                    .unwrap_err();
                assert_eq!(mismatch.code, "runtime_session.operation_mismatch");

                assert_eq!(
                    runtime
                        .execute(RuntimeCommand::Cancel { operation: 11 })
                        .unwrap(),
                    RuntimeUpdate::Applied
                );
                let gone = runtime
                    .execute(RuntimeCommand::Resume {
                        operation: 11,
                        result: None,
                    })
                    .unwrap_err();
                assert_eq!(gone.code, "runtime_session.unknown_operation");
            },
        );
    }

    #[test]
    fn resume_can_pending_again_then_continue_through_a_special_region() {
        with_runtime(
            FakeAdapter::new(
                [11, 33],
                vec![
                    ResumeStep::Pending(22),
                    ResumeStep::Complete,
                    ResumeStep::Complete,
                ],
            ),
            |runtime| {
                assert_eq!(
                    pending_id(runtime.execute(RuntimeCommand::Start).unwrap()),
                    11
                );
                assert_eq!(
                    pending_id(
                        runtime
                            .execute(RuntimeCommand::Resume {
                                operation: 11,
                                result: None
                            })
                            .unwrap(),
                    ),
                    22
                );
                assert_eq!(
                    pending_id(
                        runtime
                            .execute(RuntimeCommand::Resume {
                                operation: 22,
                                result: None
                            })
                            .unwrap(),
                    ),
                    33
                );

                let RuntimeUpdate::Ready { update } = runtime
                    .execute(RuntimeCommand::Resume {
                        operation: 33,
                        result: None,
                    })
                    .unwrap()
                else {
                    panic!("special region should finish with a ready update")
                };
                assert_eq!(update.current, "Start");
                assert!(update.nodes.iter().any(|node| {
                    matches!(node, HostNodeDto::Region { region, .. } if region == "bar")
                }));
            },
        );
    }
}

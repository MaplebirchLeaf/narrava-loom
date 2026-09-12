use boa_engine::{Context, Source};
use narrava_loom_core::{
    SourceList, expression::value::Value, i18n::I18nCatalog, resource::ResourceCatalog,
    script::ScriptCallDispatcher, state::State,
};

use super::{
    EcmaBinding, EcmaRuntime, ScriptMacroOutcome, bootstrap_source, runtime_context, state_adapter,
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
                  Surface.image("hero.png", { alt: "主角" }),
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
fn i18n_select_queues_a_host_language_request() {
    let mut context = Context::default();
    let bootstrap = bootstrap_source();
    context
        .eval(Source::from_bytes(&bootstrap))
        .expect("绑定启动脚本应可执行");
    let result = context
        .eval(Source::from_bytes(
            r#"I18n.select("en"); JSON.stringify(__narrava.takeLanguage())"#,
        ))
        .expect("I18n.select 应建立语言请求");
    let json = result
        .to_string(&mut context)
        .expect("语言请求应为字符串")
        .to_std_string_escaped();

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).expect("语言请求应为 JSON"),
        serde_json::json!({ "locale": "en" })
    );
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
fn reaction_api_rejects_unsupported_regexp_flags_at_the_script_boundary() {
    let root = std::path::PathBuf::from(format!(
        "target/test-projects/narrava-loom-reaction-regexp-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(
        root.join("contents/scripts/main.js"),
        r#"Reaction.add({ id: "invalid.flags", event: "talk", passage: /Hall/g, goto: "Hall" });"#,
    )
    .unwrap();
    let sources = SourceList::discover(&root).unwrap();
    let mut state = State::new();
    let error = match EcmaRuntime::load(
        &sources,
        &ResourceCatalog::default(),
        &I18nCatalog::default(),
        "zh-CN",
        &mut state,
    ) {
        Ok(_) => panic!("不支持的 RegExp flag 不得静默丢弃"),
        Err(error) => error,
    };

    assert_eq!(error.code, "script.execute");
    assert!(error.message.contains("InvalidPassageFlags"));
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

    let events = binding.drain_author_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "quest:completed");
    assert_eq!(
        events[0].payload,
        Value::object(vec![
            (String::from("quest"), Value::string("old_mine")),
            (String::from("reward"), Value::Number(500.0)),
        ])
    );
    assert!(binding.drain_author_events().unwrap().is_empty());
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
            emit: {
              name: "alice:friendship",
              payload: ({ quest }) => ({ stage: "friend", quest, enabled: V.reactions_enabled }),
            },
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

    let resolved = binding
        .resolve_queued_event_reactions(None, &mut state)
        .unwrap();
    assert_eq!(resolved.len(), 2);
    assert!(
        binding
            .resolve_queued_event_reactions(None, &mut state)
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
    assert_eq!(value["emitted"][0]["payload"]["quest"], "old_mine");
    assert_eq!(value["emitted"][0]["payload"]["enabled"], true);
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
        .resolve_state_reactions(None, &before, &mut state)
        .unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(binding.reaction_state()[0].triggered, 1);
}

/// Save 钩子按注册顺序执行，可改写目标，且 after 等待完成结果。
#[test]
fn bootstrap_save_hooks_preserve_order_rewrite_targets_and_wait_for_completion() {
    let mut context = Context::default();
    state_adapter::install(&mut context).expect("State bridge 应可安装");
    let mut state = State::new();
    let result = state_adapter::with_state(&mut context, &mut state, |context| {
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
          Reaction.add({ id: "boot.goto", event: "boot:ready", passage: /^start$/i, goto: "Target" });
          Reaction.add({ id: "score.50", state: "$score", passage: "Target", cond: ({ before, after }) => before < 50 && after >= 50, include: "StateNotice", once: true });
          Event.emit("boot:ready");
        "#,
    )
    .unwrap();
    std::fs::write(
        root.join("contents/story/main.twee"),
        r#":: Start
forbidden-start-body

:: Target
<<set $score to 50>>target-body<br>

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
    assert!(json.find("target-body").unwrap() < json.find("state-notice").unwrap());
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
    use super::*;
    use crate::{RuntimeData, RuntimeSession};
    use narrava_loom_core::{
        GameIdentity, bytecode::BytecodeProgram, hir::HirStory, lir::LirProgram, mir::MirStory,
        twee,
    };
    use narrava_loom_protocol::{
        HostNodeDto, HostUpdateDto, PendingOperation, PendingResult, RuntimeCommand, RuntimeUpdate,
        SaveOperation,
    };

    fn audio_update(
        update: RuntimeUpdate,
    ) -> (Vec<narrava_loom_protocol::AudioEffect>, HostUpdateDto) {
        match update {
            RuntimeUpdate::Audio {
                effects,
                update: Some(update),
            } => (effects, update),
            RuntimeUpdate::Ready { update } => (Vec::new(), update),
            _ => panic!("expected completed frame"),
        }
    }

    fn audio_navigate(
        runtime: &mut RuntimeSession<'_, '_>,
        update: &HostUpdateDto,
        target: &str,
    ) -> (Vec<narrava_loom_protocol::AudioEffect>, HostUpdateDto) {
        let interaction = update
            .nodes
            .iter()
            .find_map(|node| match node {
                HostNodeDto::Navigation {
                    id, target: name, ..
                } if name.as_deref() == Some(target) => Some(id.clone()),
                _ => None,
            })
            .expect("test navigation must exist");
        audio_update(
            runtime
                .execute(RuntimeCommand::Activate { interaction })
                .unwrap(),
        )
    }

    fn action_id(update: &HostUpdateDto, label: &str) -> String {
        update
            .nodes
            .iter()
            .find_map(|node| match node {
                HostNodeDto::Navigation {
                    id,
                    label: text,
                    target: None,
                    ..
                } if text == label => Some(id.clone()),
                _ => None,
            })
            .expect("action link")
    }

    #[test]
    fn dialog_action_opens_four_pages_without_navigation_and_can_reopen() {
        with_runtime(
            r#":: Start
<<set $opened to 0>>
<<link "查看角色">>
<<set $opened to $opened + 1>>
<<dialog "装备">>
<<page "属性">>
<<print $opened>>
<<meter "体力" 72 0 100>>
<<page "装备">>
<<image "tree.png" "树">>
<<page "经历">>
<<scriptText>>
<<page "其他">>
只有文字。
<</dialog>>
<</link>>
"#,
            "Macro.add('scriptText', {handler: () => '脚本正文'});",
            |runtime| {
                let (_, start) = audio_update(runtime.execute(RuntimeCommand::Start).unwrap());
                assert!(
                    !start
                        .nodes
                        .iter()
                        .any(|node| matches!(node, HostNodeDto::Dialog { .. }))
                );
                let id: String = action_id(&start, "查看角色");
                let mut previous_key: Option<String> = None;
                for count in 1..=2 {
                    let (_, update) = audio_update(
                        runtime
                            .execute(RuntimeCommand::Activate {
                                interaction: id.clone(),
                            })
                            .unwrap(),
                    );
                    assert_eq!(update.current, "Start");
                    assert!(!update.can_back);
                    let dialogs: Vec<&HostNodeDto> = update
                        .nodes
                        .iter()
                        .filter(|node| matches!(node, HostNodeDto::Dialog { .. }))
                        .collect();
                    assert_eq!(dialogs.len(), 1);
                    let HostNodeDto::Dialog {
                        key,
                        initial,
                        pages,
                    } = dialogs[0]
                    else {
                        unreachable!()
                    };
                    assert_eq!(initial, "装备");
                    assert_eq!(
                        pages
                            .iter()
                            .map(|page| page.title.as_str())
                            .collect::<Vec<_>>(),
                        ["属性", "装备", "经历", "其他"]
                    );
                    assert!(pages[0].nodes.iter().any(|node| matches!(node, HostNodeDto::Text { text, .. } if text == &count.to_string())));
                    assert!(pages[0].nodes.iter().any(|node| matches!(node, HostNodeDto::Component { capability, .. } if capability == "meter")));
                    assert!(
                        matches!(&pages[1].nodes.iter().find(|node| matches!(node, HostNodeDto::Image { .. })).unwrap(), HostNodeDto::Image { resource, alt, .. } if resource == "img/tree.png" && alt == "树")
                    );
                    assert!(pages[2].nodes.iter().any(
                        |node| matches!(node, HostNodeDto::Text { text, .. } if text == "脚本正文")
                    ));
                    assert_ne!(previous_key.as_ref(), Some(key));
                    previous_key = Some(key.clone());
                }
            },
        );
    }

    #[test]
    fn dialog_action_pending_cancel_and_invalid_default_roll_back() {
        for initial in ["页", "不存在"] {
            with_runtime(
                &format!(
                    r#":: Start
<<set $opened to 0>>
<<link "打开">>
<<set $opened to $opened + 1>>
<<dialog "{initial}">>
<<page "页">>
<<wait>>
<<print $opened>>
<</dialog>>
<</link>>
"#
                ),
                "Macro.add('wait', {handler: async () => { await Host.delay(1); return '完成'; }});",
                |runtime| {
                    let (_, start) = audio_update(runtime.execute(RuntimeCommand::Start).unwrap());
                    let id = action_id(&start, "打开");
                    let operation = pending(
                        runtime
                            .execute(RuntimeCommand::Activate {
                                interaction: id.clone(),
                            })
                            .unwrap(),
                    )
                    .id();
                    assert!(
                        runtime
                            .execute(RuntimeCommand::Cancel {
                                operation: operation + 1
                            })
                            .is_err()
                    );
                    runtime
                        .execute(RuntimeCommand::Cancel { operation })
                        .unwrap();
                    let operation = pending(
                        runtime
                            .execute(RuntimeCommand::Activate {
                                interaction: id.clone(),
                            })
                            .unwrap(),
                    )
                    .id();
                    let result = runtime.execute(RuntimeCommand::Resume {
                        operation,
                        result: None,
                    });
                    if initial == "页" {
                        let (_, update) = audio_update(result.unwrap());
                        let HostNodeDto::Dialog { pages, .. } = update
                            .nodes
                            .iter()
                            .find(|node| matches!(node, HostNodeDto::Dialog { .. }))
                            .unwrap()
                        else {
                            unreachable!()
                        };
                        assert!(pages[0].nodes.iter().any(
                            |node| matches!(node, HostNodeDto::Text { text, .. } if text == "1")
                        ));
                        assert!(!update.can_back);
                    } else {
                        assert!(result.is_err());
                        assert!(matches!(
                            runtime
                                .execute(RuntimeCommand::Activate { interaction: id })
                                .unwrap(),
                            RuntimeUpdate::Pending { .. }
                        ));
                    }
                },
            );
        }
    }

    #[test]
    fn audio_macros_and_script_share_declarations_without_surface_nodes() {
        with_runtime(
            r#":: Start
<<audio "forest.ogg" "bgm">>
<<scriptAudio>>
正文
"#,
            r#"Macro.add('scriptAudio', { handler: () => Audio.play('bell.wav', {channel:'voice', loop:false, volume:0.4}) });"#,
            |runtime| {
                let (effects, update) =
                    audio_update(runtime.execute(RuntimeCommand::Start).unwrap());
                assert_eq!(
                    serde_json::to_value(effects).unwrap(),
                    serde_json::json!([
                        {"type":"play","resource":"audio/forest.ogg","channel":"bgm","loop":true,"volume":1.0},
                        {"type":"play","resource":"audio/bell.wav","channel":"voice","loop":false,"volume":0.4}
                    ])
                );
                assert!(
                    !serde_json::to_string(&update.nodes)
                        .unwrap()
                        .contains("audio/")
                );
                assert!(!update.can_back);
            },
        );
    }

    #[test]
    fn audio_tags_headers_continuity_override_and_history_use_current_passage() {
        with_runtime(
            r#":: Start
<<link [[进入森林|Forest]]>><</link>>
:: Header
页眉
<<audio "forest.ogg" "ambience" "forest">>
:: Footer
页脚
:: Forest [forest rain]
<<audio "birds.ogg" "voice">>
<<link [[继续|ForestNext]]>><</link>>
:: ForestNext [forest]
<<audio "audio/forest.ogg" "ambience">>
<<link [[洞穴|Cave]]>><</link>>
:: Cave [forest]
<<audio "cave.ogg" "ambience">>
<<link [[城镇|Town]]>><</link>>
:: Town
结束
"#,
            r#"Audio.play('rain.ogg', { channel:'rain', tags:['rain'], volume:0.2 });"#,
            |runtime| {
                let (effects, start) =
                    audio_update(runtime.execute(RuntimeCommand::Start).unwrap());
                assert!(effects.is_empty());
                assert!(serde_json::to_string(&start).unwrap().contains("页眉"));
                assert!(serde_json::to_string(&start).unwrap().contains("页脚"));
                let (effects, forest) = audio_navigate(runtime, &start, "Forest");
                assert_eq!(effects.len(), 3);
                let (effects, _next) = audio_navigate(runtime, &forest, "ForestNext");
                assert!(effects.iter().all(|effect| matches!(
                    effect,
                    narrava_loom_protocol::AudioEffect::Stop { .. }
                )));
                assert_eq!(
                    effects.len(),
                    2,
                    "forest ambience must continue without another play"
                );
                let operation = pending(
                    runtime
                        .execute(RuntimeCommand::SelectLanguage {
                            locale: "en".into(),
                        })
                        .unwrap(),
                )
                .id();
                let (effects, next) = audio_update(
                    runtime
                        .execute(RuntimeCommand::Resume {
                            operation,
                            result: Some(PendingResult::SelectLanguage),
                        })
                        .unwrap(),
                );
                assert!(
                    effects.is_empty(),
                    "language refresh must not restart matching audio"
                );
                let save = pending(
                    runtime
                        .execute(RuntimeCommand::Save {
                            operation: SaveOperation::Export,
                            target: "audio-test".into(),
                        })
                        .unwrap(),
                );
                let PendingOperation::Save {
                    operation,
                    document: Some(document),
                    ..
                } = save
                else {
                    panic!("export document expected");
                };
                assert!(matches!(
                    runtime
                        .execute(RuntimeCommand::Resume {
                            operation,
                            result: Some(PendingResult::Save { document: None })
                        })
                        .unwrap(),
                    RuntimeUpdate::Applied
                ));
                let operation = pending(
                    runtime
                        .execute(RuntimeCommand::Save {
                            operation: SaveOperation::Import,
                            target: "audio-test".into(),
                        })
                        .unwrap(),
                )
                .id();
                let (effects, restored) = audio_update(
                    runtime
                        .execute(RuntimeCommand::Resume {
                            operation,
                            result: Some(PendingResult::Save {
                                document: Some(document),
                            }),
                        })
                        .unwrap(),
                );
                assert!(
                    effects.is_empty(),
                    "restoring the same scene must not restart matching audio"
                );
                assert_eq!(restored.current, next.current);
                let (effects, cave) = audio_navigate(runtime, &restored, "Cave");
                assert!(
                    matches!(&effects[..], [narrava_loom_protocol::AudioEffect::Play {resource, ..}] if resource == "audio/cave.ogg")
                );
                let (effects, _) = audio_navigate(runtime, &cave, "Town");
                assert!(
                    matches!(&effects[..], [narrava_loom_protocol::AudioEffect::Stop {channel}] if channel == "ambience")
                );
                let (effects, _) = audio_update(runtime.execute(RuntimeCommand::Back).unwrap());
                assert!(
                    matches!(&effects[..], [narrava_loom_protocol::AudioEffect::Play {resource, ..}] if resource == "audio/cave.ogg")
                );
            },
        );
    }

    #[test]
    fn audio_waits_for_commit_and_cancel_or_failure_discards_it() {
        for cancel in [false, true] {
            with_runtime(
                ":: Start\n<<audio \"audio/a.wav\">><<wait>>",
                "Macro.add('wait', { handler: async () => { await Host.delay(1); Audio.stop('sfx'); } });",
                |runtime| {
                    let operation = pending(runtime.execute(RuntimeCommand::Start).unwrap()).id();
                    assert!(
                        runtime
                            .execute(RuntimeCommand::Cancel {
                                operation: operation + 1
                            })
                            .is_err()
                    );
                    let result = runtime
                        .execute(if cancel {
                            RuntimeCommand::Cancel { operation }
                        } else {
                            RuntimeCommand::Resume {
                                operation,
                                result: None,
                            }
                        })
                        .unwrap();
                    if cancel {
                        assert!(matches!(result, RuntimeUpdate::Applied));
                        let operation =
                            pending(runtime.execute(RuntimeCommand::Start).unwrap()).id();
                        let RuntimeUpdate::Audio { effects, .. } = runtime
                            .execute(RuntimeCommand::Resume {
                                operation,
                                result: None,
                            })
                            .unwrap()
                        else {
                            panic!("new execution must complete");
                        };
                        assert_eq!(
                            effects.len(),
                            1,
                            "cancelled audio must not leak into next execution"
                        );
                    } else {
                        let RuntimeUpdate::Audio { effects, .. } = result else {
                            panic!("effects must follow successful resume");
                        };
                        assert_eq!(effects.len(), 1);
                    }
                    assert!(runtime.execute(RuntimeCommand::Back).is_err());
                },
            );
        }
        with_runtime(
            r#":: Start
<<audio "audio/a.wav">><<failOnce>>"#,
            "let attempts = 0; Macro.add('failOnce', { handler: () => { if (attempts++ === 0) Audio.play('../bad.wav'); } });",
            |runtime| {
                assert!(runtime.execute(RuntimeCommand::Start).is_err());
                let RuntimeUpdate::Audio { effects, .. } =
                    runtime.execute(RuntimeCommand::Start).unwrap()
                else {
                    panic!("retry must succeed");
                };
                assert_eq!(
                    effects.len(),
                    1,
                    "failed execution must discard queued effects"
                );
            },
        );
    }

    #[test]
    fn direct_dialog_uses_vm_and_rejects_duplicate_titles() {
        with_runtime(
            ":: Start\n<<dialog \"页\">>\n<<page \"页\">>\n正文\n<</dialog>>",
            "",
            |runtime| {
                let (_, update) = audio_update(runtime.execute(RuntimeCommand::Start).unwrap());
                assert!(update.nodes.iter().any(
                    |node| matches!(node, HostNodeDto::Dialog { pages, .. } if pages.len() == 1)
                ));
            },
        );
        for source in [
            ":: Start\n<<dialog \"页\">>\n<<page \"页\">>\nA\n<<page \"页\">>\nB\n<</dialog>>",
            ":: Start\n<<dialog 1>>\n<<page 1>>\n正文\n<</dialog>>",
        ] {
            with_runtime(source, "", |runtime| {
                assert!(runtime.execute(RuntimeCommand::Start).is_err());
            });
        }
    }

    fn with_runtime(story: &str, script: &str, test: impl FnOnce(&mut RuntimeSession<'_, '_>)) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::path::PathBuf::from(format!(
            "target/test-projects/session-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("contents/story")).unwrap();
        std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
        std::fs::write(root.join("contents/story/main.twee"), story).unwrap();
        std::fs::write(root.join("contents/scripts/main.js"), script).unwrap();
        let sources = SourceList::discover(&root).unwrap();
        let ast = twee::Story::build(&sources.items).unwrap();
        let hir = HirStory::lower(&ast).unwrap();
        let mir = MirStory::lower(&hir).unwrap();
        let lir = LirProgram::lower(&mir).unwrap();
        let bytecode = BytecodeProgram::compile(&lir);
        let mut state: State = State::new();
        let binding = EcmaBinding::load(
            &sources,
            &ResourceCatalog::default(),
            mir.i18n(),
            "en",
            &mut state,
        )
        .unwrap();
        let data = RuntimeData::new(
            GameIdentity::new("session.test", "1.0.0").unwrap(),
            mir.i18n().clone(),
            "en".into(),
            Vec::new(),
        );
        let mut runtime = RuntimeSession::with_data(&hir, &bytecode, binding, state, data);
        test(&mut runtime);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn pending(update: RuntimeUpdate) -> PendingOperation {
        let RuntimeUpdate::Pending { operation } = update else {
            panic!("expected pending")
        };
        operation
    }

    fn resume(
        runtime: &mut RuntimeSession<'_, '_>,
        operation: u64,
        result: Option<PendingResult>,
    ) -> RuntimeUpdate {
        runtime
            .execute(RuntimeCommand::Resume { operation, result })
            .unwrap()
    }

    fn navigation(nodes: &[HostNodeDto]) -> Option<String> {
        nodes.iter().find_map(|node| match node {
            HostNodeDto::Navigation { id, .. } => Some(id.clone()),
            HostNodeDto::Container { nodes, .. } | HostNodeDto::Region { nodes, .. } => {
                navigation(nodes)
            }
            _ => None,
        })
    }
    fn export(runtime: &mut RuntimeSession<'_, '_>) -> Vec<u8> {
        let PendingOperation::Save {
            operation,
            document: Some(document),
            ..
        } = pending(
            runtime
                .execute(RuntimeCommand::Save {
                    operation: SaveOperation::Export,
                    target: "quick".into(),
                })
                .unwrap(),
        )
        else {
            panic!("export document")
        };
        resume(
            runtime,
            operation,
            Some(PendingResult::Save { document: None }),
        );
        document
    }

    #[test]
    fn pending_commands_preserve_identity_and_resume_through_special_regions() {
        with_runtime(
            ":: Start\n<<wait>>Main\n:: Bar\n<<wait>>Bar\n",
            "Macro.add('wait', {handler: async () => { await Host.delay(1); await Host.delay(2); return 'ready'; }});",
            |runtime| {
                for command in [
                    RuntimeCommand::Activate {
                        interaction: "missing".into(),
                    },
                    RuntimeCommand::Input {
                        interaction: "missing".into(),
                        value: serde_json::Value::Null,
                    },
                ] {
                    assert_eq!(
                        runtime.execute(command).unwrap_err().code,
                        "runtime_session.not_started"
                    );
                }
                let first = pending(runtime.execute(RuntimeCommand::Start).unwrap()).id();
                assert_eq!(
                    runtime.execute(RuntimeCommand::Back).unwrap_err().code,
                    "runtime_session.pending"
                );
                assert_eq!(
                    runtime
                        .execute(RuntimeCommand::Resume {
                            operation: first + 100,
                            result: None
                        })
                        .unwrap_err()
                        .code,
                    "runtime_session.operation_mismatch"
                );
                let second = pending(resume(runtime, first, None)).id();
                assert_ne!(first, second);
                let third = pending(resume(runtime, second, None)).id();
                let fourth = pending(resume(runtime, third, None)).id();
                let RuntimeUpdate::Ready { update } = resume(runtime, fourth, None) else {
                    panic!("ready")
                };
                assert_eq!(update.current, "Start");
                assert!(update.nodes.iter().any(
                    |node| matches!(node, HostNodeDto::Region { region, .. } if region == "bar")
                ));
                assert_eq!(
                    runtime.execute(RuntimeCommand::Start).unwrap_err().code,
                    "runtime_session.already_started"
                );
                // A rejected command must not discard the last presented frame.
                assert_eq!(
                    runtime.execute(RuntimeCommand::Start).unwrap_err().code,
                    "runtime_session.already_started"
                );
            },
        );
    }

    #[test]
    fn cancel_restores_the_command_state_and_consumes_the_operation() {
        with_runtime(
            ":: Start\n<<set $score to 7>><<wait>>Main\n",
            "State.variables.set('score', 1); Macro.add('wait', {handler: async () => { await Host.delay(1); }});",
            |runtime| {
                let before = export(runtime);
                let operation = pending(runtime.execute(RuntimeCommand::Start).unwrap()).id();
                assert_eq!(
                    runtime
                        .execute(RuntimeCommand::Cancel { operation })
                        .unwrap(),
                    RuntimeUpdate::Applied
                );
                assert_eq!(
                    runtime
                        .execute(RuntimeCommand::Resume {
                            operation,
                            result: None
                        })
                        .unwrap_err()
                        .code,
                    "runtime_session.unknown_operation"
                );
                assert_eq!(export(runtime), before);
            },
        );
    }

    #[test]
    fn save_round_trip_and_host_failures_preserve_presented_history() {
        with_runtime(
            ":: Start\n<<set $score to 1>><<link [[Next|Next]]>><</link>>\n:: Next\n<<set $score to 2>>Done\n",
            "",
            |runtime| {
                let RuntimeUpdate::Ready { update } =
                    runtime.execute(RuntimeCommand::Start).unwrap()
                else {
                    panic!("start")
                };
                let id = navigation(&update.nodes)
                    .unwrap_or_else(|| panic!("missing navigation: {update:?}"));
                let saved = export(runtime);
                runtime
                    .execute(RuntimeCommand::Activate { interaction: id })
                    .unwrap();
                let RuntimeUpdate::Ready { update } =
                    runtime.execute(RuntimeCommand::Back).unwrap()
                else {
                    panic!("back")
                };
                assert_eq!(update.current, "Start");
                assert!(update.can_forward);
                let RuntimeUpdate::Ready { update } =
                    runtime.execute(RuntimeCommand::Forward).unwrap()
                else {
                    panic!("forward")
                };
                assert_eq!(update.current, "Next");
                let operation = pending(
                    runtime
                        .execute(RuntimeCommand::Save {
                            operation: SaveOperation::Import,
                            target: "quick".into(),
                        })
                        .unwrap(),
                )
                .id();
                let RuntimeUpdate::Ready { update } = resume(
                    runtime,
                    operation,
                    Some(PendingResult::Save {
                        document: Some(saved.clone()),
                    }),
                ) else {
                    panic!("restored")
                };
                assert_eq!(update.current, "Start");
                assert_eq!(export(runtime), saved);
                let operation = pending(
                    runtime
                        .execute(RuntimeCommand::SelectLanguage {
                            locale: "en".into(),
                        })
                        .unwrap(),
                )
                .id();
                assert_eq!(
                    runtime
                        .execute(RuntimeCommand::Resume {
                            operation,
                            result: Some(PendingResult::Save { document: None })
                        })
                        .unwrap_err()
                        .code,
                    "runtime_session.platform_result_mismatch"
                );
                let operation = pending(
                    runtime
                        .execute(RuntimeCommand::Save {
                            operation: SaveOperation::Import,
                            target: "quick".into(),
                        })
                        .unwrap(),
                )
                .id();
                assert!(
                    runtime
                        .execute(RuntimeCommand::Resume {
                            operation,
                            result: Some(PendingResult::Save {
                                document: Some(vec![0])
                            })
                        })
                        .is_err()
                );
                assert_eq!(export(runtime), saved);
                let operation = pending(
                    runtime
                        .execute(RuntimeCommand::SelectLanguage {
                            locale: "en".into(),
                        })
                        .unwrap(),
                )
                .id();
                let RuntimeUpdate::Ready { update } =
                    resume(runtime, operation, Some(PendingResult::SelectLanguage))
                else {
                    panic!("refresh")
                };
                assert_eq!(update.current, "Start");
                assert!(!update.can_back);
            },
        );
    }

    #[test]
    fn failed_reactions_restore_state_history_and_the_same_interaction() {
        with_runtime(
            ":: Start\n<<link [[Next|Next]]>><</link>>\n:: Next\n<<set $score to 2>>Done\n",
            "State.variables.set('score', 1); Reaction.add({id: 'broken', state: '$score', once: true, include: 'Missing'});",
            |runtime| {
                let RuntimeUpdate::Ready { update } =
                    runtime.execute(RuntimeCommand::Start).unwrap()
                else {
                    panic!("ready")
                };
                let id: String = navigation(&update.nodes).unwrap();
                let before: Vec<u8> = export(runtime);
                for _attempt in 0..2 {
                    let error = runtime
                        .execute(RuntimeCommand::Activate {
                            interaction: id.clone(),
                        })
                        .unwrap_err();
                    assert_eq!(error.code, "reaction.include");
                    assert_eq!(export(runtime), before);
                }
            },
        );
    }

    #[test]
    fn input_save_failure_restores_the_input_checkpoint_after_host_resume() {
        with_runtime(
            ":: Start\n<<textbox \"$name\" \"before\">>\n",
            "State.variables.set('name', 'before'); Reaction.add({id: 'save-input', state: '$name', cond: () => { Save.export('quick'); return false; }, include: 'Start'});",
            |runtime| {
                let RuntimeUpdate::Ready { update } =
                    runtime.execute(RuntimeCommand::Start).unwrap()
                else {
                    panic!("ready")
                };
                let id: String = update
                    .nodes
                    .iter()
                    .find_map(|node| match node {
                        HostNodeDto::Textbox { id, .. } => Some(id.clone()),
                        _ => None,
                    })
                    .unwrap();
                let before: Vec<u8> = export(runtime);
                let operation: u64 = pending(
                    runtime
                        .execute(RuntimeCommand::Input {
                            interaction: id,
                            value: serde_json::json!("after"),
                        })
                        .unwrap(),
                )
                .id();
                let error = runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: Some(PendingResult::Failed {
                            error: narrava_loom_protocol::HostErrorDto::new(
                                "test.io",
                                "write failed",
                            ),
                        }),
                    })
                    .unwrap_err();
                assert_eq!(error.code, "test.io");
                assert_eq!(export(runtime), before);
            },
        );
    }

    #[test]
    fn script_language_selection_refreshes_via_the_same_host_operation() {
        with_runtime(":: Start\nMain\n", "I18n.select('en');", |runtime| {
            let operation = pending(runtime.execute(RuntimeCommand::Start).unwrap()).id();
            let RuntimeUpdate::Ready { update } =
                resume(runtime, operation, Some(PendingResult::SelectLanguage))
            else {
                panic!("refresh")
            };
            assert_eq!(update.current, "Start");
            assert!(!update.can_back);
        });
    }
}

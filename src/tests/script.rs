//! Script Bundle 与叙事源码分流测试。

use crate::{
    SourceList,
    expression::{
        evaluator::evaluate_with_mut,
        parse,
        value::{ScriptCallable, Value},
    },
    script::{
        ScriptBinding, ScriptBundle, ScriptFunctionHost, ScriptI18nApi, ScriptLanguage,
        ScriptLoadContext, ScriptMacroDefinitions, ScriptMacroHooks, ScriptRuntimeContext,
        ScriptStoryApi, ScriptStoryHost, script_macro_definition,
    },
    state::State,
};

#[test]
fn script_bundle_keeps_only_ordered_ts_and_js_sources() {
    let sources: SourceList =
        SourceList::discover("src/tests/fixtures/game").expect("示例 Source 应可发现");
    let bundle: ScriptBundle<'_> = ScriptBundle::from_sources(&sources);

    assert_eq!(bundle.modules().len(), 1);
    assert_eq!(bundle.modules()[0].path(), "scripts/main.ts");
    assert_eq!(bundle.modules()[0].language(), ScriptLanguage::TypeScript);
    assert!(bundle.modules()[0].source().contains("Script Binding"));
}

#[test]
fn script_load_context_exposes_macro_crud_hooks_and_logger_when_configured() {
    use crate::{
        logger::{LogEvent, LogLevel, Logger},
        macro_runtime::{MacroArgumentKind, MacroBodyKind, MacroExecutionKind},
    };

    let mut state: State = State::new();
    let mut definitions: ScriptMacroDefinitions = ScriptMacroDefinitions::new();
    let mut hooks: ScriptMacroHooks = ScriptMacroHooks::new();
    let mut logger: Logger = Logger::new();
    {
        let mut context: ScriptLoadContext<'_> = ScriptLoadContext::new(&mut state)
            .with_macro(&mut definitions, &mut hooks)
            .with_logger(&mut logger);

        {
            let mut macro_api = context.macro_api().expect("配置后应开放 Macro API");
            let definition = script_macro_definition(
                ScriptCallable::new(10, "notice"),
                MacroBodyKind::Inline,
                MacroArgumentKind::ArgumentList,
                MacroExecutionKind::Sync,
            );
            assert!(macro_api.add("notice", definition).is_none());
            assert!(macro_api.has("notice"));
            assert_eq!(
                macro_api
                    .get("notice")
                    .expect("定义应存在")
                    .handler
                    .callable()
                    .id(),
                10
            );

            let hook_id = macro_api
                .before("notice", ScriptCallable::new(11, "beforeNotice"))
                .expect("普通 Macro 应允许 before Hook");
            assert_eq!(
                macro_api.off(hook_id).expect("订阅应可删除").name(),
                "beforeNotice"
            );
            assert!(macro_api.del("notice").is_some());
        }

        context
            .logger()
            .expect("配置后应开放 Logger")
            .log(LogEvent::new(LogLevel::Info, "script", "loaded"));
    }

    assert!(!definitions.has("notice"));
    assert_eq!(logger.get().len(), 1);
}

#[test]
fn script_load_context_supports_bulk_global_imports() {
    let mut state: State = State::new();
    let mut context: ScriptLoadContext<'_> = ScriptLoadContext::new(&mut state);
    let report = context.global_extend([
        (String::from("title"), Value::string("Forest")),
        (String::from("visits"), Value::Number(2.0)),
        (String::from("title"), Value::string("Deep Forest")),
    ]);

    assert_eq!(report.inserted, 2);
    assert_eq!(report.replaced, 1);
    assert_eq!(
        state.global_get("title"),
        Some(&Value::string("Deep Forest"))
    );
}

#[test]
fn script_load_context_exposes_events_resources_and_i18n_when_configured() {
    use crate::{
        events::{Event, EventFilter},
        i18n::I18nCatalog,
        resource::{ResourceCatalog, ResourceInput},
    };

    let mut state = State::new();
    let mut events = Event::new();
    let resources = ResourceCatalog::new([ResourceInput::new("data/info.txt", b"loom".to_vec())])
        .expect("测试资源应有效");
    let i18n = I18nCatalog::default();
    let subscription = events.subscribe(EventFilter::named("script:ready"));
    {
        let mut context = ScriptLoadContext::new(&mut state)
            .with_events(&mut events)
            .with_resources(&resources)
            .with_i18n(&i18n, "zh-CN", "en");

        context
            .events()
            .expect("配置后应开放 Event")
            .emit("script:ready", Value::Boolean(true))
            .expect("脚本事件应可发布");
        assert_eq!(
            context
                .resources()
                .expect("配置后应开放 Resource")
                .text("data/info.txt")
                .unwrap(),
            Some("loom")
        );
        let i18n: ScriptI18nApi<'_> = context.i18n().expect("配置后应开放 I18n");
        assert_eq!(i18n.default_locale(), "zh-CN");
        assert_eq!(i18n.locale(), "en");
        assert!(i18n.catalog().messages().is_empty());
    }

    assert_eq!(events.take(subscription).unwrap().len(), 1);
}

#[test]
fn script_story_api_reuses_core_story_queries() {
    use crate::{
        SourcePath,
        hir::{HirPassage, HirStory},
        story::Story,
    };

    let source = SourcePath::fragment();
    let compiled = HirStory {
        passages: vec![
            HirPassage {
                source: &source,
                name: "Start",
                tags: vec!["opening"],
                body: Vec::new(),
            },
            HirPassage {
                source: &source,
                name: "Hall",
                tags: vec!["hub"],
                body: Vec::new(),
            },
        ],
    };
    let mut story = Story::new(&compiled);
    story.goto("Start").expect("起始 Passage 应存在");
    story.goto("Hall").expect("大厅 Passage 应存在");
    story.goto("Hall").expect("大厅可重复访问");
    let api = ScriptStoryApi::new(&story);

    assert!(api.has("Start"));
    assert_eq!(api.current().unwrap().name, "Hall");
    assert_eq!(api.get("Start").unwrap().tags, vec!["opening"]);
    assert_eq!(api.visits("Hall"), 2);
}

#[test]
fn script_callable_is_invoked_by_expression_through_binding_host() {
    struct TestFunctionHost {
        calls: usize,
    }

    impl ScriptFunctionHost for TestFunctionHost {
        fn call(
            &mut self,
            callable: &ScriptCallable,
            arguments: Vec<Value>,
            state: &mut State,
        ) -> Result<Value, crate::expression::evaluator::ScriptCallError> {
            assert_eq!(callable.id(), 7);
            assert_eq!(callable.name(), "sum");
            assert_eq!(arguments, vec![Value::Number(2.0), Value::Number(3.0)]);
            self.calls += 1;
            let _previous: Option<Value> = state.temporary_set("called", Value::Boolean(true));
            Ok(Value::Number(5.0))
        }
    }

    let mut state: State = State::new();
    let _previous: Option<Value> =
        state.global_set("sum", Value::ScriptCallable(ScriptCallable::new(7, "sum")));
    let mut host: TestFunctionHost = TestFunctionHost { calls: 0 };
    let mut context: ScriptRuntimeContext<'_, TestFunctionHost> =
        ScriptRuntimeContext::new(&mut state, &mut host);
    let expression = parse("sum(2, 3)").expect("Script 函数调用应可解析");

    let value: Value =
        evaluate_with_mut(&expression, &mut context).expect("Binding 应完成函数调用");

    assert_eq!(value, Value::Number(5.0));
    assert_eq!(host.calls, 1);
    assert_eq!(state.temporary_get("called"), Some(&Value::Boolean(true)));
}

#[test]
fn state_routes_script_callable_without_persisting_the_binding_object() {
    use std::rc::Rc;

    use crate::{expression::evaluator::ScriptCallError, script::ScriptCallDispatcher};

    struct Doubler;
    impl ScriptCallDispatcher for Doubler {
        fn call(
            &self,
            _callable: &ScriptCallable,
            arguments: Vec<Value>,
            _state: &mut State,
        ) -> Result<Value, ScriptCallError> {
            let Value::Number(value) = arguments[0] else {
                return Err(ScriptCallError::Failed);
            };
            Ok(Value::Number(value * 2.0))
        }
    }

    let mut state = State::new();
    state.global_set(
        "twice",
        Value::ScriptCallable(ScriptCallable::new(1, "twice")),
    );
    state.attach_script_dispatcher(Rc::new(Doubler));
    let expression = parse("twice(6)").unwrap();

    assert_eq!(
        evaluate_with_mut(&expression, &mut state).unwrap(),
        Value::Number(12.0)
    );
    state.detach_script_dispatcher();
    assert!(evaluate_with_mut(&expression, &mut state).is_err());
}

#[test]
fn script_callable_requires_a_writable_binding_context() {
    let mut state: State = State::new();
    let _previous: Option<Value> =
        state.global_set("sum", Value::ScriptCallable(ScriptCallable::new(7, "sum")));
    let expression = parse("sum(2, 3)").expect("Script 函数调用应可解析");

    let error = crate::expression::evaluator::evaluate_with(&expression, &state)
        .expect_err("只读 State 不拥有 Script Binding");

    assert_eq!(
        error,
        crate::expression::evaluator::EvalError::MissingWriteContext(expression.span)
    );
}

#[test]
fn script_callable_is_never_saveable_even_inside_data_collections() {
    let callable: Value = Value::ScriptCallable(ScriptCallable::new(7, "sum"));
    let nested: Value = Value::array(vec![Value::object(vec![(
        String::from("callback"),
        callable.clone(),
    )])]);

    assert!(!callable.is_saveable());
    assert!(!nested.is_saveable());
    assert!(Value::array(vec![Value::Number(1.0)]).is_saveable());
}

#[test]
fn script_binding_explicitly_imports_values_through_state_api() {
    struct TestBinding;

    impl ScriptBinding for TestBinding {
        type Error = String;

        fn load(
            &mut self,
            bundle: &ScriptBundle<'_>,
            context: &mut ScriptLoadContext<'_>,
        ) -> Result<(), Self::Error> {
            assert_eq!(bundle.modules().len(), 1);
            context
                .state()
                .global_set("script_name", Value::string("main.ts"));
            Ok(())
        }
    }

    let sources: SourceList =
        SourceList::discover("src/tests/fixtures/game").expect("示例 Source 应可发现");
    let bundle: ScriptBundle<'_> = ScriptBundle::from_sources(&sources);
    let mut state: State = State::new();
    let mut binding: TestBinding = TestBinding;
    let mut context: ScriptLoadContext<'_> = ScriptLoadContext::new(&mut state);

    binding
        .load(&bundle, &mut context)
        .expect("测试 Binding 应加载成功");

    assert_eq!(
        state.global_get("script_name"),
        Some(&Value::string("main.ts"))
    );
}

#[test]
fn script_bundle_can_be_empty_without_creating_a_runtime() {
    let sources: SourceList = SourceList { items: Vec::new() };
    let bundle: ScriptBundle<'_> = ScriptBundle::from_sources(&sources);

    assert!(bundle.is_empty());
}

//! Save 文档、Value 图与 State／Story 原子恢复测试。

use std::path::Path;

use crate::{
    GameIdentity,
    expression::value::{ArrayValue, ScriptCallable, Value},
    hir::{HirPassage, HirStory},
    save::{
        SaveCompletion, SaveController, SaveDocument, SaveError, SaveLifecycleController,
        SaveLifecycleSubscriptions, SaveOperation, SaveOutcome,
    },
    source::Source,
    state::State,
    story::Story,
};

#[test]
fn save_document_json_matches_the_current_schema() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let compiled: HirStory<'_> = test_story(&source);
    let story: Story<'_, '_> = Story::new(&compiled);
    let state: State = State::new();
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").expect("身份应有效");

    let encoded: String = SaveDocument::capture(&game, &state, &story)
        .expect("空运行状态应可捕获")
        .to_json()
        .expect("存档应可编码");
    let decoded: SaveDocument = SaveDocument::from_json(&encoded).expect("存档应可解码");

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&decoded.to_json().expect("应可再次编码"))
            .expect("再次编码结果应是 JSON"),
        serde_json::from_str::<serde_json::Value>(&encoded).expect("首次编码结果应是 JSON")
    );
}

#[test]
fn save_controller_runs_before_then_queues_host_request() {
    let mut subscriptions: SaveLifecycleSubscriptions<&str> = SaveLifecycleSubscriptions::new();
    let _before_id = subscriptions
        .before(SaveOperation::Export, "prefix")
        .expect("应可订阅 export before");
    let mut lifecycle: SaveLifecycleController<'_, _, _, _> = SaveLifecycleController::new(
        &subscriptions,
        |hook: &&str, _operation: SaveOperation, target: &mut String| {
            target.insert_str(0, hook);
            Ok(())
        },
        |_hook: &&str, _completion: &SaveCompletion| Ok(()),
    );
    let mut save: SaveController = SaveController::new();

    let request_id = save
        .export(":slot-1", &mut lifecycle)
        .expect("before 后的目标应有效");
    let request = save.take().expect("Host 应能取得请求");

    assert_eq!(request.id(), request_id);
    assert_eq!(request.operation(), SaveOperation::Export);
    assert_eq!(request.target(), "prefix:slot-1");
    assert!(save.take().is_none());
}

#[test]
fn save_controller_runs_after_only_when_host_completes_request() {
    use std::{cell::RefCell, rc::Rc};

    let observed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let mut subscriptions: SaveLifecycleSubscriptions<&str> = SaveLifecycleSubscriptions::new();
    let after_id = subscriptions
        .after(SaveOperation::Import, "loaded")
        .expect("应可订阅 import after");
    assert_eq!(subscriptions.off(after_id), Some("loaded"));
    let _after_id = subscriptions
        .after(SaveOperation::Import, "loaded")
        .expect("应可重新订阅");
    let observed_after: Rc<RefCell<Vec<String>>> = Rc::clone(&observed);
    let mut lifecycle: SaveLifecycleController<'_, _, _, _> = SaveLifecycleController::new(
        &subscriptions,
        |_hook: &&str, _operation: SaveOperation, _target: &mut String| Ok(()),
        move |hook: &&str, completion: &SaveCompletion| {
            observed_after.borrow_mut().push(format!(
                "{hook}:{}:{:?}",
                completion.request().target(),
                completion.outcome()
            ));
            Ok(())
        },
    );
    let mut save: SaveController = SaveController::new();

    let _request_id = save.import("slot-2", &mut lifecycle).expect("应可请求导入");
    assert!(observed.borrow().is_empty());
    let request = save.take().expect("Host 应取得导入请求");
    let completion = save
        .complete(request, SaveOutcome::Succeeded, &mut lifecycle)
        .expect("Host 完成后应执行 after");

    assert_eq!(completion.outcome(), &SaveOutcome::Succeeded);
    assert_eq!(observed.borrow().as_slice(), ["loaded:slot-2:Succeeded"]);
}

#[test]
fn save_controller_rejects_empty_target_after_before_hooks() {
    let subscriptions: SaveLifecycleSubscriptions<()> = SaveLifecycleSubscriptions::new();
    let mut lifecycle: SaveLifecycleController<'_, _, _, _> = SaveLifecycleController::new(
        &subscriptions,
        |_hook: &(), _operation: SaveOperation, _target: &mut String| Ok(()),
        |_hook: &(), _completion: &SaveCompletion| Ok(()),
    );
    let mut save: SaveController = SaveController::new();

    let error = save
        .export("   ", &mut lifecycle)
        .expect_err("空白 Host 目标必须被拒绝");

    assert_eq!(error.diagnostic().code, "save.request.empty_target");
    assert!(save.take().is_none());
}

#[test]
fn save_json_round_trip_restores_variables_aliases_and_story_cursor() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let compiled: HirStory<'_> = test_story(&source);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    story.goto("Start").expect("Start 应可导航");
    story.record_navigation(true);
    story.goto("Map").expect("Map 应可导航");
    story.record_navigation(true);
    story.goto("End").expect("End 应可导航");
    story.record_navigation(false);
    let _map = story.back().expect("游标应回到 Map");

    let shared: ArrayValue = ArrayValue::new(vec![
        Value::Number(f64::NAN),
        Value::Number(f64::NEG_INFINITY),
        Value::Number(-0.0),
    ]);
    let mut state: State = State::new();
    let _left = state.variables_set("left", Value::Array(shared.clone()));
    let _right = state.variables_set("right", Value::Array(shared));
    let _temporary = state.temporary_set("selection", Value::string("End"));
    let _global = state.global_set("runtimeApi", Value::Boolean(true));
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").expect("身份应有效");

    let save: SaveDocument = SaveDocument::capture(&game, &state, &story).expect("应可捕获存档");
    let json: String = save.to_json().expect("存档应可编码为 JSON");
    let decoded: SaveDocument = SaveDocument::from_json(json.as_str()).expect("JSON 应可解码");

    let _changed = state.variables_set("left", Value::string("changed"));
    let _extra = state.variables_set("extra", Value::Boolean(true));
    story.goto("End").expect("活动 Story 应可继续变化");
    decoded
        .restore(&game, &mut state, &mut story)
        .expect("同一游戏应可恢复");

    let Some(Value::Array(left)) = state.variables_get("left") else {
        panic!("left 应恢复为 Array")
    };
    let Some(Value::Array(right)) = state.variables_get("right") else {
        panic!("right 应恢复为 Array")
    };
    assert!(left.same_identity(right));
    let values: Vec<Value> = left.snapshot();
    let Value::Number(nan) = values[0] else {
        panic!("第一项应为 Number")
    };
    let Value::Number(infinity) = values[1] else {
        panic!("第二项应为 Number")
    };
    let Value::Number(negative_zero) = values[2] else {
        panic!("第三项应为 Number")
    };
    assert!(nan.is_nan());
    assert_eq!(infinity, f64::NEG_INFINITY);
    assert_eq!(negative_zero.to_bits(), (-0.0_f64).to_bits());
    assert!(!state.variables_has("extra"));
    assert!(!state.temporary_has("selection"));
    assert_eq!(state.global_get("runtimeApi"), Some(&Value::Boolean(true)));
    assert_eq!(story.current().map(|passage| passage.name), Some("Map"));
    assert_eq!(story.history().len(), 3);
    assert_eq!(
        story.safe_return_target().map(|passage| passage.name),
        Some("Start")
    );
}

#[test]
fn save_rejects_script_callable_anywhere_in_variables() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let compiled: HirStory<'_> = test_story(&source);
    let story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    let _callback = state.variables_set(
        "nested",
        Value::array(vec![Value::ScriptCallable(ScriptCallable::new(
            1, "callback",
        ))]),
    );
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").expect("身份应有效");

    let error: SaveError =
        SaveDocument::capture(&game, &state, &story).expect_err("函数不得进入存档");

    assert!(matches!(error, SaveError::UnsupportedValue { .. }));
}

#[test]
fn save_restore_rejects_another_game_without_mutating_runtime() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let compiled: HirStory<'_> = test_story(&source);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    story.goto("Start").expect("Start 应可导航");
    let mut state: State = State::new();
    let _score = state.variables_set("score", Value::Number(1.0));
    let saved_game: GameIdentity = GameIdentity::new("example.save", "1.2.3").expect("身份应有效");
    let active_game: GameIdentity = GameIdentity::new("another.game", "1.2.3").expect("身份应有效");
    let save: SaveDocument =
        SaveDocument::capture(&saved_game, &state, &story).expect("应可捕获存档");

    let _changed = state.variables_set("score", Value::Number(9.0));
    story.goto("Map").expect("活动 Story 应可变化");
    let error: SaveError = save
        .restore(&active_game, &mut state, &mut story)
        .expect_err("另一游戏不得恢复");

    assert!(matches!(error, SaveError::GameMismatch { .. }));
    assert_eq!(state.variables_get("score"), Some(&Value::Number(9.0)));
    assert_eq!(story.current().map(|passage| passage.name), Some("Map"));
}

#[test]
fn save_value_graph_preserves_cycles_without_recursive_serialization() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let compiled: HirStory<'_> = test_story(&source);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    story.goto("Start").expect("Start 应可导航");
    let cycle: ArrayValue = ArrayValue::new(Vec::new());
    cycle.with_mut(|items: &mut Vec<Value>| items.push(Value::Array(cycle.clone())));
    let mut state: State = State::new();
    let _cycle = state.variables_set("cycle", Value::Array(cycle));
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").expect("身份应有效");

    let save: SaveDocument = SaveDocument::capture(&game, &state, &story).expect("循环图应可捕获");
    let json: String = save.to_json().expect("循环图不应递归展开");
    let decoded: SaveDocument = SaveDocument::from_json(json.as_str()).expect("JSON 应有效");
    decoded
        .restore(&game, &mut state, &mut story)
        .expect("循环图应可恢复");

    let Some(Value::Array(root)) = state.variables_get("cycle") else {
        panic!("cycle 应恢复为 Array")
    };
    let Value::Array(child) = &root.snapshot()[0] else {
        panic!("cycle[0] 应为 Array")
    };
    assert!(root.same_identity(child));
}

#[test]
fn save_rejects_a_dangling_value_reference_before_runtime_mutation() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let compiled: HirStory<'_> = test_story(&source);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    story.goto("Start").expect("Start 应可导航");
    let mut state: State = State::new();
    let _items = state.variables_set("items", Value::array(vec![Value::Number(1.0)]));
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").expect("身份应有效");
    let save: SaveDocument = SaveDocument::capture(&game, &state, &story).expect("应可捕获存档");
    let mut json: serde_json::Value =
        serde_json::from_str(save.to_json().expect("应可编码").as_str()).expect("JSON 应有效");
    json["state"]["roots"]["items"]["value"] = serde_json::Value::from(999_u64);
    let damaged: SaveDocument =
        SaveDocument::from_json(json.to_string().as_str()).expect("结构仍是合法 Save JSON");

    let _changed = state.variables_set("items", Value::string("active"));
    let error: SaveError = damaged
        .restore(&game, &mut state, &mut story)
        .expect_err("悬空引用不得恢复");

    assert!(matches!(error, SaveError::InvalidValueGraph { .. }));
    assert_eq!(state.variables_get("items"), Some(&Value::string("active")));
    assert_eq!(story.current().map(|passage| passage.name), Some("Start"));
    assert_eq!(error.diagnostic().code, "save.invalid_value_graph");
}

fn test_story<'source>(source: &'source Source) -> HirStory<'source> {
    HirStory {
        passages: vec![
            passage(source, "Start"),
            passage(source, "Map"),
            passage(source, "End"),
        ],
    }
}

fn passage<'source>(source: &'source Source, name: &'source str) -> HirPassage<'source> {
    HirPassage {
        source: &source.path,
        name,
        tags: Vec::new(),
        body: Vec::new(),
    }
}

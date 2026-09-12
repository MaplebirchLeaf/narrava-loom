//! Save 文档、Value 图与 State／Story 原子恢复测试。

use std::path::Path;

use crate::{
    GameIdentity,
    expression::value::{ArrayValue, ScriptCallable, Value},
    hir::{HirPassage, HirStory},
    location::{Environment, LocationPosition, Place},
    random::RandomState,
    reaction::ReactionRuntimeState,
    save::{
        SaveCompletion, SaveController, SaveDocument, SaveError, SaveLifecycleController,
        SaveLifecycleSubscriptions, SaveOperation, SaveOutcome,
    },
    source::Source,
    state::State,
    story::{Story, StoryHistoryId},
};

#[test]
fn save_document_binary_has_a_versioned_header_and_round_trips() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let compiled: HirStory<'_> = test_story(&source);
    let story: Story<'_, '_> = Story::new(&compiled);
    let state: State = State::new();
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").expect("身份应有效");

    let encoded: Vec<u8> = SaveDocument::capture(&game, &state, &story)
        .expect("空运行状态应可捕获")
        .with_reactions(vec![ReactionRuntimeState {
            id: String::from("quest.once"),
            enabled: false,
            triggered: 1,
            destroyed: true,
        }])
        .to_bytes()
        .expect("存档应可编码");
    assert_eq!(&encoded[..8], b"NRSAVE\0\x04");
    let decoded: SaveDocument = SaveDocument::from_bytes(&encoded).expect("存档应可解码");
    assert_eq!(decoded.reactions()[0].id, "quest.once");
    assert_eq!(decoded.to_bytes().expect("应可再次编码"), encoded);
}

#[test]
fn save_version_two_restores_variables_and_history() {
    // 已发布 v2 协议：score=true、一次 Start 导航、空的进入前变量图。
    let bytes: &[u8] = b"NRSAVE\0\x02\x0cexample.save\x051.2.3\x01\x05score\x02\x01\0\x01\x05Start\x01\0\0\x01\0\0";
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let compiled: HirStory<'_> = test_story(&source);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = location_test_state();
    state.seed_random(77);
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").unwrap();
    let saved: SaveDocument = SaveDocument::from_bytes(bytes).expect("v2 存档仍应可解码");

    saved.restore(&game, &mut state, &mut story).unwrap();

    assert_eq!(state.variables_get("score"), Some(&Value::Boolean(true)));
    assert_eq!(story.current().map(|passage| passage.name), Some("Start"));
    assert_eq!(story.history().len(), 1);
    assert!(state.location_state().position.is_none());
    assert_eq!(state.random_state(), RandomState::default());
    assert!(state.location().get("town").is_some());
    let history_id: StoryHistoryId = story.current_entry().unwrap().id();
    assert_eq!(story.state_snapshot(history_id).unwrap().variables_len(), 0);
    assert!(
        story
            .state_snapshot(history_id)
            .unwrap()
            .location_state()
            .position
            .is_none()
    );
    assert_eq!(&saved.to_bytes().unwrap()[..8], b"NRSAVE\0\x04");
}

#[test]
fn save_version_three_keeps_location_positions_and_defaults_random_state() {
    // 固定 v3 线格式：score=true；历史地点 town[2,3] outside，当前位置 town[4,5]。
    let bytes: &[u8] = b"NRSAVE\0\x03\x0cexample.save\x051.2.3\x01\x05score\x02\x01\0\x01\x05Start\x01\0\0\x01\x04town\x04\x06\x01\x01\x01\0\0\x01\x04town\x08\x0a\0";
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .unwrap();
    let compiled: HirStory<'_> = test_story(&source);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = location_test_state();
    state.seed_random(77);
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").unwrap();

    let saved: SaveDocument = SaveDocument::from_bytes(bytes).unwrap();
    saved.restore(&game, &mut state, &mut story).unwrap();

    assert_eq!(state.variables_get("score"), Some(&Value::Boolean(true)));
    assert_eq!(
        state.location_state().position.as_ref().unwrap().point,
        [4, 5]
    );
    assert_eq!(state.random_state(), RandomState::default());
    let snapshot: &crate::state::StateSnapshot = story
        .state_snapshot(story.current_entry().unwrap().id())
        .unwrap();
    assert_eq!(
        snapshot.location_state().position.as_ref().unwrap().point,
        [2, 3]
    );
    assert_eq!(
        snapshot
            .location_state()
            .position
            .as_ref()
            .unwrap()
            .environment,
        Some(Environment::Outside)
    );
    assert_eq!(snapshot.random_state(), RandomState::default());
}

#[test]
fn save_random_state_round_trips_current_history_and_full_width_seed() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .unwrap();
    let compiled: HirStory<'_> = test_story(&source);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    state.seed_random(u64::MAX);
    let start_id: StoryHistoryId = story.goto("Start").unwrap().id();
    story.record_state_snapshot(start_id, state.snapshot());
    let first: f64 = state.random();
    let map_id: StoryHistoryId = story.goto("Map").unwrap().id();
    story.record_state_snapshot(map_id, state.snapshot());
    let _second: f64 = state.random();
    let tail: RandomState = state.random_state();
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").unwrap();
    state.begin_random_replay(RandomState::new(123));
    let _replayed: f64 = state.random();
    let bytes: Vec<u8> = SaveDocument::capture(&game, &state, &story)
        .and_then(|document: SaveDocument| document.to_bytes())
        .unwrap();
    state.end_random_replay();
    let next: f64 = state.random();
    state.seed_random(9);

    SaveDocument::from_bytes(&bytes)
        .unwrap()
        .restore(&game, &mut state, &mut story)
        .unwrap();

    assert_eq!(state.random_state(), tail);
    assert_eq!(state.random(), next);
    let previous_id: StoryHistoryId = story.back().unwrap().id();
    state.restore_snapshot(story.state_snapshot(previous_id).unwrap());
    assert_eq!(state.random_state().seed(), u64::MAX);
    assert_eq!(state.random(), first);
}

#[test]
fn save_location_round_trip_restores_current_and_history_positions() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .unwrap();
    let compiled: HirStory<'_> = test_story(&source);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = location_test_state();
    let start_id: StoryHistoryId = story.goto("Start").unwrap().id();
    story.record_state_snapshot(start_id, state.snapshot());
    state.location_state_mut().position.as_mut().unwrap().point = [4, 5];
    let map_id: StoryHistoryId = story.goto("Map").unwrap().id();
    story.record_state_snapshot(map_id, state.snapshot());
    state.location_state_mut().position.as_mut().unwrap().point = [8, 9];
    let end_id: StoryHistoryId = story.goto("End").unwrap().id();
    story.record_state_snapshot(end_id, state.snapshot());
    state.location_state_mut().position.as_mut().unwrap().point = [9, 9];
    state
        .location_state_mut()
        .position
        .as_mut()
        .unwrap()
        .environment = None;
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").unwrap();
    let bytes: Vec<u8> = SaveDocument::capture(&game, &state, &story)
        .and_then(|document: SaveDocument| document.to_bytes())
        .unwrap();
    let saved: SaveDocument = SaveDocument::from_bytes(&bytes).unwrap();
    state.location_state_mut().position = None;

    saved.restore(&game, &mut state, &mut story).unwrap();

    let position: &LocationPosition = state.location_state().position.as_ref().unwrap();
    assert_eq!(position.point, [9, 9]);
    assert_eq!(position.environment, None);
    assert!(state.location().get("town").is_some());
    let restored_map_id: StoryHistoryId = story.back().unwrap().id();
    state.restore_snapshot(story.state_snapshot(restored_map_id).unwrap());
    assert_eq!(
        state.location_state().position.as_ref().unwrap().point,
        [4, 5]
    );
    assert_eq!(
        state
            .location_state()
            .position
            .as_ref()
            .unwrap()
            .environment,
        Some(Environment::Outside)
    );
}

#[test]
fn save_rejects_invalid_location_positions_before_replacing_any_runtime_state() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .unwrap();
    let compiled: HirStory<'_> = test_story(&source);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = location_test_state();
    let start_id: StoryHistoryId = story.goto("Start").unwrap().id();
    story.record_state_snapshot(start_id, state.snapshot());
    let _old: Option<Value> = state.variables_set("score", Value::Number(5.0));
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").unwrap();
    let saved: SaveDocument = SaveDocument::capture(&game, &state, &story).unwrap();
    let original: serde_json::Value = serde_json::to_value(&saved).unwrap();
    let _old: Option<Value> = state.variables_set("score", Value::Number(99.0));
    let _old: Option<Value> = state.temporary_set("selection", Value::string("active"));
    story.goto("Map").unwrap();
    record_missing_history_states(&mut story, &state);
    let before: Vec<u8> = SaveDocument::capture(&game, &state, &story)
        .and_then(|document: SaveDocument| document.to_bytes())
        .unwrap();

    for history in [false, true] {
        for position in [
            serde_json::json!({"place":"missing", "point":[2,3], "environment":null}),
            serde_json::json!({"place":"town", "point":[20,30], "environment":null}),
        ] {
            let mut altered: serde_json::Value = original.clone();
            let location: &mut serde_json::Value = if history {
                &mut altered["story"]["history"][0]["location"]
            } else {
                &mut altered["location"]
            };
            location["position"] = position;
            let altered: SaveDocument = serde_json::from_value(altered).unwrap();
            let bytes: Vec<u8> = altered.to_bytes().unwrap();
            let invalid: SaveDocument = SaveDocument::from_bytes(&bytes).unwrap();

            let error: SaveError = invalid.restore(&game, &mut state, &mut story).unwrap_err();

            assert_eq!(error.diagnostic().code, "save.invalid_location");
            let after: Vec<u8> = SaveDocument::capture(&game, &state, &story)
                .and_then(|document: SaveDocument| document.to_bytes())
                .unwrap();
            assert_eq!(after, before);
            assert_eq!(
                state.temporary_get("selection"),
                Some(&Value::string("active"))
            );
        }
    }
}

fn location_test_state() -> State {
    let mut state: State = State::new();
    state
        .location_mut()
        .add(Place {
            id: String::from("town"),
            name: None,
            parent: None,
            bounds: vec![[0, 0], [10, 0], [10, 10], [0, 10]],
            entry: None,
        })
        .unwrap();
    state.location_state_mut().position = Some(LocationPosition {
        place: String::from("town"),
        point: [2, 3],
        environment: Some(Environment::Outside),
    });
    state
}

#[test]
fn save_binary_scales_without_a_json_text_buffer() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let compiled: HirStory<'_> = test_story(&source);
    let story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    for index in 0..10_000 {
        let _previous: Option<Value> = state.variables_set(
            format!("value-{index}").as_str(),
            Value::Number(index as f64),
        );
    }
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").expect("身份应有效");

    let bytes: Vec<u8> = SaveDocument::capture(&game, &state, &story)
        .and_then(|document: SaveDocument| document.to_bytes())
        .expect("大型持久状态应直接编码为二进制");
    let decoded: SaveDocument = SaveDocument::from_bytes(&bytes).expect("大型存档应可解码");

    assert!(
        bytes.len() < 250_000,
        "数值型大型存档不应承担 JSON 标签与缩进成本"
    );
    assert!(decoded.reactions().is_empty());
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
fn save_binary_round_trip_restores_variables_aliases_and_story_cursor() {
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
    record_missing_history_states(&mut story, &state);
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").expect("身份应有效");

    let save: SaveDocument = SaveDocument::capture(&game, &state, &story).expect("应可捕获存档");
    let bytes: Vec<u8> = save.to_bytes().expect("存档应可编码");
    let decoded: SaveDocument = SaveDocument::from_bytes(&bytes).expect("二进制存档应可解码");

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
fn save_round_trip_preserves_persistent_state_for_each_history_position() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let compiled: HirStory<'_> = test_story(&source);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let start_id = story.goto("Start").expect("Start 应可导航").id();
    story.record_state_snapshot(start_id, state.snapshot());
    let _first = state.variables_set("visits", Value::Number(1.0));
    let map_id = story.goto("Map").expect("Map 应可导航").id();
    story.record_state_snapshot(map_id, state.snapshot());
    let _second = state.variables_set("visits", Value::Number(2.0));
    let end_id = story.goto("End").expect("End 应可导航").id();
    story.record_state_snapshot(end_id, state.snapshot());

    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").expect("身份应有效");
    let bytes: Vec<u8> = SaveDocument::capture(&game, &state, &story)
        .and_then(|document: SaveDocument| document.to_bytes())
        .expect("历史 State 应可保存");
    let save: SaveDocument = SaveDocument::from_bytes(&bytes).expect("历史 State 应可解码");

    let _changed = state.variables_set("visits", Value::Number(9.0));
    save.restore(&game, &mut state, &mut story)
        .expect("历史 State 应可恢复");
    let map_id = story.back().expect("应可回到 Map").id();
    let map_state = story.state_snapshot(map_id).expect("Map 应保留进入前状态");
    state.restore_snapshot(map_state);

    assert_eq!(state.variables_get("visits"), Some(&Value::Number(1.0)));
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
    record_missing_history_states(&mut story, &state);
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
    record_missing_history_states(&mut story, &state);
    let game: GameIdentity = GameIdentity::new("example.save", "1.2.3").expect("身份应有效");

    let save: SaveDocument = SaveDocument::capture(&game, &state, &story).expect("循环图应可捕获");
    let bytes: Vec<u8> = save.to_bytes().expect("循环图不应递归展开");
    let decoded: SaveDocument = SaveDocument::from_bytes(&bytes).expect("二进制存档应有效");
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
fn save_rejects_a_damaged_binary_document_before_runtime_mutation() {
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
    let _changed = state.variables_set("items", Value::string("active"));
    let error: SaveError =
        SaveDocument::from_bytes(b"NRSAVE\0\x02\xff").expect_err("损坏 payload 不得解码");

    assert!(matches!(error, SaveError::Decode { .. }));
    assert_eq!(state.variables_get("items"), Some(&Value::string("active")));
    assert_eq!(story.current().map(|passage| passage.name), Some("Start"));
    assert_eq!(error.diagnostic().code, "save.decode");
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

fn record_missing_history_states(story: &mut Story<'_, '_>, state: &State) {
    let ids: Vec<_> = story.history().iter().map(|entry| entry.id()).collect();
    for id in ids {
        if story.state_snapshot(id).is_none() {
            story.record_state_snapshot(id, state.snapshot());
        }
    }
}

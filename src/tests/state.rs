//! State 命名空间与 Expression Context 契约测试。

use crate::{
    expression::{
        VariableScope,
        evaluator::{
            ContextWriteError, EvaluationContext, WritableEvaluationContext, evaluate_with_mut,
        },
        parse,
        value::{ArrayValue, Value},
    },
    state::{State, StateCheckpoint, StateReset, StateSnapshot},
};

#[test]
fn state_routes_each_namespace_and_rejects_macro_locals() {
    let mut state: State = State::new();

    state
        .set_global("formatName", Value::string("Maple"))
        .expect("global 应可写入");
    state
        .set_setup(Value::object(vec![(
            String::from("difficulty"),
            Value::string("normal"),
        )]))
        .expect("setup 应可写回");
    state
        .set_variable(VariableScope::Variables, "score", Value::Number(8.0))
        .expect("$ 变量应可写入");
    state
        .set_variable(VariableScope::Temporary, "turn", Value::Number(2.0))
        .expect("_ 变量应可写入");

    assert_eq!(state.global("formatName"), Some(&Value::string("Maple")));
    assert_eq!(
        state.variable(VariableScope::Variables, "score"),
        Some(&Value::Number(8.0))
    );
    assert_eq!(
        state.variable(VariableScope::Temporary, "turn"),
        Some(&Value::Number(2.0))
    );
    assert!(state.setup().is_some());

    let local_error: ContextWriteError = state
        .set_variable(VariableScope::Local, "item", Value::Number(1.0))
        .expect_err("@ 局部变量不属于 State");
    assert_eq!(local_error, ContextWriteError::Rejected);

    let removed: Option<Value> = state
        .del_variable(VariableScope::Temporary, "turn")
        .expect("_ 变量应可删除");
    assert_eq!(removed, Some(Value::Number(2.0)));
    assert_eq!(state.variable(VariableScope::Temporary, "turn"), None);
}

#[test]
fn state_exposes_direct_namespace_apis() {
    let mut state: State = State::new();

    let global_previous: Option<Value> = state.global_set("weather", Value::string("sunny"));
    let variable_previous: Option<Value> = state.variables_set("score", Value::Number(3.0));
    let temporary_previous: Option<Value> = state.temporary_set("selection", Value::string("Map"));
    let setup_previous: Value = state.setup_set(Value::object(vec![(
        String::from("debug"),
        Value::Boolean(false),
    )]));

    assert_eq!(global_previous, None);
    assert_eq!(variable_previous, None);
    assert_eq!(temporary_previous, None);
    assert_eq!(setup_previous, Value::object(Vec::new()));
    assert!(state.global_has("weather"));
    assert!(state.variables_has("score"));
    assert!(state.temporary_has("selection"));
    assert_eq!(state.global_get("weather"), Some(&Value::string("sunny")));
    assert_eq!(state.variables_get("score"), Some(&Value::Number(3.0)));
    assert_eq!(
        state.temporary_get("selection"),
        Some(&Value::string("Map"))
    );
    assert_eq!(
        state.setup_get(),
        &Value::object(vec![(String::from("debug"), Value::Boolean(false))])
    );

    let removed_global: Option<Value> = state.global_del("weather");
    let removed_variable: Option<Value> = state.variables_del("score");
    let removed_temporary: Option<Value> = state.temporary_del("selection");

    assert_eq!(removed_global, Some(Value::string("sunny")));
    assert_eq!(removed_variable, Some(Value::Number(3.0)));
    assert_eq!(removed_temporary, Some(Value::string("Map")));
    assert!(!state.global_has("weather"));
    assert!(!state.variables_has("score"));
    assert!(!state.temporary_has("selection"));
}

#[test]
fn clearing_temporary_preserves_other_state_namespaces() {
    let mut state: State = State::new();
    let _global_previous: Option<Value> = state.global_set("api", Value::Boolean(true));
    let _variable_previous: Option<Value> = state.variables_set("score", Value::Number(9.0));
    let _temporary_first: Option<Value> = state.temporary_set("turn", Value::Number(2.0));
    let _temporary_second: Option<Value> = state.temporary_set("target", Value::string("Map"));
    let _setup_previous: Value = state.setup_set(Value::object(vec![(
        String::from("difficulty"),
        Value::string("normal"),
    )]));

    let cleared: usize = state.temporary_clear();

    assert_eq!(cleared, 2);
    assert!(!state.temporary_has("turn"));
    assert!(!state.temporary_has("target"));
    assert_eq!(state.global_get("api"), Some(&Value::Boolean(true)));
    assert_eq!(state.variables_get("score"), Some(&Value::Number(9.0)));
    assert_eq!(
        state.setup_get(),
        &Value::object(vec![(String::from("difficulty"), Value::string("normal"))])
    );
    assert_eq!(state.temporary_clear(), 0);
}

#[test]
fn state_snapshot_restores_detached_variables_and_clears_temporary() {
    let shared_array: ArrayValue = ArrayValue::new(vec![Value::Number(1.0)]);
    let shared_value: Value = Value::Array(shared_array.clone());
    let mut state: State = State::new();
    let _left_previous: Option<Value> = state.variables_set("left", shared_value.clone());
    let _right_previous: Option<Value> = state.variables_set("right", shared_value);
    let _global_previous: Option<Value> = state.global_set("version", Value::Number(1.0));
    let _setup_previous: Value = state.setup_set(Value::string("initial"));
    let _temporary_previous: Option<Value> = state.temporary_set("turn", Value::Number(1.0));

    let snapshot: StateSnapshot = state.snapshot();

    assert_eq!(snapshot.variables_len(), 2);
    assert!(snapshot.variables_has("left"));
    assert!(snapshot.variables_get("right").is_some());

    let mutation = parse("$left.push(2)").expect("Array 修改应可解析");
    let _result: Value = evaluate_with_mut(&mutation, &mut state).expect("原 State 应可修改");
    let _extra_previous: Option<Value> = state.variables_set("extra", Value::Boolean(true));
    let _global_replaced: Option<Value> = state.global_set("version", Value::Number(2.0));
    let _setup_replaced: Value = state.setup_set(Value::string("reloaded"));
    let _temporary_replaced: Option<Value> = state.temporary_set("turn", Value::Number(2.0));

    state.restore(snapshot);

    let Some(Value::Array(left)) = state.variables_get("left") else {
        unreachable!("恢复后的 left 应为 Array")
    };
    let Some(Value::Array(right)) = state.variables_get("right") else {
        unreachable!("恢复后的 right 应为 Array")
    };

    assert_eq!(left.len(), 1);
    assert!(left.same_identity(right));
    assert!(!left.same_identity(&shared_array));
    assert_eq!(state.variables_get("extra"), None);
    assert_eq!(state.temporary_get("turn"), None);
    assert_eq!(state.global_get("version"), Some(&Value::Number(2.0)));
    assert_eq!(state.setup_get(), &Value::string("reloaded"));
}

#[test]
fn resetting_game_clears_gameplay_state_and_preserves_startup_state() {
    let mut state: State = State::new();
    let _global_previous: Option<Value> = state.global_set("api", Value::string("ready"));
    let _setup_previous: Value = state.setup_set(Value::object(vec![(
        String::from("difficulty"),
        Value::string("normal"),
    )]));
    let _score_previous: Option<Value> = state.variables_set("score", Value::Number(8.0));
    let _place_previous: Option<Value> = state.variables_set("place", Value::string("Map"));
    let _turn_previous: Option<Value> = state.temporary_set("turn", Value::Number(2.0));

    let reset: StateReset = state.reset_game();

    assert_eq!(reset.variables_removed, 2);
    assert_eq!(reset.temporary_removed, 1);
    assert!(!state.variables_has("score"));
    assert!(!state.variables_has("place"));
    assert!(!state.temporary_has("turn"));
    assert_eq!(state.global_get("api"), Some(&Value::string("ready")));
    assert_eq!(
        state.setup_get(),
        &Value::object(vec![(String::from("difficulty"), Value::string("normal"))])
    );

    let empty_reset: StateReset = state.reset_game();
    assert_eq!(empty_reset.variables_removed, 0);
    assert_eq!(empty_reset.temporary_removed, 0);
}

#[test]
fn state_checkpoint_restores_every_namespace_as_one_detached_graph() {
    let shared_array: ArrayValue = ArrayValue::new(vec![Value::Number(1.0)]);
    let shared_value: Value = Value::Array(shared_array.clone());
    let mut state: State = State::new();
    let _global_previous: Option<Value> = state.global_set("shared", shared_value.clone());
    let _setup_previous: Value = state.setup_set(shared_value.clone());
    let _variable_previous: Option<Value> = state.variables_set("shared", shared_value.clone());
    let _temporary_previous: Option<Value> = state.temporary_set("shared", shared_value);

    let checkpoint: StateCheckpoint = state.checkpoint();
    let mutation = parse("$shared.push(2)").expect("共享 Array 修改应可解析");
    let _result: Value = evaluate_with_mut(&mutation, &mut state).expect("活动 State 应可修改");
    let _global_replaced: Option<Value> = state.global_set("shared", Value::string("changed"));
    let _setup_replaced: Value = state.setup_set(Value::string("changed"));
    let _variable_replaced: Option<Value> = state.variables_set("shared", Value::string("changed"));
    let _temporary_replaced: Option<Value> =
        state.temporary_set("shared", Value::string("changed"));

    state.restore_checkpoint(checkpoint);

    let Some(Value::Array(global)) = state.global_get("shared") else {
        unreachable!("恢复后的 global 应为 Array")
    };
    let Value::Array(setup) = state.setup_get() else {
        unreachable!("恢复后的 setup 应为 Array")
    };
    let Some(Value::Array(variable)) = state.variables_get("shared") else {
        unreachable!("恢复后的 variable 应为 Array")
    };
    let Some(Value::Array(temporary)) = state.temporary_get("shared") else {
        unreachable!("恢复后的 temporary 应为 Array")
    };

    assert_eq!(global.len(), 1);
    assert!(global.same_identity(setup));
    assert!(global.same_identity(variable));
    assert!(global.same_identity(temporary));
    assert!(!global.same_identity(&shared_array));
}

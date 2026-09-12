use crate::{
    expression::value::Value,
    inspect::{StateInspection, inspect_state},
    state::State,
};

#[test]
fn inspection_detaches_data_and_marks_cycles_without_mutating_state() {
    let mut state: State = State::new();
    let object: Value = Value::object(vec![("coins".into(), Value::Number(8.0))]);
    let Value::Object(reference) = &object else {
        unreachable!()
    };
    reference.insert("self", object.clone());
    state.variables_set("hero", object.clone());
    let mut preview: StateInspection = inspect_state(&state);
    assert_eq!(preview.data["variables"]["hero"]["coins"], 8.0);
    assert_eq!(preview.data["variables"]["hero"]["self"], "[循环引用]");
    preview.data["variables"]["hero"]["coins"] = serde_json::json!(99);
    assert_eq!(reference.get("coins"), Some(Value::Number(8.0)));
    assert!(reference.get("self").is_some());
}

#[test]
fn inspection_bounds_large_and_deep_values_and_handles_non_json_values() {
    let mut state: State = State::new();
    let values: Vec<Value> = (0..10_000)
        .map(|index| Value::Number(f64::from(index)))
        .collect();
    state.variables_set("large", Value::array(values));
    state.variables_set("undefined", Value::Undefined);
    state.variables_set("nan", Value::Number(f64::NAN));
    let mut deep: Value = Value::Null;
    for _ in 0..50 {
        deep = Value::array(vec![deep]);
    }
    state.temporary_set("deep", deep);
    let preview: StateInspection = inspect_state(&state);
    assert!(preview.truncated);
    assert!(preview.data["variables"]["large"].as_array().unwrap().len() <= 129);
    assert_eq!(preview.data["variables"]["undefined"], "[undefined]");
    assert_eq!(preview.data["variables"]["nan"], "[NaN]");
    assert!(serde_json::to_string(&preview.data).unwrap().len() < 10_000);
}

#[test]
fn inspection_bounds_field_names_and_decodes_only_a_string_prefix() {
    let mut state: State = State::new();
    state.variables_set(&"key".repeat(100_000), Value::Null);
    state.variables_set("text", Value::string("文".repeat(100_000)));
    let preview: StateInspection = inspect_state(&state);
    assert!(preview.truncated);
    assert_eq!(preview.data["variables"].as_object().unwrap().len(), 1);
    assert!(
        preview.data["variables"]["text"]
            .as_str()
            .unwrap()
            .ends_with("[已省略]")
    );
    assert!(serde_json::to_string(&preview.data).unwrap().len() < 10_000);
}

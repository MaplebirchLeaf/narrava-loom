//! Expression Value 行为测试。

use crate::expression::value::{
    ArrayValue, NativeCallable, NativeMethod, TextValue, Value, ValueReference,
};

#[test]
fn cloned_value_references_keep_identity_and_shared_mutation() {
    let original: ValueReference<Vec<Value>> = ValueReference::new(vec![Value::Number(1.0)]);
    let cloned: ValueReference<Vec<Value>> = original.clone();

    assert!(original.same_identity(&cloned));
    cloned.with_mut(|values: &mut Vec<Value>| values.push(Value::Number(2.0)));
    let observed: Vec<Value> = original.with(|values: &Vec<Value>| values.clone());

    assert_eq!(observed, vec![Value::Number(1.0), Value::Number(2.0)]);
}

#[test]
fn detached_clone_copies_reference_graph_without_losing_cycles() {
    let original_array: ArrayValue = ArrayValue::new(Vec::new());
    original_array.with_mut(|values: &mut Vec<Value>| {
        values.push(Value::Array(original_array.clone()));
    });
    let original: Value = Value::Array(original_array.clone());

    let detached: Value = original.detached_clone();
    let Value::Array(detached_array) = &detached else {
        unreachable!("Array 脱离克隆后仍应是 Array")
    };
    let detached_items: Vec<Value> = detached_array.snapshot();
    let Value::Array(detached_cycle) = &detached_items[0] else {
        unreachable!("循环引用应保留 Array 类型")
    };

    assert!(!original_array.same_identity(detached_array));
    assert!(detached_array.same_identity(detached_cycle));

    original_array.with_mut(|values: &mut Vec<Value>| values.push(Value::Number(2.0)));

    assert_eq!(original_array.len(), 2);
    assert_eq!(detached_array.len(), 1);
}

#[test]
fn text_value_preserves_utf16_units_and_surrogate_slices() {
    let text: TextValue = TextValue::from("😀Narrava");

    assert_eq!(text.len(), 9);
    assert_eq!(text.as_units()[0..2], [0xD83D, 0xDE00]);
    assert_eq!(text.slice_units(0, 1).as_units(), [0xD83D]);
    assert_eq!(text.slice_units(1, 2).as_units(), [0xDE00]);
    assert_eq!(
        text.slice_units(2, 9).to_unicode_string(),
        Some(String::from("Narrava"))
    );
}

#[test]
fn keeps_null_and_undefined_as_distinct_values() {
    let null: Value = Value::Null;
    let undefined: Value = Value::Undefined;

    assert_ne!(null, undefined);
    assert!(null.is_nullish());
    assert!(undefined.is_nullish());
}

#[test]
fn keeps_scalar_payloads() {
    let boolean: Value = Value::Boolean(true);
    let number: Value = Value::Number(12.5);
    let string: Value = Value::string("Narrava");

    assert_eq!(boolean, Value::Boolean(true));
    assert_eq!(number, Value::Number(12.5));
    assert_eq!(string, Value::string("Narrava"));
    assert!(!number.is_nullish());
}

#[test]
fn keeps_array_and_object_payloads_in_source_order() {
    let array: Value = Value::array(vec![Value::Number(1.0), Value::Boolean(true)]);
    let object: Value = Value::object(vec![
        (String::from("name"), Value::string("Narrava")),
        (String::from("items"), array.clone()),
    ]);

    assert_eq!(
        object,
        Value::object(vec![
            (String::from("name"), Value::string("Narrava")),
            (String::from("items"), array),
        ])
    );
}

#[test]
fn follows_web_truthiness_rules() {
    let falsy: [Value; 7] = [
        Value::Undefined,
        Value::Null,
        Value::Boolean(false),
        Value::Number(0.0),
        Value::Number(-0.0),
        Value::Number(f64::NAN),
        Value::string(""),
    ];
    let truthy: [Value; 4] = [
        Value::Boolean(true),
        Value::Number(1.0),
        Value::array(Vec::new()),
        Value::object(Vec::new()),
    ];

    assert!(falsy.iter().all(|value| !value.is_truthy()));
    assert!(truthy.iter().all(Value::is_truthy));
}

#[test]
fn exposes_narrava_typeof_names() {
    let callable: Value = Value::Callable(NativeCallable::bind(
        Value::array(Vec::new()),
        NativeMethod::ArrayIncludes,
    ));
    let cases: [(Value, &str); 8] = [
        (Value::Undefined, "undefined"),
        (Value::Null, "null"),
        (Value::Boolean(false), "boolean"),
        (Value::Number(1.0), "number"),
        (Value::string(""), "string"),
        (Value::array(Vec::new()), "array"),
        (Value::object(Vec::new()), "object"),
        (callable, "function"),
    ];

    for (value, expected) in cases {
        assert_eq!(value.type_name(), expected);
    }
}

#[test]
fn native_callable_is_truthy() {
    let callable: Value = Value::Callable(NativeCallable::bind(
        Value::string("Narrava"),
        NativeMethod::StringIncludes,
    ));

    assert!(callable.is_truthy());
}

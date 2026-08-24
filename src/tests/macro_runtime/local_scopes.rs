use super::*;

#[test]
fn isolates_nested_local_writes_and_reads_outer_values() {
    let mut locals: MacroLocalScopes<&str> = MacroLocalScopes::new();

    locals.enter();
    assert_eq!(locals.set("name", "outer"), Ok(None));
    locals.enter();

    assert_eq!(locals.get("name"), Some(&"outer"));
    assert_eq!(locals.set("name", "inner"), Ok(None));
    assert_eq!(locals.get("name"), Some(&"inner"));
    assert_eq!(locals.del("name"), Ok(Some("inner")));
    assert_eq!(locals.get("name"), Some(&"outer"));
    assert_eq!(locals.set("name", "inner"), Ok(None));

    assert!(locals.leave());
    assert_eq!(locals.get("name"), Some(&"outer"));
    assert!(locals.leave());
    assert_eq!(locals.get("name"), None);
}

#[test]
fn rejects_local_writes_without_an_active_call_scope() {
    let mut locals: MacroLocalScopes<u8> = MacroLocalScopes::new();

    assert_eq!(locals.set("value", 1), Err(MacroLocalError::NoActiveScope));
    assert_eq!(locals.del("value"), Err(MacroLocalError::NoActiveScope));
    assert!(!locals.leave());
}

#[test]
fn rejects_suspending_an_empty_scope_chain() {
    let mut scopes: MacroLocalScopes<Value> = MacroLocalScopes::new();

    let result = scopes.suspend();

    assert!(matches!(result, Err(MacroLocalError::NoActiveScope)));
    assert_eq!(scopes.args(), None);
}

#[test]
fn exposes_current_call_arguments_as_args_array() {
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(vec![Value::string("Maple"), Value::Number(2.0)]);
    let base: EmptyEvaluationContext = EmptyEvaluationContext;
    let context: MacroEvaluationContext<'_> = MacroEvaluationContext::new(&base, &locals);
    let expression = parse("@args[1]").expect("@args 索引应可解析");

    let value: Value = evaluate_with(&expression, &context).expect("@args 应可在调用中读取");

    assert_eq!(value, Value::Number(2.0));
}

#[test]
fn keeps_args_reserved_for_the_current_call() {
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());

    assert_eq!(
        locals.set("args", Value::Undefined),
        Err(MacroLocalError::ReservedName)
    );
    assert_eq!(locals.del("args"), Err(MacroLocalError::ReservedName));
}

#[test]
fn stores_widget_arguments_for_args_access() {
    let mut locals: MacroLocalScopes<&str> = MacroLocalScopes::new();

    locals.enter_call(vec!["Maple", "2", "extra"]);

    assert_eq!(locals.args(), Some(&["Maple", "2", "extra"][..]));
}

#[test]
fn restores_outer_arguments_after_nested_call() {
    let mut locals: MacroLocalScopes<&str> = MacroLocalScopes::new();

    locals.enter_call(vec!["A"]);
    locals.enter_call(vec!["B"]);
    assert_eq!(locals.args(), Some(&["B"][..]));

    assert!(locals.leave());
    assert_eq!(locals.args(), Some(&["A"][..]));
}

#[test]
fn captures_only_named_visible_locals_for_delayed_execution() {
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter();
    let _previous: Option<Value> = locals
        .set("outer", Value::string("Maple"))
        .expect("外层局部变量应可写入");
    locals.enter();
    let _previous: Option<Value> = locals
        .set("inner", Value::Number(2.0))
        .expect("内层局部变量应可写入");

    let captured: CapturedMacroLocals<Value> = locals.capture(&["outer", "missing"]);
    let restored: MacroLocalScopes<Value> = captured.into_scopes();

    assert_eq!(restored.get("outer"), Some(&Value::string("Maple")));
    assert_eq!(restored.get("inner"), None);
    assert_eq!(restored.get("missing"), None);
    assert_eq!(restored.args(), Some(&[][..]));
}

#[test]
fn captured_object_keeps_its_narrava_reference_identity() {
    let object: Value = Value::object(vec![(String::from("count"), Value::Number(1.0))]);
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter();
    let _previous: Option<Value> = locals
        .set("shared", object.clone())
        .expect("局部对象应可写入");

    let captured: CapturedMacroLocals<Value> = locals.capture(&["shared"]);
    let restored: MacroLocalScopes<Value> = captured.into_scopes();

    assert_eq!(restored.get("shared"), Some(&object));
}

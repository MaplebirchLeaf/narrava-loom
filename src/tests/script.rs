//! Script Bundle 与叙事源码分流测试。

use crate::{
    SourceList,
    expression::{
        evaluator::evaluate_with_mut,
        parse,
        value::{ScriptCallable, Value},
    },
    script::{ScriptBundle, ScriptLanguage},
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

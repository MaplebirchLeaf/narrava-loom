use std::cell::RefCell;

use super::*;

#[test]
fn lifecycle_subscriptions_keep_registration_order_and_can_be_removed() {
    let mut subscriptions: MacroLifecycleSubscriptions<&str> = MacroLifecycleSubscriptions::new();

    let first: MacroLifecycleSubscriptionId = subscriptions
        .before("choice", "before:first")
        .expect("普通 Macro 应可注册 before");
    let _second: MacroLifecycleSubscriptionId = subscriptions
        .before("choice", "before:second")
        .expect("同一 Macro 应可注册多个 before");
    let _after: MacroLifecycleSubscriptionId = subscriptions
        .after("choice", "after:first")
        .expect("普通 Macro 应可注册 after");

    assert_eq!(first.as_u64(), 1);
    assert_eq!(
        subscriptions.before_hooks("choice").collect::<Vec<_>>(),
        vec![&"before:first", &"before:second"]
    );
    assert_eq!(
        subscriptions.after_hooks("choice").collect::<Vec<_>>(),
        vec![&"after:first"]
    );
    assert_eq!(subscriptions.off(first), Some("before:first"));
    assert_eq!(
        subscriptions.before_hooks("choice").collect::<Vec<_>>(),
        vec![&"before:second"]
    );
}

#[test]
fn lifecycle_subscriptions_reject_compiler_owned_logic_macros() {
    for name in [
        "if", "elseif", "else", "switch", "case", "default", "for", "while", "break", "continue",
        "set", "unset", "run", "include", "goto", "print", "silently", "return", "capture", "exit",
        "widget",
    ] {
        let mut subscriptions: MacroLifecycleSubscriptions<()> = MacroLifecycleSubscriptions::new();
        let error: MacroLifecycleSubscriptionError = subscriptions
            .before(name, ())
            .expect_err("编译期固有逻辑语法不能注册 Macro Hook");

        assert_eq!(
            error,
            MacroLifecycleSubscriptionError::CompilerOwnedMacro(name.to_owned())
        );
        assert_eq!(error.diagnostic().code, "macro.lifecycle.compiler_owned");
    }
}

#[test]
fn lifecycle_subscription_names_remain_case_sensitive() {
    let mut subscriptions: MacroLifecycleSubscriptions<&str> = MacroLifecycleSubscriptions::new();
    let _custom: MacroLifecycleSubscriptionId = subscriptions
        .before("If", "custom")
        .expect("大写 If 不是编译器的小写 if 语法");

    assert_eq!(
        subscriptions.before_hooks("If").collect::<Vec<_>>(),
        vec![&"custom"]
    );
    assert_eq!(subscriptions.before_hooks("if").next(), None);
}

#[test]
fn lifecycle_controller_executes_matching_subscriptions_in_order() {
    let mut subscriptions: MacroLifecycleSubscriptions<&str> = MacroLifecycleSubscriptions::new();
    let _first: MacroLifecycleSubscriptionId = subscriptions
        .before("greet", "first")
        .expect("普通 Macro 应可注册 before");
    let _second: MacroLifecycleSubscriptionId = subscriptions
        .before("greet", "second")
        .expect("普通 Macro 应可注册第二个 before");
    let _after: MacroLifecycleSubscriptionId = subscriptions
        .after("greet", "suffix")
        .expect("普通 Macro 应可注册 after");
    let order: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let mut controller: MacroLifecycleController<'_, _, _, _> = MacroLifecycleController::new(
        &subscriptions,
        |hook: &&str, _name: &str, arguments: &mut [Value]| {
            order.borrow_mut().push((*hook).to_owned());
            arguments[0] = Value::string(*hook);
            Ok(())
        },
        |hook: &&str,
         _name: &str,
         _arguments: &[Value],
         mut output: crate::semantic::SemanticOutput| {
            order.borrow_mut().push((*hook).to_owned());
            output.push(SemanticNode::Text(TextValue::from(*hook)));
            Ok(output)
        },
    );
    let mut arguments: Vec<Value> = vec![Value::string("initial")];

    controller
        .before("greet", &mut arguments)
        .expect("before 序列应完成");
    let output: crate::semantic::SemanticOutput = controller
        .after(
            "greet",
            &arguments,
            crate::semantic::SemanticOutput::default(),
        )
        .expect("after 序列应完成");

    assert_eq!(arguments, vec![Value::string("second")]);
    assert_eq!(
        output.nodes(),
        &[SemanticNode::Text(TextValue::from("suffix"))]
    );
    assert_eq!(
        order.into_inner(),
        vec![
            String::from("first"),
            String::from("second"),
            String::from("suffix"),
        ]
    );
}

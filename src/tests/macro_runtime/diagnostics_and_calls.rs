use super::*;
use std::cell::RefCell;

#[test]
fn converts_macro_errors_to_stable_diagnostics() {
    let registry: Diagnostic = MacroDefinitionError::MissingDefinition.diagnostic("weather");
    let local: Diagnostic = MacroLocalError::NoActiveScope.diagnostic();
    let reserved: Diagnostic = MacroLocalError::ReservedName.diagnostic();

    assert_eq!(registry.code, "macro.missing_definition");
    assert_eq!(registry.severity, DiagnosticSeverity::Error);
    assert_eq!(registry.message, "Macro `weather` 尚未注册");
    assert_eq!(registry.location, None);
    assert_eq!(local.code, "macro.no_active_local_scope");
    assert_eq!(local.message, "当前没有活动的 Macro Local Scope");
    assert_eq!(reserved.code, "macro.reserved_local_name");
}

#[test]
fn converts_dispatch_contract_and_handler_errors_to_diagnostics() {
    let body: Diagnostic = MacroDispatchError::<&str, u64>::BodyKindMismatch {
        expected: MacroBodyKind::Inline,
        actual: MacroBodyKind::Container,
    }
    .diagnostic("sample", |_| unreachable!("结构错误不应进入 Handler 映射"));
    let pending: Diagnostic = MacroDispatchError::<&str, u64>::UnexpectedPending(17)
        .diagnostic("sample", |_| unreachable!("执行错误不应进入 Handler 映射"));
    let handler: Diagnostic = MacroDispatchError::<&str, u64>::Handler("failed")
        .diagnostic("sample", |message: &str| {
            Diagnostic::new("macro.handler_failed", DiagnosticSeverity::Error, message)
        });

    assert_eq!(body.code, "macro.body_kind_mismatch");
    assert_eq!(
        body.message,
        "Macro `sample` 定义为 Inline，但调用提供了 Container 正文"
    );
    assert_eq!(pending.code, "macro.unexpected_pending");
    assert_eq!(
        pending.message,
        "Macro `sample` 声明为 Sync，但 Handler 返回了 Pending"
    );
    assert_eq!(handler.code, "macro.handler_failed");
    assert_eq!(handler.message, "failed");
}

#[test]
fn prepares_registered_argument_list_call() {
    let mut definitions: MacroDefinitions<MacroDefinition<&str>> = MacroDefinitions::new();
    let _: Option<MacroDefinition<&str>> = definitions.add(
        "sample",
        MacroDefinition::new(
            MacroBodyKind::Inline,
            MacroArgumentKind::ArgumentList,
            MacroExecutionKind::Sync,
            "handler",
        ),
    );
    let context: EmptyEvaluationContext = EmptyEvaluationContext;

    let call: PreparedMacroCall<'_, &str> =
        prepare_macro_call(&definitions, "sample", "1 (2 + 3)", |expression| {
            evaluate_with(expression, &context)
        })
        .expect("已注册调用应完成准备");

    assert_eq!(call.name, "sample");
    assert_eq!(call.raw_arguments, "1 (2 + 3)");
    assert_eq!(call.arguments, vec![Value::Number(1.0), Value::Number(5.0)]);
    assert_eq!(call.definition.handler, "handler");
}

#[test]
fn raw_call_skips_expression_parsing_and_evaluation() {
    let mut definitions: MacroDefinitions<MacroDefinition<&str>> = MacroDefinitions::new();
    let _: Option<MacroDefinition<&str>> = definitions.add(
        "raw",
        MacroDefinition::new(
            MacroBodyKind::Inline,
            MacroArgumentKind::Raw,
            MacroExecutionKind::Sync,
            "handler",
        ),
    );
    let mut evaluated: bool = false;

    let call: PreparedMacroCall<'_, &str> =
        prepare_macro_call(&definitions, "raw", "not valid expression ;", |_| {
            evaluated = true;
            Ok::<Value, EvalError>(Value::Undefined)
        })
        .expect("Raw 参数应保持原文");

    assert!(call.arguments.is_empty());
    assert!(!evaluated);
}

#[test]
fn rejects_missing_definition_before_argument_evaluation() {
    let definitions: MacroDefinitions<MacroDefinition<&str>> = MacroDefinitions::new();
    let mut evaluated: bool = false;

    let result: Result<PreparedMacroCall<'_, &str>, MacroCallPreparationError<EvalError>> =
        prepare_macro_call(&definitions, "missing", "1", |_| {
            evaluated = true;
            Ok(Value::Undefined)
        });

    assert!(matches!(
        result,
        Err(MacroCallPreparationError::Definition(
            MacroDefinitionError::MissingDefinition
        ))
    ));
    assert!(!evaluated);
}

#[test]
fn preparation_issue_keeps_definition_error_without_fake_argument_span() {
    let error: MacroCallPreparationError<EvalError> =
        MacroCallPreparationError::Definition(MacroDefinitionError::MissingDefinition);

    let issue: MacroCallPreparationIssue = error.issue("missing", 3);

    assert_eq!(issue.diagnostic.code, "macro.missing_definition");
    assert_eq!(issue.span, None);
}

#[test]
fn preparation_issue_keeps_argument_parse_span_and_twee_location() {
    let error: MacroArgumentListError = parse_argument_list("1 $").expect_err("参数应解析失败");
    let preparation: MacroCallPreparationError<EvalError> =
        MacroCallPreparationError::ArgumentList(error);
    let issue: MacroCallPreparationIssue = preparation.issue("sample", 3);
    let locator: DiagnosticLocator<'_> = DiagnosticLocator::new("story/main.twee", "X\n<<x 1 $>>");
    let diagnostic: Diagnostic = issue
        .locate(&locator, 6)
        .expect("参数片段位置应映射回 Twee Source");

    assert_eq!(diagnostic.code, "expression.invalid_variable");
    assert_eq!(diagnostic.location.expect("应包含位置").start, 8);
}

#[test]
fn preparation_issue_keeps_argument_evaluation_span() {
    let preparation: MacroCallPreparationError<EvalError> =
        MacroCallPreparationError::ArgumentValue(MacroArgumentValueError::Expression {
            error: EvalError::UnknownGlobal(Span { start: 1, end: 4 }),
            offset: 10,
        });

    let issue: MacroCallPreparationIssue = preparation.issue("sample", 20);

    assert_eq!(issue.diagnostic.code, "expression.unknown_global");
    assert_eq!(issue.span, Some(Span { start: 11, end: 14 }));
}

#[test]
fn complete_prepared_call_removes_only_its_own_frame() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::ArgumentList,
        MacroExecutionKind::Sync,
        "handler",
    );
    let prepared: PreparedMacroCall<'_, &str> = PreparedMacroCall {
        name: "sample",
        raw_arguments: "1",
        arguments: vec![Value::Number(1.0)],
        definition: &definition,
    };
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(vec![Value::string("outer")]);
    let mut context: () = ();

    let outcome: MacroCallOutcome<Value, u64> = execute_prepared_macro(
        prepared,
        RuntimeExecutionIdentity::new(1, 2),
        MacroInvocationBody::<()>::Inline,
        &mut context,
        &mut locals,
        |_, invocation| {
            assert_eq!(invocation.arguments, &[Value::Number(1.0)]);
            Ok::<MacroHandlerOutcome<Value, u64>, &str>(MacroHandlerOutcome::Complete(
                Value::string("done"),
            ))
        },
    )
    .expect("同步调用应完成");

    assert_eq!(outcome, MacroCallOutcome::Complete(Value::string("done")));
    assert_eq!(locals.args(), Some([Value::string("outer")].as_slice()));
}

#[test]
fn failed_prepared_call_removes_its_frame() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::Raw,
        MacroExecutionKind::Sync,
        "handler",
    );
    let prepared: PreparedMacroCall<'_, &str> = PreparedMacroCall {
        name: "sample",
        raw_arguments: "",
        arguments: Vec::new(),
        definition: &definition,
    };
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut context: () = ();

    let result = execute_prepared_macro(
        prepared,
        RuntimeExecutionIdentity::new(1, 2),
        MacroInvocationBody::<()>::Inline,
        &mut context,
        &mut locals,
        |_, _| Err::<MacroHandlerOutcome<Value, u64>, &str>("failed"),
    );

    assert!(matches!(result, Err(MacroDispatchError::Handler("failed"))));
    assert_eq!(locals.args(), None);
}

#[test]
fn pending_prepared_call_transfers_the_whole_scope_chain() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::Raw,
        MacroExecutionKind::Async,
        "handler",
    );
    let prepared: PreparedMacroCall<'_, &str> = PreparedMacroCall {
        name: "sample",
        raw_arguments: "",
        arguments: Vec::new(),
        definition: &definition,
    };
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(vec![Value::string("outer")]);
    let mut context: () = ();

    let outcome: MacroCallOutcome<Value, u64> = execute_prepared_macro(
        prepared,
        RuntimeExecutionIdentity::new(1, 2),
        MacroInvocationBody::<()>::Inline,
        &mut context,
        &mut locals,
        |_, _| Ok::<MacroHandlerOutcome<Value, u64>, &str>(MacroHandlerOutcome::Pending(29)),
    )
    .expect("Async Handler 应可暂停");
    let MacroCallOutcome::Pending(suspension) = outcome else {
        panic!("调用应返回 Pending");
    };
    let restored: MacroLocalScopes<Value> = suspension.scopes.into_scopes();

    assert_eq!(suspension.identity, RuntimeExecutionIdentity::new(1, 2));
    assert_eq!(suspension.handle, 29);
    assert_eq!(locals.args(), None);
    assert_eq!(restored.args(), Some([].as_slice()));
}

#[test]
fn resumed_call_completes_and_returns_outer_scopes() {
    let mut scopes: MacroLocalScopes<Value> = MacroLocalScopes::new();
    scopes.enter_call(vec![Value::string("outer")]);
    scopes.enter_call(vec![Value::string("inner")]);
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(4, 7);
    let suspension: MacroSuspension<u64> = MacroSuspension {
        identity,
        handle: 29,
        scopes: scopes.suspend().expect("活动作用域应可暂停"),
    };

    let outcome: MacroResumeOutcome<Value, u64> =
        resume_macro_suspension(identity, suspension, |_handle, active| {
            assert_eq!(active.args(), Some([Value::string("inner")].as_slice()));
            Ok::<MacroHandlerOutcome<Value, u64>, &str>(MacroHandlerOutcome::Complete(
                Value::string("done"),
            ))
        })
        .expect("恢复调用应完成");
    let MacroResumeOutcome::Complete { output, scopes } = outcome else {
        panic!("恢复调用应返回 Complete");
    };

    assert_eq!(output, Value::string("done"));
    assert_eq!(scopes.args(), Some([Value::string("outer")].as_slice()));
}

#[test]
fn resumed_call_can_suspend_again_without_losing_scopes() {
    let mut scopes: MacroLocalScopes<Value> = MacroLocalScopes::new();
    scopes.enter_call(vec![Value::string("inner")]);
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(4, 7);
    let suspension: MacroSuspension<u64> = MacroSuspension {
        identity,
        handle: 29,
        scopes: scopes.suspend().expect("活动作用域应可暂停"),
    };

    let outcome: MacroResumeOutcome<Value, u64> =
        resume_macro_suspension(identity, suspension, |_handle, _active| {
            Ok::<MacroHandlerOutcome<Value, u64>, &str>(MacroHandlerOutcome::Pending(31))
        })
        .expect("恢复调用应可再次暂停");
    let MacroResumeOutcome::Pending(suspension) = outcome else {
        panic!("恢复调用应再次返回 Pending");
    };
    let restored: MacroLocalScopes<Value> = suspension.scopes.into_scopes();

    assert_eq!(suspension.identity, identity);
    assert_eq!(suspension.handle, 31);
    assert_eq!(restored.args(), Some([Value::string("inner")].as_slice()));
}

#[test]
fn resumed_call_failure_removes_current_frame_and_returns_outer_scopes() {
    let mut scopes: MacroLocalScopes<Value> = MacroLocalScopes::new();
    scopes.enter_call(vec![Value::string("outer")]);
    scopes.enter_call(vec![Value::string("inner")]);
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(4, 7);
    let suspension: MacroSuspension<u64> = MacroSuspension {
        identity,
        handle: 29,
        scopes: scopes.suspend().expect("活动作用域应可暂停"),
    };

    let error =
        resume_macro_suspension::<Value, u64, &str>(identity, suspension, |_handle, _active| {
            Err("failed")
        })
        .expect_err("恢复失败应返回错误及剩余作用域");
    let MacroResumeError::Resume(failure) = error else {
        panic!("回调失败应返回 Resume 错误");
    };

    assert_eq!(failure.error, "failed");
    assert_eq!(
        failure.scopes.args(),
        Some([Value::string("outer")].as_slice())
    );
}

#[test]
fn rejects_resume_for_a_different_execution_identity() {
    let actual: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(4, 7);
    let expected: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(5, 7);
    let mut scopes: MacroLocalScopes<Value> = MacroLocalScopes::new();
    scopes.enter_call(Vec::new());
    let suspension: MacroSuspension<u64> = MacroSuspension {
        identity: actual,
        handle: 29,
        scopes: scopes.suspend().expect("活动作用域应可暂停"),
    };
    let mut resumed: bool = false;

    let error =
        resume_macro_suspension::<Value, u64, &str>(expected, suspension, |_handle, _active| {
            resumed = true;
            Ok(MacroHandlerOutcome::Complete(Value::Undefined))
        })
        .expect_err("不同执行身份必须拒绝恢复");
    let diagnostic: Diagnostic = match &error {
        MacroResumeError::Identity(error) => error.diagnostic(),
        MacroResumeError::Resume(_) => panic!("身份不匹配不应成为恢复回调错误"),
    };
    let MacroResumeError::Identity(error) = error else {
        panic!("身份不匹配应返回 Identity 错误");
    };

    assert!(!resumed);
    assert_eq!(error.expected, expected);
    assert_eq!(error.suspension.identity, actual);
    assert_eq!(error.suspension.handle, 29);
    assert_eq!(diagnostic.code, "macro.resume_identity_mismatch");
    assert_eq!(
        diagnostic.message,
        "Macro 暂停属于 Story 4 的执行链 7，不能恢复到 Story 5 的执行链 7"
    );
}

#[test]
fn resume_error_maps_to_diagnostic_without_consuming_the_suspension() {
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(4, 7);
    let expected: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(5, 7);
    let mut scopes: MacroLocalScopes<Value> = MacroLocalScopes::new();
    scopes.enter_call(Vec::new());
    let suspension: MacroSuspension<u64> = MacroSuspension {
        identity,
        handle: 29,
        scopes: scopes.suspend().expect("活动作用域应可暂停"),
    };
    let error: MacroResumeError<&str, u64> =
        resume_macro_suspension::<Value, u64, &str>(expected, suspension, |_handle, _active| {
            panic!("身份不匹配时不应调用恢复回调")
        })
        .expect_err("错误执行身份必须被拒绝");

    let diagnostic: Diagnostic = error.diagnostic(|handler_error: &&str| {
        Diagnostic::new(
            "macro.resume_handler",
            DiagnosticSeverity::Error,
            handler_error,
        )
    });

    assert_eq!(diagnostic.code, "macro.resume_identity_mismatch");
    let MacroResumeError::Identity(identity_error) = error else {
        panic!("诊断转换后仍应保留身份错误及原暂停状态");
    };
    assert_eq!(identity_error.suspension.handle, 29);
}

#[test]
fn resume_handler_failure_uses_the_owner_diagnostic_mapper() {
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(4, 7);
    let mut scopes: MacroLocalScopes<Value> = MacroLocalScopes::new();
    scopes.enter_call(Vec::new());
    let suspension: MacroSuspension<u64> = MacroSuspension {
        identity,
        handle: 29,
        scopes: scopes.suspend().expect("活动作用域应可暂停"),
    };
    let error: MacroResumeError<&str, u64> =
        resume_macro_suspension::<Value, u64, &str>(identity, suspension, |_handle, _active| {
            Err("failed")
        })
        .expect_err("Handler 恢复失败应返回错误");

    let diagnostic: Diagnostic = error.diagnostic(|handler_error: &&str| {
        Diagnostic::new(
            "scripts.macro.resume_failed",
            DiagnosticSeverity::Error,
            handler_error,
        )
    });

    assert_eq!(diagnostic.code, "scripts.macro.resume_failed");
    assert_eq!(diagnostic.message, "failed");
}

#[test]
fn sync_lifecycle_shares_modified_args_and_transforms_handler_output() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::ArgumentList,
        MacroExecutionKind::Sync,
        "handler",
    );
    let prepared: PreparedMacroCall<'_, &str> = PreparedMacroCall {
        name: "sample",
        raw_arguments: "1",
        arguments: vec![Value::Number(1.0)],
        definition: &definition,
    };
    let mut context: () = ();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let order: RefCell<Vec<&str>> = RefCell::new(Vec::new());
    let before_hooks: [&str; 1] = ["before"];
    let after_hooks: [&str; 1] = ["after"];

    let output: Value = execute_prepared_sync_macro_with_lifecycle(
        prepared,
        MacroInvocationBody::<()>::Inline,
        MacroLifecycleExecutionContext::new(&mut context, &mut locals),
        MacroLifecycleHookSequence::new(before_hooks.iter(), after_hooks.iter()),
        |hook: &&str, active: &mut MacroLocalScopes<Value>, _context: &mut ()| {
            order.borrow_mut().push(*hook);
            active.args_mut().expect("before 应处于调用帧中")[0] = Value::Number(2.0);
            Ok::<(), &str>(())
        },
        |handler, invocation| {
            order.borrow_mut().push(handler);
            assert_eq!(invocation.arguments, &[Value::Number(2.0)]);
            Ok::<MacroHandlerOutcome<Value, u64>, &str>(MacroHandlerOutcome::Complete(
                Value::Number(3.0),
            ))
        },
        |hook: &&str, handler_output, active: &mut MacroLocalScopes<Value>, _context: &mut ()| {
            order.borrow_mut().push(*hook);
            assert_eq!(active.args(), Some([Value::Number(2.0)].as_slice()));
            assert_eq!(handler_output, Value::Number(3.0));
            Ok::<Value, &str>(Value::Number(4.0))
        },
    )
    .expect("同步生命周期应完整执行");

    assert_eq!(output, Value::Number(4.0));
    assert_eq!(order.into_inner(), vec!["before", "handler", "after"]);
    assert_eq!(locals.args(), None);
}

#[test]
fn sync_lifecycle_cleans_the_call_frame_when_before_fails() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::Raw,
        MacroExecutionKind::Sync,
        "handler",
    );
    let prepared: PreparedMacroCall<'_, &str> = PreparedMacroCall {
        name: "sample",
        raw_arguments: "",
        arguments: Vec::new(),
        definition: &definition,
    };
    let mut context: () = ();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let before_hooks: [&str; 1] = ["before"];
    let after_hooks: [&str; 0] = [];

    let error: MacroLifecycleError<&str, u64> = execute_prepared_sync_macro_with_lifecycle(
        prepared,
        MacroInvocationBody::<()>::Inline,
        MacroLifecycleExecutionContext::new(&mut context, &mut locals),
        MacroLifecycleHookSequence::new(before_hooks.iter(), after_hooks.iter()),
        |_hook, _active: &mut MacroLocalScopes<Value>, _context: &mut ()| Err("before failed"),
        |_handler, _invocation| panic!("before 失败后不能执行 Handler"),
        |_hook, _output: Value, _active: &mut MacroLocalScopes<Value>, _context: &mut ()| {
            panic!("before 失败后不能执行 after")
        },
    )
    .expect_err("before 失败应保留生命周期阶段");

    assert_eq!(error, MacroLifecycleError::Before("before failed"));
    assert_eq!(locals.args(), None);
}

#[test]
fn sync_lifecycle_cleans_the_call_frame_when_after_fails() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::Raw,
        MacroExecutionKind::Sync,
        "handler",
    );
    let prepared: PreparedMacroCall<'_, &str> = PreparedMacroCall {
        name: "sample",
        raw_arguments: "",
        arguments: Vec::new(),
        definition: &definition,
    };
    let mut context: () = ();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let before_hooks: [&str; 0] = [];
    let after_hooks: [&str; 1] = ["after"];

    let error: MacroLifecycleError<&str, u64> = execute_prepared_sync_macro_with_lifecycle(
        prepared,
        MacroInvocationBody::<()>::Inline,
        MacroLifecycleExecutionContext::new(&mut context, &mut locals),
        MacroLifecycleHookSequence::new(before_hooks.iter(), after_hooks.iter()),
        |_hook, _active: &mut MacroLocalScopes<Value>, _context: &mut ()| Ok(()),
        |_handler, _invocation| Ok(MacroHandlerOutcome::Complete(Value::Number(1.0))),
        |_hook, _output, _active: &mut MacroLocalScopes<Value>, _context: &mut ()| {
            Err("after failed")
        },
    )
    .expect_err("after 失败应保留生命周期阶段");

    assert_eq!(error, MacroLifecycleError::After("after failed"));
    assert_eq!(locals.args(), None);
}

#[test]
fn sync_lifecycle_rejects_async_definition_before_creating_a_frame() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::Raw,
        MacroExecutionKind::Async,
        "handler",
    );
    let prepared: PreparedMacroCall<'_, &str> = PreparedMacroCall {
        name: "sample",
        raw_arguments: "",
        arguments: Vec::new(),
        definition: &definition,
    };
    let mut context: () = ();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let before_hooks: [&str; 0] = [];
    let after_hooks: [&str; 0] = [];

    let error: MacroLifecycleError<&str, u64> = execute_prepared_sync_macro_with_lifecycle(
        prepared,
        MacroInvocationBody::<()>::Inline,
        MacroLifecycleExecutionContext::new(&mut context, &mut locals),
        MacroLifecycleHookSequence::new(before_hooks.iter(), after_hooks.iter()),
        |_hook, _active: &mut MacroLocalScopes<Value>, _context: &mut ()| Ok(()),
        |_handler, _invocation| Ok(MacroHandlerOutcome::Complete(Value::Undefined)),
        |_hook, output, _active: &mut MacroLocalScopes<Value>, _context: &mut ()| Ok(output),
    )
    .expect_err("Async Definition 不能进入同步生命周期");

    assert_eq!(error, MacroLifecycleError::AsyncDefinition);
    assert_eq!(locals.args(), None);
}

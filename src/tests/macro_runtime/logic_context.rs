use super::*;

#[test]
fn logic_context_routes_state_locals_and_story_independently() {
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);
    let state_write = parse("$count += 2").expect("State 写入表达式应可解析");
    let local_write = parse("@index = 4").expect("Macro Local 写入表达式应可解析");

    let _: Value = evaluate_with_mut(&state_write, &mut context).expect("$ 应写入 State");
    let _: Value = evaluate_with_mut(&local_write, &mut context).expect("@ 应写入 Local Scope");
    assert!(context.story().has("End"));
    context
        .story_mut()
        .include("Start")
        .expect("include 请求应成功");
    context.story_mut().goto("End").expect("goto 请求应成功");

    assert_eq!(
        context.state().variable(VariableScope::Variables, "count"),
        Some(&Value::Number(3.0))
    );
    assert_eq!(context.local("index"), Some(&Value::Number(4.0)));
    assert_eq!(context.story().included, vec![String::from("Start")]);
    assert_eq!(context.story().destination, Some(String::from("End")));
}

#[test]
fn logic_context_routes_root_deletion_to_its_actual_owner() {
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(3.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    locals
        .set("index", Value::Number(4.0))
        .expect("Local 测试绑定应建立");
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let state_value: Option<Value> = context
        .del_variable(VariableScope::Variables, "count")
        .expect("$ 变量应交给 State 删除");
    let local_value: Option<Value> = context
        .del_variable(VariableScope::Local, "index")
        .expect("@ 变量应交给 Macro Local 删除");
    let args_error: Result<Option<Value>, ContextWriteError> =
        context.del_variable(VariableScope::Local, "args");

    assert_eq!(state_value, Some(Value::Number(3.0)));
    assert_eq!(local_value, Some(Value::Number(4.0)));
    assert_eq!(context.local("index"), None);
    assert_eq!(args_error, Err(ContextWriteError::Rejected));
}

#[test]
fn prepared_logic_call_builds_context_after_entering_its_frame() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::ArgumentList,
        MacroExecutionKind::Sync,
        "handler",
    );
    let prepared: PreparedMacroCall<'_, &str> = PreparedMacroCall {
        name: "sample",
        raw_arguments: "2",
        arguments: vec![Value::Number(2.0)],
        definition: &definition,
    };
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();

    let outcome: MacroCallOutcome<Value, u64> = execute_prepared_logic_macro(
        prepared,
        RuntimeExecutionIdentity::new(1, 1),
        MacroInvocationBody::<()>::Inline,
        &mut state,
        &mut story,
        &mut locals,
        |_, invocation| {
            assert_eq!(invocation.arguments, &[Value::Number(2.0)]);
            assert_eq!(
                invocation.context.local("args"),
                Some(&Value::array(vec![Value::Number(2.0)]))
            );
            let state_write = parse("$count += 2").expect("State 写入应可解析");
            let local_write = parse("@index = 4").expect("Local 写入应可解析");
            let _: Value = evaluate_with_mut(&state_write, invocation.context)?;
            let _: Value = evaluate_with_mut(&local_write, invocation.context)?;
            invocation
                .context
                .story_mut()
                .goto("End")
                .expect("goto 请求应成功");
            Ok::<MacroHandlerOutcome<Value, u64>, EvalError>(MacroHandlerOutcome::Complete(
                Value::Undefined,
            ))
        },
    )
    .expect("逻辑 Macro 应完成");

    assert_eq!(outcome, MacroCallOutcome::Complete(Value::Undefined));
    assert_eq!(state.count, Value::Number(3.0));
    assert_eq!(story.destination, Some(String::from("End")));
    assert_eq!(locals.args(), None);
}

#[test]
fn run_executes_side_effects_and_discards_the_expression_value() {
    let expression = parse("$count += 2").expect("run 表达式应可解析");
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };

    let output: Value = run(&expression, &mut state).expect("run 应执行表达式");

    assert_eq!(output, Value::Undefined);
    assert_eq!(state.count, Value::Number(3.0));
}

#[test]
fn run_preserves_expression_errors_and_their_span() {
    let expression = parse("missing()").expect("run 表达式应可解析");
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };

    let error: EvalError = run(&expression, &mut state).expect_err("未知名称应求值失败");

    assert_eq!(error, EvalError::UnknownGlobal(Span { start: 0, end: 7 }));
}

#[test]
fn set_executes_the_normalized_assignment_and_returns_undefined() {
    let assignment = parse("$count = 8").expect("HIR 归一化后的 set 赋值应可解析");
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };

    let output: Value = set(&assignment, &mut state).expect("set 应写入 State");

    assert_eq!(output, Value::Undefined);
    assert_eq!(state.count, Value::Number(8.0));
}

#[test]
fn set_preserves_assignment_errors_and_their_span() {
    let assignment = parse("$missing = 8").expect("set 赋值应可解析");
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };

    let error: EvalError = set(&assignment, &mut state).expect_err("State 可拒绝未知变量写入");

    assert_eq!(
        error,
        EvalError::ContextWriteRejected(Span { start: 0, end: 8 })
    );
}

#[test]
fn unset_deletes_macro_local_and_returns_undefined() {
    let target = parse("@index").expect("unset Local 目标应可解析");
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    locals
        .set("index", Value::Number(4.0))
        .expect("Local 测试绑定应建立");
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let output: Value = unset(&target, &mut context).expect("unset 应删除 Local 绑定");

    assert_eq!(output, Value::Undefined);
    assert_eq!(context.local("index"), None);
}

#[test]
fn unset_preserves_reserved_args_error_span() {
    let target = parse("@args").expect("保留 Local 目标应可解析");
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(vec![Value::Number(2.0)]);
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let error: EvalError = unset(&target, &mut context).expect_err("@args 不允许删除");

    assert_eq!(
        error,
        EvalError::ContextWriteRejected(Span { start: 0, end: 5 })
    );
    assert_eq!(
        context.local("args"),
        Some(&Value::array(vec![Value::Number(2.0)]))
    );
}

#[test]
fn include_evaluates_name_and_requests_story_without_changing_case() {
    let expression = parse(r#""Start""#).expect("Passage 名称应可解析");
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl = include(&expression, &mut context).expect("Start Passage 应可包含");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(context.story().included, vec![String::from("Start")]);
    assert_eq!(context.story().destination, None);
}

#[test]
fn include_reports_case_sensitive_missing_passage() {
    let expression = parse(r#""start""#).expect("Passage 名称应可解析");
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let error: crate::macro_runtime::StoryMacroError<&'static str> =
        include(&expression, &mut context).expect_err("Passage 名称应区分大小写");

    assert_eq!(
        error,
        crate::macro_runtime::StoryMacroError::MissingPassage {
            name: String::from("start"),
            span: Span { start: 0, end: 7 },
        }
    );
    assert_eq!(context.story().included, Vec::<String>::new());
}

#[test]
fn include_rejects_non_scalar_name_with_stable_diagnostic() {
    let expression = parse("{ name: 'Start' }").expect("Object 表达式应可解析");
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let error: crate::macro_runtime::StoryMacroError<&'static str> =
        include(&expression, &mut context).expect_err("Object 不能作为 Passage 名称");
    let span: Option<Span> = error.span();
    let diagnostic: Diagnostic = error.diagnostic(|_story_error| unreachable!());

    assert_eq!(span, Some(Span { start: 0, end: 17 }));
    assert_eq!(diagnostic.code, "macro.invalid_passage_name");
    assert_eq!(context.story().included, Vec::<String>::new());
}

#[test]
fn goto_requests_navigation_and_stops_current_passage() {
    let expression = parse(r#""End""#).expect("Passage 名称应可解析");
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl = goto(&expression, &mut context).expect("End Passage 应可导航");

    assert_eq!(control, BodyControl::StopPassage);
    assert_eq!(context.story().destination, Some(String::from("End")));
    assert_eq!(context.story().included, Vec::<String>::new());
}

#[test]
fn goto_does_not_request_navigation_when_target_is_missing() {
    let expression = parse(r#""end""#).expect("Passage 名称应可解析");
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let error: crate::macro_runtime::StoryMacroError<&'static str> =
        goto(&expression, &mut context).expect_err("Passage 名称应区分大小写");

    assert_eq!(
        error,
        crate::macro_runtime::StoryMacroError::MissingPassage {
            name: String::from("end"),
            span: Span { start: 0, end: 5 },
        }
    );
    assert_eq!(context.story().destination, None);
}

#[test]
fn logic_body_executes_native_nodes_and_stops_after_goto() {
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_node(HirBodyKind::Set(Box::new(
            parse("$count = 2").expect("set 应可解析"),
        ))),
        logic_node(HirBodyKind::Set(Box::new(
            parse("@remove = 1").expect("Local set 应可解析"),
        ))),
        logic_node(HirBodyKind::Unset(Box::new(
            parse("@remove").expect("unset 应可解析"),
        ))),
        logic_node(HirBodyKind::Run(Box::new(
            parse("$count += 3").expect("run 应可解析"),
        ))),
        logic_node(HirBodyKind::Include(Box::new(
            parse(r#""Start""#).expect("include 应可解析"),
        ))),
        logic_node(HirBodyKind::Goto(Box::new(
            parse(r#""End""#).expect("goto 应可解析"),
        ))),
        logic_node(HirBodyKind::Set(Box::new(
            parse("$count = 99").expect("停止后的 set 应可解析"),
        ))),
    ];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl = execute_logic_body(&body, &mut context).expect("逻辑正文应执行");

    assert_eq!(control, BodyControl::StopPassage);
    assert_eq!(
        context.state().variable(VariableScope::Variables, "count"),
        Some(&Value::Number(5.0))
    );
    assert_eq!(context.local("remove"), None);
    assert_eq!(context.story().included, vec![String::from("Start")]);
    assert_eq!(context.story().destination, Some(String::from("End")));
}

#[test]
fn logic_body_ignores_visible_text_owned_by_surface_dispatch() {
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Text("visible"))];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl =
        execute_logic_body(&body, &mut context).expect("SemanticOutput 文本不应阻断延迟动作");

    assert_eq!(control, BodyControl::Continue);
}

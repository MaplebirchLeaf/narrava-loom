use super::*;

#[test]
fn logic_if_executes_only_the_first_truthy_branch() {
    let conditional: HirIf<'_> = HirIf {
        branches: vec![
            HirIfBranch {
                condition: parse("false").expect("false 条件应可解析"),
                body: vec![logic_set("$count = 10")],
            },
            HirIfBranch {
                condition: parse("[]").expect("空 Array 条件应可解析"),
                body: vec![logic_set("$count = 2")],
            },
            HirIfBranch {
                condition: parse("missing()").expect("后续条件应可解析"),
                body: vec![logic_set("$count = 30")],
            },
        ],
        fallback: Some(vec![logic_set("$count = 40")]),
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::If(conditional))];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl = execute_logic_body(&body, &mut context).expect("If 应选择第二分支");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(
        context.state().variable(VariableScope::Variables, "count"),
        Some(&Value::Number(2.0))
    );
}

#[test]
fn logic_if_propagates_stop_passage_from_selected_branch() {
    let conditional: HirIf<'_> = HirIf {
        branches: vec![HirIfBranch {
            condition: parse("true").expect("true 条件应可解析"),
            body: vec![logic_node(HirBodyKind::Goto(Box::new(
                parse(r#""End""#).expect("goto 应可解析"),
            )))],
        }],
        fallback: None,
    };
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_node(HirBodyKind::If(conditional)),
        logic_set("$count = 99"),
    ];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl = execute_logic_body(&body, &mut context).expect("If 内 goto 应成功");

    assert_eq!(control, BodyControl::StopPassage);
    assert_eq!(context.story().destination, Some(String::from("End")));
    assert_eq!(
        context.state().variable(VariableScope::Variables, "count"),
        Some(&Value::Number(1.0))
    );
}

#[test]
fn logic_switch_uses_strict_first_match_and_evaluates_subject_once() {
    let switch: HirSwitch<'_> = HirSwitch {
        value: parse("$count++").expect("switch 主值应可解析"),
        cases: vec![
            HirSwitchCase {
                value: parse(r#""1""#).expect("String case 应可解析"),
                body: vec![logic_set("$count = 20")],
            },
            HirSwitchCase {
                value: parse("1").expect("Number case 应可解析"),
                body: vec![logic_set("$count = 30")],
            },
            HirSwitchCase {
                value: parse("missing()").expect("后续 case 应可解析"),
                body: vec![logic_set("$count = 40")],
            },
        ],
        default: Some(vec![logic_set("$count = 50")]),
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Switch(Box::new(switch)))];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl =
        execute_logic_body(&body, &mut context).expect("Number case 应严格匹配");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(
        context.state().variable(VariableScope::Variables, "count"),
        Some(&Value::Number(30.0))
    );
}

#[test]
fn logic_switch_executes_default_and_propagates_stop_passage() {
    let switch: HirSwitch<'_> = HirSwitch {
        value: parse("false").expect("switch 主值应可解析"),
        cases: vec![HirSwitchCase {
            value: parse("0").expect("Number case 应可解析"),
            body: vec![logic_set("$count = 20")],
        }],
        default: Some(vec![logic_node(HirBodyKind::Goto(Box::new(
            parse(r#""End""#).expect("goto 应可解析"),
        )))]),
    };
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_node(HirBodyKind::Switch(Box::new(switch))),
        logic_set("$count = 99"),
    ];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl =
        execute_logic_body(&body, &mut context).expect("default goto 应成功");

    assert_eq!(control, BodyControl::StopPassage);
    assert_eq!(context.story().destination, Some(String::from("End")));
    assert_eq!(
        context.state().variable(VariableScope::Variables, "count"),
        Some(&Value::Number(1.0))
    );
}

#[test]
fn logic_while_consumes_continue_and_break_at_its_boundary() {
    let loop_node: HirWhile<'_> = HirWhile {
        condition: parse("$count < 5").expect("while 条件应可解析"),
        body: vec![
            logic_set("$count += 1"),
            logic_if("$count === 2", vec![logic_node(HirBodyKind::Continue)]),
            logic_if("$count === 4", vec![logic_node(HirBodyKind::Break)]),
            logic_set("@sum += $count"),
        ],
    };
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_set("@sum = 0"),
        logic_node(HirBodyKind::While(Box::new(loop_node))),
        logic_set("@finished = true"),
    ];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl = execute_logic_body(&body, &mut context).expect("while 应完成");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(
        context.state().variable(VariableScope::Variables, "count"),
        Some(&Value::Number(4.0))
    );
    assert_eq!(context.local("sum"), Some(&Value::Number(4.0)));
    assert_eq!(context.local("finished"), Some(&Value::Boolean(true)));
}

#[test]
fn logic_while_propagates_stop_passage() {
    let loop_node: HirWhile<'_> = HirWhile {
        condition: parse("true").expect("while 条件应可解析"),
        body: vec![logic_node(HirBodyKind::Goto(Box::new(
            parse(r#""End""#).expect("goto 应可解析"),
        )))],
    };
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_node(HirBodyKind::While(Box::new(loop_node))),
        logic_set("$count = 99"),
    ];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl =
        execute_logic_body(&body, &mut context).expect("while 内 goto 应成功");

    assert_eq!(control, BodyControl::StopPassage);
    assert_eq!(context.story().destination, Some(String::from("End")));
    assert_eq!(
        context.state().variable(VariableScope::Variables, "count"),
        Some(&Value::Number(1.0))
    );
}

#[test]
fn logic_for_of_iterates_snapshot_values_and_consumes_continue() {
    let loop_node: HirFor<'_> = HirFor {
        target: logic_for_target("@item"),
        kind: HirForKind::Of {
            collection: parse("[1, 2, 3]").expect("Array 集合应可解析"),
            span: logic_span(),
        },
        body: vec![
            logic_if("@item === 2", vec![logic_node(HirBodyKind::Continue)]),
            logic_set("@sum += @item"),
        ],
    };
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_set("@sum = 0"),
        logic_node(HirBodyKind::For(Box::new(loop_node))),
    ];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl = execute_logic_body(&body, &mut context).expect("for of 应完成");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(context.local("sum"), Some(&Value::Number(4.0)));
    assert_eq!(context.local("item"), Some(&Value::Number(3.0)));
}

#[test]
fn logic_for_in_iterates_object_keys_in_property_order() {
    let loop_node: HirFor<'_> = HirFor {
        target: logic_for_target("@key"),
        kind: HirForKind::In {
            collection: parse("{ first: 1, second: 2 }").expect("Object 集合应可解析"),
            span: logic_span(),
        },
        body: vec![logic_set("@last = @key")],
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::For(Box::new(loop_node)))];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl = execute_logic_body(&body, &mut context).expect("for in 应完成");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(context.local("key"), Some(&Value::string("second")));
    assert_eq!(context.local("last"), Some(&Value::string("second")));
}

#[test]
fn logic_for_range_includes_end_and_selects_default_direction() {
    let ascending: HirFor<'_> = HirFor {
        target: logic_for_target("@item"),
        kind: HirForKind::Range {
            start: parse("1").expect("range 起点应可解析"),
            start_span: logic_span(),
            end: parse("3").expect("range 终点应可解析"),
            end_span: logic_span(),
            step: None,
            step_span: None,
        },
        body: vec![logic_set("@sum += @item")],
    };
    let descending: HirFor<'_> = HirFor {
        target: logic_for_target("@item"),
        kind: HirForKind::Range {
            start: parse("3").expect("range 起点应可解析"),
            start_span: logic_span(),
            end: parse("1").expect("range 终点应可解析"),
            end_span: logic_span(),
            step: None,
            step_span: None,
        },
        body: vec![logic_set("@sum += @item")],
    };
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_set("@sum = 0"),
        logic_node(HirBodyKind::For(Box::new(ascending))),
        logic_node(HirBodyKind::For(Box::new(descending))),
    ];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl = execute_logic_body(&body, &mut context).expect("range 应完成");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(context.local("sum"), Some(&Value::Number(12.0)));
    assert_eq!(context.local("item"), Some(&Value::Number(1.0)));
}

#[test]
fn logic_for_range_uses_explicit_step() {
    let loop_node: HirFor<'_> = HirFor {
        target: logic_for_target("@item"),
        kind: HirForKind::Range {
            start: parse("1").expect("range 起点应可解析"),
            start_span: logic_span(),
            end: parse("5").expect("range 终点应可解析"),
            end_span: logic_span(),
            step: Some(parse("2").expect("range 步长应可解析")),
            step_span: Some(logic_span()),
        },
        body: vec![logic_set("@sum += @item")],
    };
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_set("@sum = 0"),
        logic_node(HirBodyKind::For(Box::new(loop_node))),
    ];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl = execute_logic_body(&body, &mut context).expect("range 应完成");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(context.local("sum"), Some(&Value::Number(9.0)));
    assert_eq!(context.local("item"), Some(&Value::Number(5.0)));
}

#[test]
fn logic_for_range_rejects_step_that_moves_away_from_end() {
    let loop_node: HirFor<'_> = HirFor {
        target: logic_for_target("@item"),
        kind: HirForKind::Range {
            start: parse("1").expect("range 起点应可解析"),
            start_span: logic_span(),
            end: parse("3").expect("range 终点应可解析"),
            end_span: logic_span(),
            step: Some(parse("-1").expect("range 步长应可解析")),
            step_span: Some(logic_span()),
        },
        body: Vec::new(),
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::For(Box::new(loop_node)))];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let error: LogicNodeError<&'static str> =
        execute_logic_body(&body, &mut context).expect_err("反向步长应被拒绝");

    assert_eq!(
        error,
        LogicNodeError::Evaluation(EvalError::InvalidRange(Span { start: 0, end: 2 }))
    );
    assert_eq!(context.local("item"), None);
}

#[test]
fn logic_exit_crosses_loop_structure_and_stops_remaining_body() {
    let loop_node: HirWhile<'_> = HirWhile {
        condition: parse("true").expect("while 条件应可解析"),
        body: vec![logic_node(HirBodyKind::Exit)],
    };
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_node(HirBodyKind::While(Box::new(loop_node))),
        logic_set("$count = 99"),
    ];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(4.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let mut context: MacroLogicContext<'_, LogicStoryContext> =
        MacroLogicContext::new(&mut state, &mut story, &mut locals);

    let control: BodyControl =
        execute_logic_body(&body, &mut context).expect("exit 应传播到最近执行域");

    assert_eq!(control, BodyControl::ExitScope);
    assert_eq!(
        context.state().variable(VariableScope::Variables, "count"),
        Some(&Value::Number(4.0))
    );
}

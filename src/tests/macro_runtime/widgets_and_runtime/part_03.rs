// widgets_and_runtime.rs 测试分片 03：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn runtime_context_routes_widgets_inside_for_of_and_for_in() {
    let widget: HirWidget<'_> = HirWidget {
        name: "countIteration",
        body: vec![logic_set("$count += 1")],
    };
    let of_loop: HirFor<'_> = HirFor {
        target: logic_for_target("@item"),
        kind: HirForKind::Of {
            collection: parse("[1, 2, 3, 4]").expect("Array 集合应可解析"),
            span: logic_span(),
        },
        body: vec![
            logic_if("@item === 2", vec![logic_node(HirBodyKind::Continue)]),
            logic_node(HirBodyKind::Macro(HirMacro {
                name: "countIteration",
                arguments: HirMacroArguments::None,
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: Vec::new(),
            })),
            logic_if("@item === 3", vec![logic_node(HirBodyKind::Break)]),
        ],
    };
    let in_loop: HirFor<'_> = HirFor {
        target: logic_for_target("@key"),
        kind: HirForKind::In {
            collection: parse("{ first: 1, second: 2 }").expect("Object 集合应可解析"),
            span: logic_span(),
        },
        body: vec![logic_node(HirBodyKind::Macro(HirMacro {
            name: "countIteration",
            arguments: HirMacroArguments::None,
            syntax_kind: crate::twee::MacroSyntaxKind::Inline,
            body: Vec::new(),
        }))],
    };
    let iterator: HirWidget<'_> = HirWidget {
        name: "iterate",
        body: vec![
            logic_node(HirBodyKind::For(Box::new(of_loop))),
            logic_node(HirBodyKind::For(Box::new(in_loop))),
        ],
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Macro(HirMacro {
        name: "iterate",
        arguments: HirMacroArguments::None,
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    }))];
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let _iterator_previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &iterator);
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let result: Result<BodyControl, RuntimeExecutionError<&'static str>> =
        runtime.execute_body(&body);
    let control: BodyControl = result.expect("for of/in 应使用上层 Macro 分派");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(state.count, Value::Number(4.0));
    assert_eq!(locals.args(), None);
}

#[test]
fn runtime_context_routes_widgets_and_consumes_control_inside_for_range() {
    let widget: HirWidget<'_> = HirWidget {
        name: "addIteration",
        body: vec![logic_set("$count += @args[0]")],
    };
    let loop_node: HirFor<'_> = HirFor {
        target: logic_for_target("@item"),
        kind: HirForKind::Range {
            start: parse("1").expect("range 起点应可解析"),
            start_span: logic_span(),
            end: parse("5").expect("range 终点应可解析"),
            end_span: logic_span(),
            step: None,
            step_span: None,
        },
        body: vec![
            logic_if("@item === 2", vec![logic_node(HirBodyKind::Continue)]),
            logic_node(HirBodyKind::Macro(HirMacro {
                name: "addIteration",
                arguments: HirMacroArguments::Raw("@item"),
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: Vec::new(),
            })),
            logic_if("@item === 4", vec![logic_node(HirBodyKind::Break)]),
        ],
    };
    let iterator: HirWidget<'_> = HirWidget {
        name: "iterateRange",
        body: vec![logic_node(HirBodyKind::For(Box::new(loop_node)))],
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Macro(HirMacro {
        name: "iterateRange",
        arguments: HirMacroArguments::None,
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    }))];
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _widget_previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let _iterator_previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &iterator);
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let result: Result<BodyControl, RuntimeExecutionError<&'static str>> =
        runtime.execute_body(&body);
    let control: BodyControl = result.expect("for range 应使用上层 Macro 分派");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(state.count, Value::Number(8.0));
    assert_eq!(locals.args(), None);
}

#[test]
fn passage_execution_consumes_exit_and_stops_remaining_body() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let passage: HirPassage<'_> = HirPassage {
        source: &source.path,
        name: "Start",
        tags: Vec::new(),
        body: vec![
            logic_set("$count = 1"),
            logic_node(HirBodyKind::Exit),
            logic_set("$count = 99"),
        ],
    };
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let result: Result<BodyExecution, RuntimeExecutionError<&'static str>> =
        runtime.execute_passage(&passage);
    let control: BodyControl = result.expect("Passage 应在自己的边界消费 exit").control;

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(state.count, Value::Number(1.0));
}

#[test]
fn passage_execution_preserves_goto_stop_signal() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let passage: HirPassage<'_> = HirPassage {
        source: &source.path,
        name: "Start",
        tags: Vec::new(),
        body: vec![
            logic_node(HirBodyKind::Goto(Box::new(
                parse(r#""End""#).expect("goto 目标应可解析"),
            ))),
            logic_set("$count = 99"),
        ],
    };
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let result: Result<BodyExecution, RuntimeExecutionError<&'static str>> =
        runtime.execute_passage(&passage);
    let control: BodyControl = result.expect("Passage 应保留 goto 的停止信号").control;

    assert_eq!(control, BodyControl::StopPassage);
    assert_eq!(story.destination, Some(String::from("End")));
    assert_eq!(state.count, Value::Number(0.0));
}

#[test]
fn story_passage_execution_uses_case_sensitive_names() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled_story: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![logic_set("$count = 1")],
            },
            HirPassage {
                source: &source.path,
                name: "start",
                tags: Vec::new(),
                body: vec![logic_set("$count = 2")],
            },
        ],
    };
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let upper: BodyControl = runtime
        .execute_story_passage(&compiled_story, "Start")
        .expect("Start Passage 应存在")
        .control;
    let lower: BodyControl = runtime
        .execute_story_passage(&compiled_story, "start")
        .expect("start Passage 应独立存在")
        .control;
    let missing: RuntimeExecutionError<&'static str> = runtime
        .execute_story_passage(&compiled_story, "START")
        .expect_err("PassageName 不应忽略大小写");

    assert_eq!(upper, BodyControl::Continue);
    assert_eq!(lower, BodyControl::Continue);
    assert_eq!(
        missing,
        RuntimeExecutionError::MissingPassage(String::from("START"))
    );
    assert_eq!(state.count, Value::Number(2.0));
}

use super::*;

#[test]
fn print_builtin_builds_styled_text_from_flat_twee_arguments() {
    let execution: BodyExecution = print(&[
        Value::string("关键道具"),
        Value::Number(32.0),
        Value::string("strong"),
    ])
    .expect("短写参数应产生语义文字");

    assert_eq!(
        execution.output.nodes(),
        [SemanticNode::StyledText {
            text: TextValue::from("关键道具"),
            styles: vec![TextStyle::Strong],
            color: TextColor::GREEN,
            delay: None,
            heading: None,
        }]
    );
}

#[test]
fn print_builtin_without_options_emits_plain_text() {
    let execution: BodyExecution =
        print(&[Value::string("普通文本")]).expect("单参数 print 应输出纯文本");

    assert_eq!(
        execution.output.nodes(),
        [SemanticNode::Text(TextValue::from("普通文本"))]
    );
}

#[test]
fn print_builtin_tone_error_names_the_current_macro() {
    let error: Diagnostic =
        print(&[Value::string("x"), Value::Number(64.0)]).expect_err("超出色阶范围的 color 应报错");

    assert_eq!(error.code, "macro.print.invalid_arguments");
    assert!(error.message.contains("`print` color"));
    assert!(!error.message.contains("`text`"));
}

#[test]
fn print_builtin_object_form_parses_delay_and_validates_range() {
    let execution: BodyExecution = print(&[
        Value::string("延迟文本"),
        Value::object(vec![
            (String::from("color"), Value::Number(24.0)),
            (
                String::from("styles"),
                Value::array(vec![Value::string("strong")]),
            ),
            (String::from("delay"), Value::Number(800.0)),
        ]),
    ])
    .expect("对象形式应产生带延迟的语义文字");

    assert_eq!(
        execution.output.nodes(),
        [SemanticNode::StyledText {
            text: TextValue::from("延迟文本"),
            styles: vec![TextStyle::Strong],
            color: TextColor::YELLOW,
            delay: Some(800),
            heading: None,
        }]
    );

    for invalid in [
        Value::object(vec![(String::from("delay"), Value::string("800"))]),
        Value::object(vec![(String::from("delay"), Value::Number(-1.0))]),
        Value::object(vec![(String::from("delay"), Value::Number(86_400_001.0))]),
        Value::object(vec![(String::from("delay"), Value::Number(800.5))]),
    ] {
        let error: Diagnostic =
            print(&[Value::string("x"), invalid]).expect_err("非法 delay 应报错");
        assert_eq!(error.code, "macro.print.invalid_arguments");
    }
}

#[test]
fn print_builtin_object_form_parses_heading_and_validates_level() {
    let execution: BodyExecution = print(&[
        Value::string("第一页"),
        Value::object(vec![
            (String::from("heading"), Value::Number(2.0)),
            (
                String::from("styles"),
                Value::array(vec![Value::string("strong")]),
            ),
        ]),
    ])
    .expect("对象形式应解析结构性标题级别");

    assert_eq!(
        execution.output.nodes(),
        [SemanticNode::StyledText {
            text: TextValue::from("第一页"),
            styles: vec![TextStyle::Strong],
            color: TextColor::DEFAULT,
            delay: None,
            heading: Some(crate::semantic::HeadingLevel::H2),
        }]
    );

    for invalid in [
        Value::object(vec![(String::from("heading"), Value::string("h2"))]),
        Value::object(vec![(String::from("heading"), Value::Number(0.0))]),
        Value::object(vec![(String::from("heading"), Value::Number(3.0))]),
        Value::object(vec![(String::from("heading"), Value::Number(1.5))]),
    ] {
        let error: Diagnostic =
            print(&[Value::string("x"), invalid]).expect_err("非法 heading 应报错");
        assert_eq!(error.code, "macro.print.invalid_arguments");
    }
}

#[test]
fn link_builtin_converts_prepared_interaction_into_navigation_semantics() {
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(4, 7);
    let argument: Value = Value::object(vec![
        (String::from("label"), Value::string("进入森林")),
        (String::from("target"), Value::string("Forest")),
    ]);

    let execution: BodyExecution = link(&[argument], identity).expect("合法交互参数应产生导航语义");

    assert_eq!(execution.control, BodyControl::Continue);
    assert!(matches!(
        execution.output.nodes(),
        [SemanticNode::Navigation { id, label, target, .. }]
            if !id.as_str().is_empty()
                && label.as_units() == TextValue::from("进入森林").as_units()
                && target == "Forest"
    ));
}

#[test]
fn dynamic_fragment_execution_accumulates_markup_and_text_output() {
    let source: crate::source::SourcePath = crate::source::SourcePath::fragment();
    let nodes: Vec<crate::twee::BodyNode<'_>> =
        crate::twee::parse_fragment("你好，${$count}！", &source).expect("片段应可解析");
    let hir_nodes: Vec<crate::hir::HirBodyNode<'_>> =
        crate::hir::lower_fragment(&nodes).expect("片段应可降为 HIR");

    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(42.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let execution: BodyExecution = runtime
        .execute_fragment(&hir_nodes)
        .expect("动态片段应执行完成");

    assert_eq!(execution.control, BodyControl::Continue);
    assert_eq!(execution.output.len(), 1);
    assert_eq!(
        execution.output.nodes()[0],
        SemanticNode::Text(TextValue::from("你好，${$count}！"))
    );
}

#[test]
fn public_fragment_api_parses_and_executes() {
    use crate::macro_runtime::{ParsedFragment, parse_fragment};

    let fragment: ParsedFragment<'_> =
        parse_fragment("你好，${$count}！").expect("公开入口应解析片段");
    assert_eq!(fragment.nodes().len(), 1);

    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(7.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let execution: BodyExecution = runtime
        .execute_parsed_fragment(&fragment)
        .expect("公开入口应执行片段");
    assert_eq!(execution.output.len(), 1);
    assert_eq!(
        execution.output.nodes()[0],
        SemanticNode::Text(TextValue::from("你好，${$count}！"))
    );
}

#[test]
fn public_fragment_api_keeps_unclosed_interpolation_like_text() {
    use crate::macro_runtime::{ParsedFragment, parse_fragment};

    let fragment: ParsedFragment<'_> = parse_fragment("${未闭合").expect("普通正文应保持字面 Text");
    assert_eq!(fragment.nodes().len(), 1);
}

#[test]
fn print_is_the_only_explicit_text_evaluation_boundary() {
    use crate::macro_runtime::{ParsedFragment, parse_fragment};

    let fragment: ParsedFragment<'_> = parse_fragment(
        "你在$forest_name森林里。\n你在<<print $forest_name>>森林里。\n你在<<print `$forest_name`>>森林里。\n你在<<print ${$forest_name}>>森林里。\n你在${$forest_name}森林里。",
    )
    .expect("print 示例应解析");
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut state: State = State::new();
    let _previous: Option<Value> = state.variables_set("forest_name", Value::string("奇幻"));
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let execution: BodyExecution = runtime
        .execute_parsed_fragment(&fragment)
        .expect("print 示例应执行");
    let rendered: String = execution
        .output
        .nodes()
        .iter()
        .map(|node: &SemanticNode| match node {
            SemanticNode::Text(text) => text.to_unicode_string().expect("测试文本不含孤立代理项"),
            _ => panic!("print 示例只应产生 Text"),
        })
        .collect();

    assert_eq!(
        rendered,
        "你在$forest_name森林里。\n你在奇幻森林里。\n你在$forest_name森林里。\n你在奇幻森林里。\n你在${$forest_name}森林里。"
    );
}

#[test]
fn silently_discards_text_but_keeps_state_changes() {
    let source: Source = Source {
        path: crate::source::SourcePath::from_path(Path::new("story/silently.twee"))
            .expect("测试 SourcePath 应有效"),
        kind: crate::source::SourceKind::Twee,
        content: String::from(
            ":: Start\n<<silently>><<set $forest_name to \"奇幻\">>你在$forest_name森林里。<</silently>>你在<<print $forest_name>>森林里。",
        ),
    };
    let story_ast: crate::twee::Story<'_> =
        crate::twee::Story::build(std::slice::from_ref(&source)).expect("Twee 应可编译");
    let hir: crate::hir::HirStory<'_> =
        crate::hir::HirStory::lower(&story_ast).expect("silently 应进入 HIR");
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut state: State = State::new();
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let execution: BodyExecution = runtime
        .execute_passage(&hir.passages[0])
        .expect("silently 正文应执行");
    drop(runtime);
    let rendered: String = execution
        .output
        .nodes()
        .iter()
        .map(|node: &SemanticNode| match node {
            SemanticNode::Text(text) => text.to_unicode_string().expect("测试文本应为有效 Unicode"),
            _ => panic!("silently 示例只应产生 Text"),
        })
        .collect();

    assert_eq!(rendered, "你在奇幻森林里。");
    assert_eq!(
        state.variables_get("forest_name"),
        Some(&Value::string("奇幻"))
    );
}

#[test]
fn passage_execution_accumulates_text_and_print_output() {
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
            logic_node(HirBodyKind::Text("你好，")),
            logic_node(HirBodyKind::Print(crate::hir::HirPrint::Expression(
                parse("$count").expect("插值应可解析"),
            ))),
            logic_node(HirBodyKind::Text("   ")),
        ],
    };
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(42.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let execution: BodyExecution = runtime
        .execute_passage(&passage)
        .expect("静态正文与 print 应正常执行");

    assert_eq!(execution.control, BodyControl::Continue);
    // 静态正文和 print 结果都进入 Text；纯空白不产生输出。
    assert_eq!(execution.output.len(), 2);
    assert_eq!(
        execution.output.nodes()[0],
        SemanticNode::Text(TextValue::from("你好，"))
    );
    assert_eq!(
        execution.output.nodes()[1],
        SemanticNode::Text(TextValue::from("42"))
    );
}

#[test]
fn passage_output_survives_stop_control_and_is_cleared_between_executions() {
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
            logic_node(HirBodyKind::Text("before")),
            logic_node(HirBodyKind::Goto(Box::new(
                parse(r#""End""#).expect("goto 目标应可解析"),
            ))),
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

    let first: BodyExecution = runtime
        .execute_passage(&passage)
        .expect("goto 应保留停止信号并携带已累积输出");
    assert_eq!(first.control, BodyControl::StopPassage);
    assert_eq!(first.output.len(), 1);
    assert_eq!(
        first.output.nodes()[0],
        SemanticNode::Text(TextValue::from("before"))
    );

    let second: BodyExecution = runtime
        .execute_passage(&passage)
        .expect("第二次执行应从空输出开始");
    assert_eq!(second.output.len(), 1, "公共入口应取走上一次累积");
}

#[test]
fn input_builtins_create_state_bound_semantic_controls() {
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(7, 11);
    let checkbox_output: BodyExecution = checkbox(
        "$enabled",
        &Value::Boolean(false),
        &Value::Boolean(true),
        &Value::Boolean(true),
        identity,
        1,
    )
    .expect("checkbox 应接受布尔状态");
    let radio_output: BodyExecution = radiobutton(
        "$mode",
        &Value::string("story"),
        &Value::string("story"),
        identity,
        2,
    )
    .expect("radiobutton 应接受文字状态");
    let text_output: BodyExecution =
        textbox("$name", &Value::string("Maple"), identity, 3).expect("textbox 应接受文字状态");

    let SemanticNode::Input { binding, .. } = &checkbox_output.output.nodes()[0] else {
        panic!("checkbox 应产生 Input")
    };
    assert_eq!(binding.receiver, "$enabled");
    assert_eq!(
        binding.kind,
        SemanticInputKind::Checkbox {
            unchecked: SemanticValue::Boolean(false),
            checked: SemanticValue::Boolean(true),
            selected: true,
        }
    );
    assert!(matches!(
        radio_output.output.nodes()[0],
        SemanticNode::Input {
            binding: crate::semantic::SemanticInputBinding {
                kind: SemanticInputKind::Radio { selected: true, .. },
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        text_output.output.nodes()[0],
        SemanticNode::Input {
            binding: crate::semantic::SemanticInputBinding {
                kind: SemanticInputKind::Text { .. },
                ..
            },
            ..
        }
    ));
}

// widgets_and_runtime.rs 测试分片 02：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn async_native_resume_runs_after_only_on_final_completion() {
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(3, 5);
    let suspension = async_native_suspension(41);
    let mut lifecycle = AsyncNativeAfterLifecycle;

    let outcome = resume_async_native_macro(
        identity,
        suspension,
        Some(&mut lifecycle),
        |handle, scopes| {
            assert_eq!(handle, 41);
            assert_eq!(scopes.args(), Some(&[Value::Number(7.0)][..]));
            Ok::<_, &'static str>(MacroHandlerOutcome::Complete(RuntimeMacroExecution {
                execution: BodyExecution {
                    control: BodyControl::Continue,
                    output: crate::semantic::SemanticOutput::from_nodes(vec![
                        SemanticNode::Text(TextValue::from("完成")),
                    ]),
                },
                includes_entered: 2,
            }))
        },
    )
    .expect("最终完成应执行 after");

    match outcome {
        MacroResumeOutcome::Complete { output, scopes } => {
            assert_eq!(output.includes_entered, 2);
            assert_eq!(
                output.execution.output.nodes(),
                &[
                    SemanticNode::Text(TextValue::from("完成")),
                    SemanticNode::Text(TextValue::from("!")),
                ]
            );
            assert_eq!(scopes.args(), None);
        }
        MacroResumeOutcome::Pending(_) => panic!("测试恢复应最终完成"),
    }
}

#[test]
fn async_native_resume_preserves_name_when_pending_again() {
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(3, 5);
    let suspension = async_native_suspension(41);

    let outcome = resume_async_native_macro(identity, suspension, None, |handle, _scopes| {
        assert_eq!(handle, 41);
        Ok::<_, &'static str>(MacroHandlerOutcome::Pending(42))
    })
    .expect("再次 Pending 应保留 Native 调用身份");

    match outcome {
        MacroResumeOutcome::Pending(suspension) => {
            assert_eq!(
                suspension.handle,
                RuntimeNativePending {
                    name: "wait".to_owned(),
                    handle: 42,
                }
            );
            assert_eq!(
                suspension.scopes.into_scopes().args(),
                Some(&[Value::Number(7.0)][..])
            );
        }
        MacroResumeOutcome::Complete { .. } => panic!("测试恢复应再次暂停"),
    }
}

#[test]
fn async_native_resume_cleans_its_frame_when_after_fails() {
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(3, 5);
    let suspension = async_native_suspension(41);
    let mut lifecycle = FailingAfterLifecycle;

    let error = resume_async_native_macro(
        identity,
        suspension,
        Some(&mut lifecycle),
        |_handle, _scopes| {
            Ok::<_, &'static str>(MacroHandlerOutcome::Complete(RuntimeMacroExecution {
                execution: BodyExecution::default(),
                includes_entered: 0,
            }))
        },
    )
    .expect_err("after 失败应终止恢复");

    match *error {
        MacroResumeError::Resume(failure) => {
            assert!(matches!(
                failure.error,
                RuntimeNativeResumeError::Lifecycle(_)
            ));
            assert_eq!(failure.scopes.args(), None);
        }
        MacroResumeError::Identity(_) => panic!("测试身份应一致"),
    }
}

#[test]
fn runtime_reports_a_missing_native_macro_adapter() {
    let node: HirBodyNode<'_> = logic_node(HirBodyKind::Macro(HirMacro {
        name: "announce",
        arguments: HirMacroArguments::None,
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    }));
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> = definitions
        .add(
            "announce",
            MacroDefinition::new(
                MacroBodyKind::Inline,
                MacroArgumentKind::Raw,
                MacroExecutionKind::Sync,
                RuntimeMacroHandler::Native("scripts:announce"),
            ),
        );
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let error: RuntimeExecutionError<&'static str> = runtime
        .execute_node(&node)
        .expect_err("缺少 Binding Adapter 必须明确失败");

    assert_eq!(
        error,
        RuntimeExecutionError::NativeMacro(NativeMacroError::MissingCallbacks)
    );
}

#[test]
fn runtime_widget_routes_every_variable_scope_to_its_real_owner() {
    let widget: HirWidget<'_> = HirWidget {
        name: "updateState",
        body: vec![
            logic_set(r#"formatName = "changed""#),
            logic_set("setup.ready = true"),
            logic_set("$count += @args[0]"),
            logic_set("_turn = @args[1]"),
            logic_set("@inside = 9"),
        ],
    };
    let node: HirBodyNode<'_> = logic_node(HirBodyKind::Macro(HirMacro {
        name: "updateState",
        arguments: HirMacroArguments::Raw("2 3"),
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    }));
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let mut state: State = State::new();
    let _global_previous: Option<Value> = state.global_set("formatName", Value::string("initial"));
    let _setup_previous: Value = state.setup_set(Value::object(vec![(
        String::from("ready"),
        Value::Boolean(false),
    )]));
    let _count_previous: Option<Value> = state.variables_set("count", Value::Number(1.0));
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let result: Result<BodyControl, RuntimeExecutionError<&'static str>> =
        runtime.execute_node(&node);
    let control: BodyControl = result.expect("真实 State 应能贯通 Widget Runtime");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(
        state.global_get("formatName"),
        Some(&Value::string("changed"))
    );
    assert_eq!(
        state.setup_get(),
        &Value::object(vec![(String::from("ready"), Value::Boolean(true))])
    );
    assert_eq!(state.variables_get("count"), Some(&Value::Number(3.0)));
    assert_eq!(state.temporary_get("turn"), Some(&Value::Number(3.0)));
    assert_eq!(locals.get("inside"), None);
    assert_eq!(locals.args(), None);
}

#[test]
fn runtime_context_executes_a_widget_called_directly_from_another_widget() {
    let inner: HirWidget<'_> = HirWidget {
        name: "inner",
        body: vec![logic_set("$count = @args[0]")],
    };
    let outer: HirWidget<'_> = HirWidget {
        name: "outer",
        body: vec![logic_node(HirBodyKind::Macro(HirMacro {
            name: "inner",
            arguments: HirMacroArguments::Raw("(@args[0] + 3)"),
            syntax_kind: crate::twee::MacroSyntaxKind::Inline,
            body: Vec::new(),
        }))],
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Macro(HirMacro {
        name: "outer",
        arguments: HirMacroArguments::Raw("4"),
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    }))];
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _inner_previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &inner);
    let _outer_previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &outer);
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let result: Result<BodyControl, RuntimeExecutionError<&'static str>> =
        runtime.execute_body(&body);
    let control: BodyControl = result.expect("嵌套 Widget 应共用上层 Definitions 分派");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(state.count, Value::Number(7.0));
    assert_eq!(locals.args(), None);
}

#[test]
fn failed_widget_discards_its_isolated_surface_output() {
    let widget: HirWidget<'_> = HirWidget {
        name: "partial",
        body: vec![
            logic_node(HirBodyKind::Text("不应泄漏")),
            logic_node(HirBodyKind::Macro(HirMacro {
                name: "missing",
                arguments: HirMacroArguments::None,
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: Vec::new(),
            })),
        ],
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Macro(HirMacro {
        name: "partial",
        arguments: HirMacroArguments::None,
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    }))];
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let error: RuntimeExecutionError<&'static str> = runtime
        .execute_fragment(&body)
        .expect_err("Widget 内层错误应终止本次调用");
    let recovered: BodyExecution = runtime
        .execute_fragment(&[])
        .expect("错误后的空片段应可读取剩余输出");

    assert_eq!(
        error,
        RuntimeExecutionError::MacroDefinition(MacroDefinitionError::MissingDefinition)
    );
    assert!(recovered.output.is_empty());
}

#[test]
fn successful_widget_merges_its_output_at_the_call_position() {
    let widget: HirWidget<'_> = HirWidget {
        name: "middle",
        body: vec![logic_node(HirBodyKind::Text("中"))],
    };
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_node(HirBodyKind::Text("前")),
        logic_node(HirBodyKind::Macro(HirMacro {
            name: "middle",
            arguments: HirMacroArguments::None,
            syntax_kind: crate::twee::MacroSyntaxKind::Inline,
            body: Vec::new(),
        })),
        logic_node(HirBodyKind::Text("后")),
    ];
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let execution: BodyExecution = runtime
        .execute_fragment(&body)
        .expect("成功 Widget 应在调用位置合并输出");

    assert_eq!(
        execution.output.nodes(),
        &[
            SemanticNode::Text(TextValue::from("前")),
            SemanticNode::Text(TextValue::from("中")),
            SemanticNode::Text(TextValue::from("后")),
        ]
    );
}

#[derive(Default)]
struct RecordingMacroLifecycle {
    stages: Vec<&'static str>,
}

impl MacroLifecycleCallbacks for RecordingMacroLifecycle {
    fn before(&mut self, name: &str, arguments: &mut [Value]) -> Result<(), Diagnostic> {
        assert_eq!(name, "greet");
        self.stages.push("before");
        arguments[0] = Value::string("Hook");
        Ok(())
    }

    fn after(
        &mut self,
        name: &str,
        arguments: &[Value],
        mut output: crate::semantic::SemanticOutput,
    ) -> Result<crate::semantic::SemanticOutput, Diagnostic> {
        assert_eq!(name, "greet");
        assert_eq!(arguments, &[Value::string("Hook")]);
        self.stages.push("after");
        output.push(SemanticNode::Text(TextValue::from("!")));
        Ok(output)
    }
}

#[test]
fn runtime_widget_runs_lifecycle_around_its_isolated_output() {
    let widget: HirWidget<'_> = HirWidget {
        name: "greet",
        body: vec![logic_node(HirBodyKind::Print(
            crate::hir::HirPrint::Expression(parse("@args[0]").expect("Widget 参数插值应可解析")),
        ))],
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Macro(HirMacro {
        name: "greet",
        arguments: HirMacroArguments::Raw(r#""Original""#),
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    }))];
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let mut lifecycle: RecordingMacroLifecycle = RecordingMacroLifecycle::default();
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals)
            .with_macro_lifecycle(&mut lifecycle);

    let execution: BodyExecution = runtime
        .execute_fragment(&body)
        .expect("Widget 生命周期应执行完成");

    assert_eq!(
        execution.output.nodes(),
        &[
            SemanticNode::Text(TextValue::from("Hook")),
            SemanticNode::Text(TextValue::from("!")),
        ]
    );
    assert_eq!(lifecycle.stages, vec!["before", "after"]);
}

#[test]
fn compiler_owned_logic_does_not_enter_macro_lifecycle() {
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::If(HirIf {
        branches: vec![HirIfBranch {
            condition: parse("true").expect("条件应可解析"),
            body: vec![logic_node(HirBodyKind::Text("通过"))],
        }],
        fallback: None,
    }))];
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut lifecycle: RecordingMacroLifecycle = RecordingMacroLifecycle::default();
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals)
            .with_macro_lifecycle(&mut lifecycle);

    let execution: BodyExecution = runtime
        .execute_fragment(&body)
        .expect("编译器逻辑应正常执行");

    assert_eq!(
        execution.output.nodes(),
        &[SemanticNode::Text(TextValue::from("通过"))]
    );
    assert!(lifecycle.stages.is_empty());
}

struct FailingAfterLifecycle;

impl MacroLifecycleCallbacks for FailingAfterLifecycle {
    fn before(&mut self, _name: &str, _arguments: &mut [Value]) -> Result<(), Diagnostic> {
        Ok(())
    }

    fn after(
        &mut self,
        _name: &str,
        _arguments: &[Value],
        _output: crate::semantic::SemanticOutput,
    ) -> Result<crate::semantic::SemanticOutput, Diagnostic> {
        Err(Diagnostic::new(
            "test.macro.after_failed",
            DiagnosticSeverity::Error,
            "after failed",
        ))
    }
}

#[test]
fn failed_widget_after_cleans_its_frame_and_discards_output() {
    let widget: HirWidget<'_> = HirWidget {
        name: "broken",
        body: vec![logic_node(HirBodyKind::Text("不能泄漏"))],
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Macro(HirMacro {
        name: "broken",
        arguments: HirMacroArguments::None,
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    }))];
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let mut lifecycle: FailingAfterLifecycle = FailingAfterLifecycle;
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals)
            .with_macro_lifecycle(&mut lifecycle);

    let error: RuntimeExecutionError<&'static str> = runtime
        .execute_fragment(&body)
        .expect_err("after 失败应终止当前 Widget");
    let recovered: BodyExecution = runtime
        .execute_fragment(&[])
        .expect("失败后 Runtime 应保持可用");

    assert!(matches!(
        error,
        RuntimeExecutionError::MacroLifecycle(diagnostic)
            if diagnostic.code == "test.macro.after_failed"
    ));
    assert!(recovered.output.is_empty());
    drop(runtime);
    assert_eq!(locals.args(), None);
}

#[test]
fn runtime_context_routes_a_widget_inside_the_selected_if_branch() {
    let widget: HirWidget<'_> = HirWidget {
        name: "selected",
        body: vec![logic_set("$count = @args[0]")],
    };
    let conditional: HirIf<'_> = HirIf {
        branches: vec![
            HirIfBranch {
                condition: parse("false").expect("false 条件应可解析"),
                body: vec![logic_set("$count = 99")],
            },
            HirIfBranch {
                condition: parse("true").expect("true 条件应可解析"),
                body: vec![logic_node(HirBodyKind::Macro(HirMacro {
                    name: "selected",
                    arguments: HirMacroArguments::Raw("8"),
                    syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                    body: Vec::new(),
                }))],
            },
        ],
        fallback: None,
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::If(conditional))];
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let result: Result<BodyControl, RuntimeExecutionError<&'static str>> =
        runtime.execute_body(&body);
    let control: BodyControl = result.expect("选中的 if 分支应使用上层 Macro 分派");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(state.count, Value::Number(8.0));
    assert_eq!(locals.args(), None);
}

#[test]
fn runtime_context_routes_a_widget_inside_the_matching_switch_case() {
    let widget: HirWidget<'_> = HirWidget {
        name: "matched",
        body: vec![logic_set("$count = @args[0]")],
    };
    let switch: HirSwitch<'_> = HirSwitch {
        value: parse("2").expect("switch 主值应可解析"),
        cases: vec![
            HirSwitchCase {
                value: parse(r#""2""#).expect("String case 应可解析"),
                body: vec![logic_set("$count = 99")],
            },
            HirSwitchCase {
                value: parse("2").expect("Number case 应可解析"),
                body: vec![logic_node(HirBodyKind::Macro(HirMacro {
                    name: "matched",
                    arguments: HirMacroArguments::Raw("12"),
                    syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                    body: Vec::new(),
                }))],
            },
        ],
        default: Some(vec![logic_set("$count = 77")]),
    };
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Switch(Box::new(switch)))];
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let result: Result<BodyControl, RuntimeExecutionError<&'static str>> =
        runtime.execute_body(&body);
    let control: BodyControl = result.expect("匹配的 switch case 应使用上层 Macro 分派");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(state.count, Value::Number(12.0));
    assert_eq!(locals.args(), None);
}

#[test]
fn runtime_context_routes_widgets_and_consumes_control_inside_while() {
    let widget: HirWidget<'_> = HirWidget {
        name: "increment",
        body: vec![logic_set("$count = @args[0] + 1")],
    };
    let loop_node: HirWhile<'_> = HirWhile {
        condition: parse("$count < 5").expect("while 条件应可解析"),
        body: vec![
            logic_node(HirBodyKind::Macro(HirMacro {
                name: "increment",
                arguments: HirMacroArguments::Raw("$count"),
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: Vec::new(),
            })),
            logic_if("$count === 2", vec![logic_node(HirBodyKind::Continue)]),
            logic_if("$count === 3", vec![logic_node(HirBodyKind::Break)]),
            logic_set("$count = 99"),
        ],
    };
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_node(HirBodyKind::While(Box::new(loop_node))),
        logic_set("$count += 10"),
    ];
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(1.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let result: Result<BodyControl, RuntimeExecutionError<&'static str>> =
        runtime.execute_body(&body);
    let control: BodyControl = result.expect("while 应使用上层 Macro 分派并消费循环控制");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(state.count, Value::Number(13.0));
    assert_eq!(locals.args(), None);
}

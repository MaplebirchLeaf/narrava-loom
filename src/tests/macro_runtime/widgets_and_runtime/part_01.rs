// widgets_and_runtime.rs 测试分片 01：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn widget_body_uses_an_isolated_args_frame_and_consumes_exit() {
    let body: Vec<HirBodyNode<'_>> = vec![
        logic_set("@seen = @args[0]"),
        logic_node(HirBodyKind::Exit),
        logic_set("$count = 99"),
    ];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(4.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(vec![Value::string("outer")]);
    locals
        .set("seen", Value::string("outer-local"))
        .expect("外层局部值应建立");

    let control: BodyControl = execute_widget_body(
        &body,
        vec![Value::string("inner")],
        &mut state,
        &mut story,
        &mut locals,
    )
    .expect("Widget 正文应完成");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(locals.args(), Some([Value::string("outer")].as_slice()));
    assert_eq!(locals.get("seen"), Some(&Value::string("outer-local")));
    assert_eq!(state.count, Value::Number(4.0));
}

#[test]
fn widget_body_with_visible_text_restores_the_outer_frame() {
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Text("尚未接入渲染"))];
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(vec![Value::string("outer")]);

    let control: BodyControl = execute_widget_body(
        &body,
        vec![Value::string("inner")],
        &mut state,
        &mut story,
        &mut locals,
    )
    .expect("可见文本由 SemanticOutput 负责，不应破坏 Widget 逻辑帧");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(locals.args(), Some([Value::string("outer")].as_slice()));
}

#[test]
fn widget_macro_prepares_arguments_and_executes_registered_body() {
    let widget: HirWidget<'_> = HirWidget {
        name: "setCount",
        body: vec![
            logic_set("$count = @args[0]"),
            logic_node(HirBodyKind::Exit),
            logic_set("$count = 99"),
        ],
    };
    let call: HirMacro<'_> = HirMacro {
        name: "setCount",
        arguments: HirMacroArguments::Raw("($count + 2)"),
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    };
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(3.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());

    let control: BodyControl =
        execute_widget_macro(&call, &definitions, &mut state, &mut story, &mut locals)
            .expect("已注册 Widget 调用应完成");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(state.count, Value::Number(5.0));
    assert_eq!(locals.args(), Some([].as_slice()));
}

#[test]
fn registers_top_level_widgets_from_widget_tagged_passages_only() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let span: TweeSpan = TweeSpan {
        start: 0,
        end: 0,
        line: 1,
        column: 1,
    };
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Widgets",
                tags: vec!["widget"],
                body: vec![
                    logic_node(HirBodyKind::Text("不会作为 Passage 输出")),
                    logic_node(HirBodyKind::Widget(HirWidget {
                        name: "greet",
                        body: vec![logic_node(HirBodyKind::Text("Hello"))],
                    })),
                    logic_node(HirBodyKind::Widget(HirWidget {
                        name: "farewell",
                        body: vec![logic_node(HirBodyKind::Text("Bye"))],
                    })),
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Ordinary",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Widget(HirWidget {
                        name: "ignored",
                        body: Vec::new(),
                    }),
                    span,
                }],
            },
        ],
    };
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> = definitions
        .add(
            "greet",
            MacroDefinition::new(
                MacroBodyKind::Inline,
                MacroArgumentKind::Raw,
                MacroExecutionKind::Sync,
                RuntimeMacroHandler::Native("host greet"),
            ),
        );

    let report: WidgetRegistrationReport = register_story_widgets(&mut definitions, &compiled);

    assert_eq!(
        report,
        WidgetRegistrationReport {
            registered: 2,
            replaced: 1,
        }
    );
    assert!(definitions.has("greet"));
    assert!(definitions.has("farewell"));
    assert!(!definitions.has("ignored"));
    let greet: &MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>> =
        definitions.get("greet").expect("后出现的 greet 应保留");
    let RuntimeMacroHandler::Widget(body) = &greet.handler else {
        panic!("greet 应保持 Widget Handler");
    };
    assert!(matches!(body[0].kind, HirBodyKind::Text("Hello")));
}

#[test]
fn widget_macro_rejects_a_container_call_before_running_the_body() {
    let widget: HirWidget<'_> = HirWidget {
        name: "inlineOnly",
        body: vec![logic_set("$count = 9")],
    };
    let call: HirMacro<'_> = HirMacro {
        name: "inlineOnly",
        arguments: HirMacroArguments::None,
        syntax_kind: crate::twee::MacroSyntaxKind::Container,
        body: vec![logic_set("$count = 7")],
    };
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

    let error: WidgetMacroError<&'static str> =
        execute_widget_macro(&call, &definitions, &mut state, &mut story, &mut locals)
            .expect_err("Widget 调用处不能携带正文");

    assert_eq!(error, WidgetMacroError::ContainerCall);
    assert_eq!(state.count, Value::Number(1.0));
    assert_eq!(locals.args(), None);
}

#[test]
fn widget_macro_rejects_an_empty_container_call() {
    let widget: HirWidget<'_> = HirWidget {
        name: "inlineOnly",
        body: vec![logic_set("$count = 9")],
    };
    let call: HirMacro<'_> = HirMacro {
        name: "inlineOnly",
        arguments: HirMacroArguments::None,
        syntax_kind: crate::twee::MacroSyntaxKind::Container,
        body: Vec::new(),
    };
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

    let error: WidgetMacroError<&'static str> =
        execute_widget_macro(&call, &definitions, &mut state, &mut story, &mut locals)
            .expect_err("空正文 Container 也不能调用 Inline Widget");

    assert_eq!(error, WidgetMacroError::ContainerCall);
    assert_eq!(state.count, Value::Number(1.0));
    assert_eq!(locals.args(), None);
}

#[test]
fn runtime_context_routes_a_generic_macro_node_to_its_widget_definition() {
    let widget: HirWidget<'_> = HirWidget {
        name: "raiseCount",
        body: vec![logic_set("$count = @args[0]")],
    };
    let node: HirBodyNode<'_> = logic_node(HirBodyKind::Macro(HirMacro {
        name: "raiseCount",
        arguments: HirMacroArguments::Raw("($count + 4)"),
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    }));
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(2.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let result: Result<BodyControl, RuntimeExecutionError<&'static str>> =
        runtime.execute_node(&node);
    let control: BodyControl = result.expect("通用 Macro 节点应调用 Widget");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(state.count, Value::Number(6.0));
    assert_eq!(locals.args(), None);
}

#[test]
fn vm_pending_macro_can_run_through_the_shared_runtime_dispatcher() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let widget: HirWidget<'_> = HirWidget {
        name: "notice",
        body: vec![logic_node(HirBodyKind::Text("宏输出"))],
    };
    let hir: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![logic_node(HirBodyKind::Macro(HirMacro {
                name: "notice",
                arguments: HirMacroArguments::None,
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: Vec::new(),
            }))],
        }],
    };
    let mir: crate::mir::MirStory<'_, '_> =
        crate::mir::MirStory::lower(&hir).expect("动态 Macro 应进入 MIR");
    let lir: crate::lir::LirProgram<'_, '_, '_> =
        crate::lir::LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut frame: crate::vm::MirExecutionFrame = crate::vm::MirExecutionFrame::new(passage);
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

    assert_eq!(
        frame.step(&bytecode, &mut state),
        Ok(crate::vm::MirStep::MacroPending)
    );
    let owned_call = frame
        .pending_macro(&bytecode)
        .expect("VM 应公开待处理的原始 Macro 调用");
    let call: HirMacro<'_> = owned_call.as_hir();
    let execution: BodyExecution = {
        let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
            RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);
        runtime
            .execute_macro(&call)
            .expect("共享 Runtime 应执行待处理 Widget")
    };

    assert_eq!(execution.control, BodyControl::Continue);
    frame
        .complete_macro(&bytecode, execution.output)
        .expect("完成输出应交回同一 VM 位置");
    assert_eq!(
        frame.step(&bytecode, &mut state),
        Ok(crate::vm::MirStep::Halted)
    );
    assert!(matches!(
        frame.output().nodes(),
        [SemanticNode::Text(text)]
            if text.to_unicode_string().as_deref() == Some("宏输出")
    ));
}

struct TestNativeMacroCallbacks;

impl NativeMacroCallbacks<&'static str, LogicStoryContext> for TestNativeMacroCallbacks {
    fn invoke(
        &mut self,
        handler: &&'static str,
        invocation: MacroInvocation<'_, HirBodyNode<'_>, MacroLogicContext<'_, LogicStoryContext>>,
    ) -> Result<BodyExecution, Diagnostic> {
        assert_eq!(*handler, "scripts:announce");
        assert_eq!(invocation.name, "announce");
        assert!(matches!(invocation.body, MacroInvocationBody::Inline));
        invocation
            .context
            .set_variable(
                VariableScope::Variables,
                "count",
                invocation.arguments[0].clone(),
            )
            .expect("Native Handler 应通过受控 Context 写入 State");
        Ok(BodyExecution {
            control: BodyControl::Continue,
            output: crate::semantic::SemanticOutput::from_nodes(vec![
                SemanticNode::Text(TextValue::from("Native")),
            ]),
        })
    }
}

#[test]
fn runtime_dispatches_a_sync_native_macro_through_binding_callbacks() {
    let node: HirBodyNode<'_> = logic_node(HirBodyKind::Macro(HirMacro {
        name: "announce",
        arguments: HirMacroArguments::Raw("7"),
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
                MacroArgumentKind::ArgumentList,
                MacroExecutionKind::Sync,
                RuntimeMacroHandler::Native("scripts:announce"),
            ),
        );
    let mut callbacks: TestNativeMacroCallbacks = TestNativeMacroCallbacks;
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals)
            .with_native_macros(&mut callbacks);

    let execution: BodyExecution = runtime
        .execute_fragment(&[node])
        .expect("同步 Native Macro 应完成");

    assert_eq!(
        execution.output.nodes(),
        &[SemanticNode::Text(TextValue::from("Native"))]
    );
    drop(runtime);
    assert_eq!(state.count, Value::Number(7.0));
    assert_eq!(locals.args(), None);
}

struct CaptureNativeMacroCallbacks;

impl NativeMacroCallbacks<&'static str, LogicStoryContext> for CaptureNativeMacroCallbacks {
    fn invoke(
        &mut self,
        _handler: &&'static str,
        invocation: MacroInvocation<'_, HirBodyNode<'_>, MacroLogicContext<'_, LogicStoryContext>>,
    ) -> Result<BodyExecution, Diagnostic> {
        let captured: MacroLocalScopes<Value> = invocation.captures.into_scopes();
        assert_eq!(captured.get("selected"), Some(&Value::string("Forest")));
        assert_eq!(captured.get("private"), None);
        assert_eq!(captured.args(), Some(&[][..]));
        Ok(BodyExecution::default())
    }
}

#[test]
fn widget_capture_reaches_nested_native_macro_without_leaking_other_locals() {
    let widget: HirWidget<'_> = HirWidget {
        name: "makeLink",
        body: vec![
            logic_set(r#"@selected = "Forest""#),
            logic_set(r#"@private = "hidden""#),
            logic_node(HirBodyKind::Capture(HirCapture {
                locals: vec!["selected"],
                body: vec![logic_node(HirBodyKind::Macro(HirMacro {
                    name: "announce",
                    arguments: HirMacroArguments::None,
                    syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                    body: Vec::new(),
                }))],
            })),
        ],
    };
    let call: HirBodyNode<'_> = logic_node(HirBodyKind::Macro(HirMacro {
        name: "makeLink",
        arguments: HirMacroArguments::None,
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    }));
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
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
    let mut callbacks: CaptureNativeMacroCallbacks = CaptureNativeMacroCallbacks;
    let mut state: LogicStateContext = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story: LogicStoryContext = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals)
            .with_native_macros(&mut callbacks);

    let control: BodyControl = runtime
        .execute_node(&call)
        .expect("Widget 内的 capture 应传给 Native Macro");

    assert_eq!(control, BodyControl::Continue);
    drop(runtime);
    assert_eq!(locals.args(), None);
}

enum TestAsyncNativeResult {
    Complete,
    Pending(u32),
}

struct TestAsyncNativeMacroCallbacks {
    result: TestAsyncNativeResult,
}

impl AsyncNativeMacroCallbacks<&'static str, LogicStoryContext, u32>
    for TestAsyncNativeMacroCallbacks
{
    fn invoke(
        &mut self,
        handler: &&'static str,
        invocation: MacroInvocation<'_, HirBodyNode<'_>, MacroLogicContext<'_, LogicStoryContext>>,
    ) -> Result<MacroHandlerOutcome<BodyExecution, u32>, Diagnostic> {
        assert_eq!(*handler, "scripts:wait");
        assert_eq!(invocation.name, "wait");
        assert_eq!(invocation.arguments, &[Value::Number(7.0)]);
        assert!(matches!(invocation.body, MacroInvocationBody::Inline));

        match self.result {
            TestAsyncNativeResult::Complete => Ok(MacroHandlerOutcome::Complete(BodyExecution {
                control: BodyControl::Continue,
                output: crate::semantic::SemanticOutput::from_nodes(vec![
                    SemanticNode::Text(TextValue::from("立即完成")),
                ]),
            })),
            TestAsyncNativeResult::Pending(handle) => Ok(MacroHandlerOutcome::Pending(handle)),
        }
    }
}

fn async_native_definition()
-> MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'static, 'static, &'static str>>> {
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'static, 'static, &'static str>>,
    > = MacroDefinitions::new();
    let _previous = definitions.add(
        "wait",
        MacroDefinition::new(
            MacroBodyKind::Inline,
            MacroArgumentKind::ArgumentList,
            MacroExecutionKind::Async,
            RuntimeMacroHandler::Native("scripts:wait"),
        ),
    );
    definitions
}

#[test]
fn runtime_suspends_an_async_native_macro_with_its_identity_and_arguments() {
    let call: HirMacro<'_> = HirMacro {
        name: "wait",
        arguments: HirMacroArguments::Raw("7"),
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    };
    let definitions = async_native_definition();
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(3, 5);
    let mut callbacks = TestAsyncNativeMacroCallbacks {
        result: TestAsyncNativeResult::Pending(41),
    };
    let mut state = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let outcome = runtime
        .execute_async_native_macro(&call, identity, &mut callbacks)
        .expect("Async Native Macro 应返回暂停结果");

    let suspension = match outcome {
        MacroCallOutcome::Pending(suspension) => suspension,
        MacroCallOutcome::Complete(_) => panic!("测试回调应保持 Pending"),
    };
    assert_eq!(suspension.identity, identity);
    assert_eq!(
        suspension.handle,
        RuntimeNativePending {
            name: "wait".to_owned(),
            handle: 41,
        }
    );
    let suspended_locals: MacroLocalScopes<Value> = suspension.scopes.into_scopes();
    assert_eq!(suspended_locals.args(), Some(&[Value::Number(7.0)][..]));
    drop(runtime);
    assert_eq!(locals.args(), None);
}

#[test]
fn runtime_accepts_an_immediately_completed_async_native_macro() {
    let call: HirMacro<'_> = HirMacro {
        name: "wait",
        arguments: HirMacroArguments::Raw("7"),
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    };
    let definitions = async_native_definition();
    let mut callbacks = TestAsyncNativeMacroCallbacks {
        result: TestAsyncNativeResult::Complete,
    };
    let mut state = LogicStateContext {
        count: Value::Number(0.0),
    };
    let mut story = LogicStoryContext::default();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut runtime =
        RuntimeExecutionContext::new(&definitions, &mut state, &mut story, &mut locals);

    let outcome = runtime
        .execute_async_native_macro(&call, RuntimeExecutionIdentity::new(3, 5), &mut callbacks)
        .expect("Async Handler 也可以首次调用时立即完成");

    assert!(matches!(
        outcome,
        MacroCallOutcome::Complete(RuntimeMacroExecution {
            execution: BodyExecution {
                control: BodyControl::Continue,
                ..
            },
            includes_entered: 0,
        })
    ));
    drop(runtime);
    assert_eq!(locals.args(), None);
}

struct AsyncNativeAfterLifecycle;

impl MacroLifecycleCallbacks for AsyncNativeAfterLifecycle {
    fn before(&mut self, _name: &str, _arguments: &mut [Value]) -> Result<(), Diagnostic> {
        panic!("恢复 Async Native Macro 不应再次执行 before")
    }

    fn after(
        &mut self,
        name: &str,
        arguments: &[Value],
        mut output: crate::semantic::SemanticOutput,
    ) -> Result<crate::semantic::SemanticOutput, Diagnostic> {
        assert_eq!(name, "wait");
        assert_eq!(arguments, &[Value::Number(7.0)]);
        output.push(SemanticNode::Text(TextValue::from("!")));
        Ok(output)
    }
}

fn async_native_suspension(handle: u32) -> MacroSuspension<RuntimeNativePending<u32>> {
    let mut scopes: MacroLocalScopes<Value> = MacroLocalScopes::new();
    scopes.enter_call(vec![Value::Number(7.0)]);
    MacroSuspension {
        identity: RuntimeExecutionIdentity::new(3, 5),
        handle: RuntimeNativePending {
            name: "wait".to_owned(),
            handle,
        },
        scopes: scopes.suspend().expect("活动调用帧应可暂停"),
    }
}

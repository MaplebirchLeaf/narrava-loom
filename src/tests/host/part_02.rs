// host.rs 测试分片 02：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn host_start_returns_only_current_identity_and_presentation() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: Vec::<HirBodyNode<'_>>::new(),
        }],
    };
    let mut state: State = State::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);

    let update: HostUpdate = HostApi::start(
        &mut state,
        &mut story,
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |_passage, _state, _requests, _limits| {
            Ok::<BodyExecution, Diagnostic>(BodyExecution {
                control: BodyControl::Continue,
                output: PresentationOutput::from_nodes(vec![PresentationNode::Text(
                    TextValue::from("森林入口"),
                )]),
            })
        },
    )
    .expect("Host 应能启动 Story");

    assert_eq!(update.current(), "Start");
    assert_eq!(
        update.presentation().nodes(),
        &[PresentationNode::Text(TextValue::from("森林入口"))]
    );
}

#[test]
fn host_navigation_input_is_validated_and_executed_by_engine() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::<HirBodyNode<'_>>::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Forest",
                tags: Vec::new(),
                body: Vec::<HirBodyNode<'_>>::new(),
            },
        ],
    };
    let mut state: State = State::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let mut pending: HostPendingExecutions<EngineMirContinuation<'_, '_, u64>> =
        HostPendingExecutions::new();
    let interaction: InteractionId = InteractionId::from_key("start:choice:0");
    let presented: HostUpdate = HostApi::start(
        &mut state,
        &mut story,
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |_passage, _state, _requests, _limits| {
            Ok::<BodyExecution, Diagnostic>(BodyExecution {
                control: BodyControl::Continue,
                output: PresentationOutput::from_nodes(vec![PresentationNode::Navigation {
                    role: crate::presentation::NavigationRole::Link,
                    id: interaction.clone(),
                    label: TextValue::from("进入森林"),
                    target: String::from("Forest"),
                }]),
            })
        },
    )
    .expect("测试应先进入 Start");
    let params: Value = Value::Null;
    let result: HostDriveResult = HostApi::advance_mir(
        &mut pending,
        &mut state,
        &mut story,
        &bytecode,
        HostMirAdvanceRequest {
            presented: &presented,
            input: HostInput::activate(interaction),
            params: &params,
            identity: RuntimeExecutionIdentity::new(1, 2),
            limits: EngineExecutionLimits {
                passages: 1,
                includes: 0,
            },
            language: None,
        },
        |_phase, _context, _state| Ok::<(), Diagnostic>(()),
        |_invocation,
         _state,
         _requests,
         _scopes|
         -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, u64>,
            crate::engine::EngineMirMacroCallbackFailure<&'static str>,
        > { panic!("空 Passage 不应请求 Macro 分派") },
    )
    .expect("Host 导航动作应交给 Engine 执行");
    let HostDriveResult::Ready(update) = result else {
        panic!("空目标 Passage 应直接产生 HostUpdate");
    };

    assert_eq!(update.current(), "Forest");
    assert!(matches!(
        update.presentation().nodes(),
        [PresentationNode::SafeReturn { target, .. }] if target == "Start"
    ));
    assert_eq!(state.variables_get("unused"), None::<&Value>);
}

#[test]
fn host_rejects_an_interaction_not_presented_by_core() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: Vec::<HirBodyNode<'_>>::new(),
        }],
    };
    let mut state: State = State::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let presented: HostUpdate = HostApi::start(
        &mut state,
        &mut story,
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |_passage, _state, _requests, _limits| {
            Ok::<BodyExecution, Diagnostic>(BodyExecution::default())
        },
    )
    .expect("测试应先进入 Start");
    let unknown: InteractionId = InteractionId::from_key("forged");

    let error: Diagnostic = HostApi::advance(
        &mut state,
        &mut story,
        &presented,
        HostInput::activate(unknown.clone()),
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |_passage, _state, _requests, _limits| {
            Ok::<BodyExecution, Diagnostic>(BodyExecution::default())
        },
    )
    .expect_err("Host 不能提交 Core 未呈现的交互身份");

    assert_eq!(error.code, "host.unknown_interaction");
    assert_eq!(error.severity, DiagnosticSeverity::Error);
    assert!(error.message.contains(unknown.as_str()));
    assert_eq!(story.current().map(|passage| passage.name), Some("Start"));

    let async_error: Diagnostic = HostApi::advance(
        &mut state,
        &mut story,
        &presented,
        HostInput::resume(HostExecutionToken::from_identity(
            RuntimeExecutionIdentity::new(1, 1),
        )),
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |_passage, _state, _requests, _limits| {
            Ok::<BodyExecution, Diagnostic>(BodyExecution::default())
        },
    )
    .expect_err("同步 advance 不得消费异步恢复输入");
    assert_eq!(async_error.code, "host.async_input.requires_pending");
    assert_eq!(story.current().map(|passage| passage.name), Some("Start"));
}

#[test]
fn host_start_converts_missing_start_to_a_stable_diagnostic() {
    let compiled: HirStory<'_> = HirStory {
        passages: Vec::<HirPassage<'_>>::new(),
    };
    let mut state: State = State::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);

    let error: Diagnostic = HostApi::start(
        &mut state,
        &mut story,
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |_passage, _state, _requests, _limits| {
            Ok::<BodyExecution, Diagnostic>(BodyExecution::default())
        },
    )
    .expect_err("缺少 Start 时 Host 应返回稳定诊断");

    assert_eq!(error.code, "story.navigation.missing_passage");
    assert_eq!(error.severity, DiagnosticSeverity::Error);
}

#[test]
fn host_preserves_runtime_diagnostic() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: Vec::<HirBodyNode<'_>>::new(),
        }],
    };
    let mut state: State = State::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let runtime_error: Diagnostic = Diagnostic::new(
        "runtime.test_failure",
        DiagnosticSeverity::Error,
        "Runtime 测试失败",
    );

    let error: Diagnostic = HostApi::start(
        &mut state,
        &mut story,
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |_passage, _state, _requests, _limits| Err(runtime_error.clone()),
    )
    .expect_err("Runtime 失败应越过 Host 边界");

    assert_eq!(error, runtime_error);
}

#[test]
fn host_state_view_reads_each_namespace_without_owning_state() {
    let mut state: State = State::new();
    let _global_previous: Option<Value> = state.global_set("gameName", Value::string("Narrava"));
    let _setup_previous: Value = state.setup_set(Value::object(vec![(
        String::from("difficulty"),
        Value::string("normal"),
    )]));
    let _variable_previous: Option<Value> = state.variables_set("score", Value::Number(8.0));
    let _temporary_previous: Option<Value> =
        state.temporary_set("selection", Value::string("Forest"));

    let view: HostStateView<'_> = HostApi::state(&state);

    assert_eq!(view.global("gameName"), Some(&Value::string("Narrava")));
    assert_eq!(
        view.setup(),
        &Value::object(vec![(String::from("difficulty"), Value::string("normal"))])
    );
    assert_eq!(view.variable("score"), Some(&Value::Number(8.0)));
    assert_eq!(view.temporary("selection"), Some(&Value::string("Forest")));
}

#[test]
fn host_async_inputs_carry_only_a_stable_execution_token() {
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(7, 11);
    let token: HostExecutionToken = HostExecutionToken::from_identity(identity);

    assert_eq!(token.identity(), identity);
    assert_eq!(
        HostInput::resume(token),
        HostInput::Resume { execution: token }
    );
    assert_eq!(
        HostInput::cancel(token),
        HostInput::Cancel { execution: token }
    );
}

#[test]
fn host_pending_store_never_overwrites_or_double_consumes_an_execution() {
    let token: HostExecutionToken =
        HostExecutionToken::from_identity(RuntimeExecutionIdentity::new(7, 11));
    let mut pending: HostPendingExecutions<String> = HostPendingExecutions::new();

    pending
        .add(token, "first".to_owned())
        .expect("首次保存应成功");
    let duplicate = pending
        .add(token, "second".to_owned())
        .expect_err("重复 Token 不得覆盖活动执行");

    assert_eq!(duplicate.token, token);
    assert_eq!(duplicate.pending, "second");
    assert!(pending.has(token));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending.take(token).as_deref(), Some("first"));
    assert_eq!(pending.take(token), None);
    assert!(pending.is_empty());
}

#[test]
fn host_cancel_rejects_an_unknown_execution_without_changing_domains() {
    let compiled: HirStory<'_> = HirStory {
        passages: Vec::<HirPassage<'_>>::new(),
    };
    let mut state: State = State::new();
    let _previous: Option<Value> = state.variables_set("score", Value::Number(9.0));
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut pending: HostPendingExecutions<EngineMirContinuation<'_, '_, u64>> =
        HostPendingExecutions::new();
    let token = HostExecutionToken::from_identity(RuntimeExecutionIdentity::new(7, 11));

    let error = HostApi::cancel_pending(&mut pending, &mut state, &mut story, token)
        .expect_err("未知 Token 不得伪造取消结果");

    assert_eq!(error.diagnostic.code, "host.pending.unknown_execution");
    assert_eq!(error.pending, None);
    assert_eq!(state.variables_get("score"), Some(&Value::Number(9.0)));
    assert!(story.history().is_empty());

    let mir = crate::mir::MirStory::lower(&compiled).expect("空 Story 仍可建立 MIR 映射");
    let lir: crate::lir::LirProgram<'_, '_, '_> =
        crate::lir::LirProgram::lower(&mir).expect("空 MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let resume_error = HostApi::resume_pending(
        &mut pending,
        &mut state,
        &mut story,
        &bytecode,
        token,
        |_handle, _state, _requests, _locals| {
            Ok::<_, &'static str>(crate::macro_runtime::MacroHandlerOutcome::Pending(1_u64))
        },
    )
    .err()
    .expect("未知 Token 不得伪造恢复结果");
    assert_eq!(resume_error.code, "host.pending.unknown_execution");
    assert_eq!(state.variables_get("score"), Some(&Value::Number(9.0)));
}

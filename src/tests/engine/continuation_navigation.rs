// Continuation 导航、回滚及执行限制。
// 本文件被上级 `engine` 测试模块直接包含，以共享统一的测试夹具。
#[test]
fn engine_mir_continuation_resumes_goto_and_commits_the_target_passage() {
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
                body: vec![
                    HirBodyNode {
                        kind: HirBodyKind::Capture(HirCapture {
                            locals: vec!["selected"],
                            body: vec![HirBodyNode {
                                kind: HirBodyKind::Macro(HirMacro {
                                    name: "wait",
                                    arguments: HirMacroArguments::None,
                                    syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                                    body: Vec::new(),
                                }),
                                span: TweeSpan {
                                    start: 0,
                                    end: 1,
                                    line: 1,
                                    column: 1,
                                },
                            }],
                        }),
                        span: TweeSpan {
                            start: 0,
                            end: 1,
                            line: 1,
                            column: 1,
                        },
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Macro(HirMacro {
                            name: "second",
                            arguments: HirMacroArguments::None,
                            syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                            body: Vec::new(),
                        }),
                        span: TweeSpan {
                            start: 1,
                            end: 2,
                            line: 1,
                            column: 2,
                        },
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Goto(Box::new(
                            parse(r#""End""#).expect("goto 目标应有效"),
                        )),
                        span: TweeSpan {
                            start: 2,
                            end: 3,
                            line: 1,
                            column: 3,
                        },
                    },
                ],
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Text("终点"),
                    span: TweeSpan {
                        start: 2,
                        end: 3,
                        line: 2,
                        column: 1,
                    },
                }],
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("Macro Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let translation: I18nValidatedTemplate = mir
        .i18n()
        .validate(I18nTemplate::new(
            "en",
            BTreeMap::new(),
            BTreeMap::from([(
                String::from("p3:End:body.0"),
                I18nTemplateMessage::new("终点", "Destination", BTreeMap::new()),
            )]),
        ))
        .expect("恢复链目标语言应通过校验");
    let mut state: State = State::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let _previous: Option<Value> = state.variables_set("changed", Value::Boolean(true));
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(9, 1);
    let limits: EngineExecutionLimits = EngineExecutionLimits {
        passages: 4,
        includes: 2,
    };
    let params: Value = Value::string("entry params");
    let phases: RefCell<Vec<PassageLifecyclePhase>> = RefCell::new(Vec::new());
    let started = Engine::begin_mir_chain(
        &mut state,
        &mut story,
        &bytecode,
        EngineMirBeginRequest {
            name: "Start",
            params: &params,
            identity,
            limits,
            language: Some(&I18nRuntimeLanguage::Translation(translation)),
        },
        |phase, _context, _state| {
            phases.borrow_mut().push(phase);
            Ok::<(), &'static str>(())
        },
    )
    .unwrap_or_else(|_| panic!("首次 Engine VM 应运行到 MacroPending"));
    let EngineMirVmResume::MacroPending(started) = started else {
        panic!("Start 的首个动态 Macro 应形成稳定边界");
    };
    let dispatched = EngineMirVmResume::MacroPending(started)
        .dispatch_macro(
            &bytecode,
            &mut state,
            &story,
            |invocation, _state, _requests, mut scopes| {
                assert_eq!(invocation.identity, identity);
                let captured_scopes: MacroLocalScopes<Value> = invocation.captures.into_scopes();
                assert_eq!(captured_scopes.get("selected"), None);
                scopes.enter_call(vec![Value::string("argument")]);
                Ok::<_, crate::engine::EngineMirMacroCallbackFailure<&'static str>>(
                    MacroResumeOutcome::Pending(MacroSuspension {
                        identity,
                        handle: 17_u64,
                        scopes: scopes.suspend().expect("活动 Macro 应能暂停"),
                    }),
                )
            },
        )
        .unwrap_or_else(|_| panic!("首次异步分派应构造完整 continuation"));
    let EngineMirMacroDispatch::Pending(continuation) = dispatched else {
        panic!("测试 Macro 应保持 Pending");
    };

    let current: StoryHistoryEntry<'_, '_> = continuation.progress().current();
    assert_eq!(
        *phases.borrow(),
        vec![PassageLifecyclePhase::Init, PassageLifecyclePhase::Start]
    );
    assert_eq!(continuation.runtime().identity(), identity);
    assert_eq!(continuation.progress().current(), current);
    assert_eq!(continuation.progress().params(), &params);
    assert_eq!(continuation.progress().limits(), limits);
    let token: HostExecutionToken = HostExecutionToken::from_identity(identity);
    let mut host_pending: HostPendingExecutions<EngineMirContinuation<'_, '_, u64>> =
        HostPendingExecutions::new();
    assert!(host_pending.add(token, continuation).is_ok());
    let driven = HostApi::resume_and_drive(
        &mut host_pending,
        &mut state,
        &mut story,
        &bytecode,
        token,
        HostResumeCallbacks::new(
            |phase: PassageLifecyclePhase,
             context: PassageLifecycleContext<'_, '_, '_, '_>,
             _state: &mut State| {
                phases.borrow_mut().push(phase);
                if matches!(
                    phase,
                    PassageLifecyclePhase::Render | PassageLifecyclePhase::Display
                ) {
                    assert_eq!(context.entry().passage().name, "End");
                    assert_eq!(context.output().map(PresentationOutput::len), Some(1));
                }
                Ok::<(), Diagnostic>(())
            },
            |handle: u64,
             _state: &mut State,
             requests: &mut StoryRuntimeRequests<'_, '_, '_>,
             locals: &mut MacroLocalScopes<Value>| {
                assert_eq!(handle, 17);
                assert!(requests.pending_goto().is_none());
                assert!(locals.args().is_some());
                Ok::<_, &'static str>(MacroHandlerOutcome::Complete(RuntimeMacroExecution {
                    execution: BodyExecution {
                        control: BodyControl::Continue,
                        output: PresentationOutput::from_nodes(vec![PresentationNode::Text(
                            TextValue::from("恢复输出"),
                        )]),
                    },
                    includes_entered: 2,
                }))
            },
            |invocation: EngineMirMacroInvocation<'_>,
             _state: &mut State,
             _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
             scopes: MacroLocalScopes<Value>| {
                assert_eq!(invocation.call.name, "second");
                assert_eq!(invocation.identity, identity);
                Ok::<_, crate::engine::EngineMirMacroCallbackFailure<&'static str>>(
                    MacroResumeOutcome::Complete {
                        output: RuntimeMacroExecution {
                            execution: BodyExecution {
                                control: BodyControl::Continue,
                                output: PresentationOutput::from_nodes(vec![
                                    PresentationNode::Text(TextValue::from("第二输出")),
                                ]),
                            },
                            includes_entered: 0,
                        },
                        scopes,
                    },
                )
            },
        ),
    )
    .unwrap_or_else(|_| panic!("Host 应自动分派 Macro、继续导航并提交 Halted 边界"));
    let HostDriveResult::Ready(committed) = driven else {
        panic!("同步完成的后续 Macro 不应留下 Pending");
    };

    assert_eq!(
        phases.into_inner(),
        vec![
            PassageLifecyclePhase::Init,
            PassageLifecyclePhase::Start,
            PassageLifecyclePhase::End,
            PassageLifecyclePhase::Init,
            PassageLifecyclePhase::Start,
            PassageLifecyclePhase::Render,
            PassageLifecyclePhase::Display,
        ]
    );
    assert_eq!(story.history().len(), 2);
    assert_eq!(story.history()[0], current);
    assert_eq!(committed.current(), "End");
    assert_eq!(committed.presentation().len(), 3);
    assert!(matches!(
        committed.presentation().nodes().last(),
        Some(PresentationNode::Text(text))
            if text.to_unicode_string().as_deref() == Some("Destination")
    ));
    assert_eq!(state.variables_get("changed"), Some(&Value::Boolean(true)));
    assert_eq!(story.current().map(|entry| entry.name), Some("End"));
}

#[test]
fn engine_confirms_runtime_goto_only_after_the_current_passage_stops() {
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
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Goto(Box::new(
                        parse(r#""End""#).expect("goto 目标应可解析"),
                    )),
                    span: TweeSpan {
                        start: 0,
                        end: 0,
                        line: 1,
                        column: 1,
                    },
                }],
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Set(Box::new(
                        parse("$executedEnd = true").expect("End set 应可解析"),
                    )),
                    span: TweeSpan {
                        start: 0,
                        end: 0,
                        line: 1,
                        column: 1,
                    },
                }],
            },
        ],
    };
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let navigation: EngineRequestedNavigation<'_, '_> = Engine::navigate_with_requests(
        &mut state,
        &mut story,
        "Start",
        |passage: &HirPassage<'_>,
         state: &mut State,
         requests: &mut StoryRuntimeRequests<'_, '_, '_>| {
            let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
                RuntimeExecutionContext::new(&definitions, state, requests, &mut locals);
            runtime.execute_passage(passage)
        },
    )
    .expect("StopPassage 与 pending goto 应确认一跳导航");

    assert_eq!(navigation.entered.passage().name, "Start");
    assert_eq!(
        navigation.requested.map(|entry| entry.passage().name),
        Some("End")
    );
    assert_eq!(story.current().map(|passage| passage.name), Some("End"));
    assert_eq!(story.history().len(), 2);
    assert_eq!(state.variables_get("executedEnd"), None);
}

#[test]
fn engine_rolls_back_when_stop_passage_has_no_pending_goto() {
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
            body: Vec::new(),
        }],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    let _score: Option<Value> = state.variables_set("score", Value::Number(1.0));

    let result: Result<
        EngineRequestedNavigation<'_, '_>,
        EngineNavigationError<EngineRequestedExecutionError<&'static str>>,
    > = Engine::navigate_with_requests(
        &mut state,
        &mut story,
        "Start",
        |_passage: &HirPassage<'_>,
         state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>| {
            let _score: Option<Value> = state.variables_set("score", Value::Number(9.0));
            Ok(execution(BodyControl::StopPassage))
        },
    );

    assert_eq!(
        result,
        Err(EngineNavigationError::Execution(
            EngineRequestedExecutionError::MissingGotoRequest
        ))
    );
    assert_eq!(state.variables_get("score"), Some(&Value::Number(1.0)));
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

#[test]
fn engine_rolls_back_when_pending_goto_does_not_stop_the_passage() {
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
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    let _score: Option<Value> = state.variables_set("score", Value::Number(1.0));

    let result: Result<
        EngineRequestedNavigation<'_, '_>,
        EngineNavigationError<EngineRequestedExecutionError<&'static str>>,
    > = Engine::navigate_with_requests(
        &mut state,
        &mut story,
        "Start",
        |_passage: &HirPassage<'_>,
         state: &mut State,
         requests: &mut StoryRuntimeRequests<'_, '_, '_>| {
            requests.goto("End").expect("End 导航请求应有效");
            let _score: Option<Value> = state.variables_set("score", Value::Number(9.0));
            Ok(execution(BodyControl::Continue))
        },
    );

    assert_eq!(
        result,
        Err(EngineNavigationError::Execution(
            EngineRequestedExecutionError::UnexpectedGotoRequest
        ))
    );
    assert_eq!(state.variables_get("score"), Some(&Value::Number(1.0)));
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

#[test]
fn engine_executes_confirmed_goto_target_in_the_same_bounded_chain() {
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
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Goto(Box::new(
                        parse(r#""End""#).expect("goto 目标应可解析"),
                    )),
                    span: TweeSpan {
                        start: 0,
                        end: 0,
                        line: 1,
                        column: 1,
                    },
                }],
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Set(Box::new(
                        parse("$executedEnd = true").expect("End set 应可解析"),
                    )),
                    span: TweeSpan {
                        start: 0,
                        end: 0,
                        line: 1,
                        column: 1,
                    },
                }],
            },
        ],
    };
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let navigation: EngineNavigationChain<'_, '_> = Engine::navigate_chain_with_requests(
        &mut state,
        &mut story,
        "Start",
        EngineExecutionLimits {
            passages: 2,
            includes: 4,
        },
        |passage: &HirPassage<'_>,
         state: &mut State,
         requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         limits: EngineExecutionLimits| {
            if passage.name == "Start" {
                let _marker: Option<Value> =
                    state.temporary_set("previousPassage", Value::Boolean(true));
            } else {
                assert_eq!(state.temporary_get("previousPassage"), None);
            }
            let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
                RuntimeExecutionContext::new(&definitions, state, requests, &mut locals);
            runtime.execute_passage_with_includes(passage, limits.includes)
        },
    )
    .expect("两步上限应允许执行 Start 和 End");

    let names: Vec<&str> = navigation
        .entries
        .iter()
        .map(|entry: &StoryHistoryEntry<'_, '_>| entry.passage().name)
        .collect();
    assert_eq!(names, vec!["Start", "End"]);
    assert_eq!(
        state.variables_get("executedEnd"),
        Some(&Value::Boolean(true))
    );
    assert_eq!(story.current().map(|passage| passage.name), Some("End"));
}

#[test]
fn engine_rolls_back_a_navigation_chain_that_exceeds_its_passage_limit() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "A",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "B",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    let _score: Option<Value> = state.variables_set("score", Value::Number(1.0));

    let result: Result<
        EngineNavigationChain<'_, '_>,
        EngineNavigationError<EngineRequestedExecutionError<&'static str>>,
    > = Engine::navigate_chain_with_requests(
        &mut state,
        &mut story,
        "A",
        EngineExecutionLimits {
            passages: 2,
            includes: 0,
        },
        |passage: &HirPassage<'_>,
         state: &mut State,
         requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            let target: &str = if passage.name == "A" { "B" } else { "A" };
            requests.goto(target).expect("循环目标应存在");
            let _score: Option<Value> = state.variables_set("score", Value::Number(9.0));
            Ok(execution(BodyControl::StopPassage))
        },
    );

    assert_eq!(
        result,
        Err(EngineNavigationError::Execution(
            EngineRequestedExecutionError::PassageLimitExceeded { limit: 2 }
        ))
    );
    assert_eq!(state.variables_get("score"), Some(&Value::Number(1.0)));
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

#[test]
fn engine_does_not_silently_discard_unconsumed_include_requests() {
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
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Included",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let result: Result<
        EngineRequestedNavigation<'_, '_>,
        EngineNavigationError<EngineRequestedExecutionError<&'static str>>,
    > = Engine::navigate_with_requests(
        &mut state,
        &mut story,
        "Start",
        |_passage: &HirPassage<'_>,
         _state: &mut State,
         requests: &mut StoryRuntimeRequests<'_, '_, '_>| {
            requests
                .include("Included")
                .expect("include 请求应先由 Adapter 接收");
            Ok(execution(BodyControl::Continue))
        },
    );

    assert_eq!(
        result,
        Err(EngineNavigationError::Execution(
            EngineRequestedExecutionError::UnconsumedIncludeRequests { count: 1 }
        ))
    );
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

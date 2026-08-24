// 宏交互取消、Passage 生命周期与回调顺序。
// 本文件被上级 `engine` 测试模块直接包含，以共享统一的测试夹具。
#[test]
fn engine_macro_interaction_cancel_restores_domains_and_returns_owners() {
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
                name: "Other",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let story_mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("Story 应进入 MIR");
    let story_lir: LirProgram<'_, '_, '_> =
        LirProgram::lower(&story_mir).expect("Story MIR 应进入 LIR");
    let story_bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&story_lir);
    story.goto("Start").expect("测试应先进入 Start");
    let mut state: State = State::new();
    let state_checkpoint = state.checkpoint();
    let story_snapshot = story.snapshot();
    let requests = StoryRuntimeRequests::new(&story).into_pending();
    let body: Vec<HirBodyNode<'_>> = vec![
        HirBodyNode {
            kind: HirBodyKind::Macro(HirMacro {
                name: "wait",
                arguments: HirMacroArguments::None,
                syntax_kind: MacroSyntaxKind::Inline,
                body: Vec::new(),
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
                name: "wait",
                arguments: HirMacroArguments::None,
                syntax_kind: MacroSyntaxKind::Inline,
                body: Vec::new(),
            }),
            span: TweeSpan {
                start: 1,
                end: 2,
                line: 1,
                column: 2,
            },
        },
    ];
    let mir: MirMacroBody<'_, '_> = MirMacroBody::lower(&body).expect("正文应进入 MIR");
    let macro_bytecode: crate::bytecode::BytecodeMacroBody =
        crate::bytecode::BytecodeMacroBody::compile(&mir);
    let mut frame: MirExecutionFrame = MirExecutionFrame::new_macro(&macro_bytecode);
    assert_eq!(
        frame.step_macro(&macro_bytecode, &mut state),
        Ok(MirStep::MacroPending)
    );
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(31, 1);
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let suspension: MacroSuspension<u64> = MacroSuspension {
        identity,
        handle: 77,
        scopes: locals.suspend().expect("调用帧应可暂停"),
    };
    let runtime: RuntimeMacroBodyContinuation<u64> =
        RuntimeMacroBodyContinuation::new(identity, frame, suspension, &mir)
            .expect("Runtime continuation 应有效");
    let action: MacroInteraction<'_, '_> =
        MacroInteraction::new("Other", &body, CapturedMacroLocals::empty());
    let continuation: EngineMacroInteractionContinuation<'_, '_, u64> =
        EngineMacroInteractionContinuation::new(
            runtime,
            mir,
            action,
            EngineMacroInteractionTransaction::new(
                state_checkpoint,
                story_snapshot,
                requests,
                &Value::Null,
                EngineExecutionLimits {
                    passages: 1,
                    includes: 0,
                },
            ),
        );
    let _previous: Option<Value> = state.variables_set("leaked", Value::Boolean(true));
    story.goto("Other").expect("测试应制造待回滚导航");
    let interaction_id: InteractionId = InteractionId::from_key("link:engine:async");
    let token: HostExecutionToken = HostExecutionToken::from_identity(identity);
    let mut host_pending: HostPendingExecutions<HostMacroInteractionPending<'_, '_, u64>> =
        HostPendingExecutions::new();
    let saved = host_pending.add(
        token,
        HostMacroInteractionPending::new(interaction_id.clone(), continuation),
    );
    assert!(saved.is_ok(), "Host 应保存 Interaction continuation");
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();

    let result = HostApi::resume_macro_interaction_pending(
        &mut host_pending,
        &mut interactions,
        &mut state,
        &mut story,
        token,
        |pending, _state, _requests, _locals| {
            assert_eq!(pending, 77);
            Ok::<_, &'static str>(MacroHandlerOutcome::Pending(88))
        },
    );
    let Ok(resumed) = result else {
        panic!("Host 再次 Pending 应归还完整 continuation");
    };
    let HostMacroInteractionResume::Pending { execution } = resumed else {
        panic!("测试恢复应再次等待");
    };
    assert_eq!(execution, token);

    let cancelled = HostApi::cancel_macro_interaction_pending(
        &mut host_pending,
        &mut interactions,
        &mut state,
        &mut story,
        token,
    )
    .expect("Host 取消应回滚并恢复动作");

    assert_eq!(cancelled.pending, 88);
    assert_eq!(cancelled.execution, token);
    assert_eq!(
        interactions
            .get(&interaction_id)
            .map(MacroInteraction::target),
        Some("Other")
    );
    assert_eq!(state.variables_get("leaked"), None);
    assert_eq!(story.current().map(|entry| entry.name), Some("Start"));

    let mir: MirMacroBody<'_, '_> = MirMacroBody::lower(&body).expect("正文应再次进入 MIR");
    let macro_bytecode: crate::bytecode::BytecodeMacroBody =
        crate::bytecode::BytecodeMacroBody::compile(&mir);
    let mut frame: MirExecutionFrame = MirExecutionFrame::new_macro(&macro_bytecode);
    assert_eq!(
        frame.step_macro(&macro_bytecode, &mut state),
        Ok(MirStep::MacroPending)
    );
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(31, 2);
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(Vec::new());
    let runtime: RuntimeMacroBodyContinuation<u64> = RuntimeMacroBodyContinuation::new(
        identity,
        frame,
        MacroSuspension {
            identity,
            handle: 91,
            scopes: locals.suspend().expect("调用帧应可暂停"),
        },
        &mir,
    )
    .expect("第二条 Runtime continuation 应有效");
    let continuation: EngineMacroInteractionContinuation<'_, '_, u64> =
        EngineMacroInteractionContinuation::new(
            runtime,
            mir,
            MacroInteraction::new("Other", &body, CapturedMacroLocals::empty()),
            EngineMacroInteractionTransaction::new(
                state.checkpoint(),
                story.snapshot(),
                StoryRuntimeRequests::new(&story).into_pending(),
                &Value::Null,
                EngineExecutionLimits {
                    passages: 1,
                    includes: 0,
                },
            ),
        );
    let second_interaction_id: InteractionId =
        InteractionId::from_key("link:engine:async:complete");
    let second_token: HostExecutionToken = HostExecutionToken::from_identity(identity);
    let saved = host_pending.add(
        second_token,
        HostMacroInteractionPending::new(second_interaction_id, continuation),
    );
    assert!(saved.is_ok(), "Host 应保存第二条 Interaction continuation");
    let result = HostApi::resume_macro_interaction_pending(
        &mut host_pending,
        &mut interactions,
        &mut state,
        &mut story,
        second_token,
        |pending, _state, _requests, _locals| {
            assert_eq!(pending, 91);
            Ok::<_, &'static str>(MacroHandlerOutcome::Complete(RuntimeMacroExecution {
                execution: BodyExecution::default(),
                includes_entered: 0,
            }))
        },
    );
    let Ok(HostMacroInteractionResume::Continue(resumed)) = result else {
        panic!("完成的 Handler 应交出 Host 可驱动的正文事务");
    };
    let mut passage_pending: HostPendingExecutions<EngineMirContinuation<'_, '_, u64>> =
        HostPendingExecutions::new();
    let driven = HostApi::drive_macro_interaction(
        *resumed,
        HostMacroInteractionDriveContext::new(
            &mut host_pending,
            &mut passage_pending,
            &mut interactions,
        ),
        &mut state,
        &mut story,
        &story_bytecode,
        |_phase, _context, _state| Ok::<(), Diagnostic>(()),
        |invocation, _state, _requests, scopes| {
            assert_eq!(invocation.call.name, "wait");
            Ok::<_, EngineMirMacroCallbackFailure<&'static str>>(MacroResumeOutcome::Complete {
                output: RuntimeMacroExecution {
                    execution: BodyExecution::default(),
                    includes_entered: 0,
                },
                scopes,
            })
        },
    )
    .unwrap_or_else(|_| panic!("Host 应驱动正文并进入目标 Passage"));
    let HostDriveResult::Ready(update) = driven else {
        panic!("同步目标 Passage 应产生可呈现更新");
    };
    assert_eq!(update.current(), "Other");
    assert!(host_pending.is_empty());
    assert!(passage_pending.is_empty());
    assert_eq!(story.current().map(|entry| entry.name), Some("Other"));
}

#[test]
fn passage_lifecycle_exposes_five_ordered_passage_phases() {
    let phases: [PassageLifecyclePhase; 5] = PassageLifecyclePhase::ORDERED;

    assert_eq!(
        phases,
        [
            PassageLifecyclePhase::Init,
            PassageLifecyclePhase::Start,
            PassageLifecyclePhase::Render,
            PassageLifecyclePhase::Display,
            PassageLifecyclePhase::End,
        ]
    );
}

#[test]
fn passage_lifecycle_context_keeps_params_separate_from_writable_state() {
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
    let entry: StoryHistoryEntry<'_, '_> = *story.goto("Start").expect("测试 Passage 应存在");
    let params: Value = Value::String(String::from("Map").into());
    let context: PassageLifecycleContext<'_, '_, '_, '_> =
        PassageLifecycleContext::new(entry, &params);
    let mut state: State = State::new();

    let passage_name: String = context.entry().passage().name.to_owned();
    let _location: Option<Value> =
        state.variables_set("location", Value::String(passage_name.into()));
    let _entered_from: Option<Value> = state.temporary_set("enteredFrom", context.params().clone());

    assert_eq!(context.params(), &Value::String(String::from("Map").into()));
    assert_eq!(
        state.variables_get("location"),
        Some(&Value::String(String::from("Start").into()))
    );
    assert_eq!(
        state.temporary_get("enteredFrom"),
        Some(&Value::String(String::from("Map").into()))
    );
}

#[test]
fn engine_lifecycle_runs_init_start_and_end_around_each_navigated_passage() {
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
    let params: Value = Value::String(String::from("Menu").into());
    let events: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let navigation: EngineNavigationChain<'_, '_> = Engine::navigate_chain_with_lifecycle(
        &mut state,
        &mut story,
        "Start",
        &params,
        EngineExecutionLimits {
            passages: 2,
            includes: 0,
        },
        |phase: PassageLifecyclePhase,
         context: PassageLifecycleContext<'_, '_, '_, '_>,
         state: &mut State| {
            events
                .borrow_mut()
                .push(format!("{phase:?}:{}", context.entry().passage().name));
            if phase == PassageLifecyclePhase::Init {
                let expected: &Value = if context.entry().passage().name == "Start" {
                    &params
                } else {
                    &Value::Undefined
                };
                assert_eq!(context.params(), expected);
                let _phase: Option<Value> =
                    state.temporary_set("phase", Value::String(String::from("init").into()));
            }
            Ok::<(), &'static str>(())
        },
        |passage: &HirPassage<'_>,
         state: &mut State,
         requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            assert_eq!(
                state.temporary_get("phase"),
                Some(&Value::String(String::from("init").into()))
            );
            if passage.name == "Start" {
                requests.goto("End").expect("End 导航请求应有效");
                Ok(execution(BodyControl::StopPassage))
            } else {
                Ok(execution(BodyControl::Continue))
            }
        },
    )
    .expect("生命周期回调应包围两段导航 Passage");

    assert_eq!(navigation.entries.len(), 2);
    assert_eq!(
        events.into_inner(),
        vec![
            String::from("Init:Start"),
            String::from("Start:Start"),
            String::from("End:Start"),
            String::from("Init:End"),
            String::from("Start:End"),
            String::from("Render:End"),
            String::from("Display:End"),
        ]
    );
}

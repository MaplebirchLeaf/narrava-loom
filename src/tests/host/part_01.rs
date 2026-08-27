// host.rs 测试分片 01：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn host_start_forwards_the_fallback_chain_into_the_engine_transaction() {
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
            body: vec![
                HirBodyNode {
                    kind: HirBodyKind::Text("Found "),
                    span: Span {
                        start: 0,
                        end: 1,
                        line: 1,
                        column: 1,
                    },
                },
                HirBodyNode {
                    kind: HirBodyKind::Print(HirPrint::Expression(
                        parse("$item").expect("物品表达式应有效"),
                    )),
                    span: Span {
                        start: 1,
                        end: 2,
                        line: 1,
                        column: 2,
                    },
                },
            ],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let chain: I18nLanguageChain = I18nLanguageChain::validate(
        mir.i18n(),
        "en",
        vec![
            host_language_package("zh-Hant", Some("zh"), ""),
            host_language_package("zh", None, "发现{$item}"),
        ],
    )
    .expect("Host fallback 链应通过校验");
    let mut state: State = State::new();
    let _previous: Option<Value> =
        state.variables_set("item", Value::String(TextValue::from("铁剑")));
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut pending: HostPendingExecutions<EngineMirContinuation<'_, '_, u64>> =
        HostPendingExecutions::new();
    let params: Value = Value::Null;

    let result: HostDriveResult = HostApi::start_mir(
        &mut pending,
        &mut state,
        &mut story,
        &bytecode,
        HostMirRequest {
            params: &params,
            identity: RuntimeExecutionIdentity::new(1, 1),
            limits: EngineExecutionLimits {
                passages: 1,
                includes: 0,
            },
            language: Some(&I18nRuntimeLanguage::Chain(chain)),
        },
        |_passage, _state, _requests, _limits| {
            Ok::<BodyExecution, Diagnostic>(BodyExecution::default())
        },
        |_phase, _context, _state| Ok::<(), Diagnostic>(()),
        |_invocation,
         _state,
         _requests,
         _scopes|
         -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, u64>,
            EngineMirMacroCallbackFailure<()>,
        > { panic!("纯文本 Passage 不应分派 Macro") },
    )
    .expect("Host 应提交已翻译的 Start");
    let HostDriveResult::Ready(update) = result else {
        panic!("纯文本 Start 不应留下异步执行");
    };

    assert!(matches!(
        update.surface().nodes(),
        [SurfaceNode::Text(text)]
            if text.to_unicode_string().as_deref() == Some("发现铁剑")
    ));
}

fn host_language_package(
    locale: &str,
    fallback: Option<&str>,
    text: &str,
) -> NlangValidatedPackage {
    let fallback_field: String = fallback
        .map(|value: &str| format!(", \"fallback\": {value:?}"))
        .unwrap_or_default();
    let manifest: String = format!(
        r#"{{
            "locale": {locale:?}{fallback_field},
            "version": "1.0.0",
            "game": {{ "id": "example.forest", "versions": "*" }}
        }}"#
    );
    let translation: I18nTemplate = I18nTemplate::new(
        locale,
        BTreeMap::new(),
        BTreeMap::from([(
            String::from("p5:Start:body.0"),
            I18nTemplateMessage::new(
                "Found {$item}",
                text,
                BTreeMap::from([(String::from("$item"), String::new())]),
            ),
        )]),
    );
    let translations: String = translation.to_nmsg();
    let game: GameIdentity = GameIdentity::new("example.forest", "0.1.0").expect("游戏身份应有效");
    NlangPackageInput::new(vec![
        NlangPackageEntry::new("manifest.json", manifest.into_bytes()),
        NlangPackageEntry::new("translations.nmsg", translations.into_bytes()),
        NlangPackageEntry::new("dictionary.json", br#"{}"#.to_vec()),
    ])
    .validate(locale, &game)
    .expect("Host 测试语言包应有效")
}

#[test]
fn host_takes_only_a_presented_macro_interaction_with_the_same_target() {
    let body: Vec<HirBodyNode<'_>> = Vec::new();
    let id: InteractionId = InteractionId::from_key("link:host:valid");
    let presented: HostUpdate = host_update_with_navigation(id.clone(), "Forest");
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();
    interactions
        .add(
            id.clone(),
            MacroInteraction::new("Forest", &body, CapturedMacroLocals::empty()),
        )
        .expect("动作应可登记");

    let action: MacroInteraction<'_, '_> =
        HostApi::take_macro_interaction(&presented, &id, &mut interactions)
            .expect("相同 ID 和目标应通过验证");

    assert_eq!(action.target(), "Forest");
    assert!(!interactions.has(&id));
}

#[test]
fn host_does_not_consume_macro_interaction_when_surface_does_not_match() {
    let body: Vec<HirBodyNode<'_>> = Vec::new();
    let id: InteractionId = InteractionId::from_key("link:host:mismatch");
    let presented: HostUpdate = host_update_with_navigation(id.clone(), "Forest");
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();
    interactions
        .add(
            id.clone(),
            MacroInteraction::new("Town", &body, CapturedMacroLocals::empty()),
        )
        .expect("动作应可登记");

    let error: Diagnostic = HostApi::take_macro_interaction(&presented, &id, &mut interactions)
        .expect_err("目标不一致时必须拒绝动作");

    assert_eq!(error.code, "host.macro_interaction_target_mismatch");
    assert!(interactions.has(&id));
}

#[test]
fn host_rejects_a_presented_navigation_without_a_macro_action() {
    let id: InteractionId = InteractionId::from_key("link:host:missing");
    let presented: HostUpdate = host_update_with_navigation(id.clone(), "Forest");
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();

    let error: Diagnostic = HostApi::take_macro_interaction(&presented, &id, &mut interactions)
        .expect_err("缺少 Core 动作时不能只信任 Surface 目标");

    assert_eq!(error.code, "host.missing_macro_interaction");
    assert!(interactions.is_empty());
}

#[test]
fn host_executes_macro_interaction_body_before_entering_its_target() {
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
                name: "Forest",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Text("森林"),
                    span: Span {
                        start: 0,
                        end: 1,
                        line: 1,
                        column: 1,
                    },
                }],
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let mut state: State = State::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    story.goto("Start").expect("测试应先位于 Start");
    let body: Vec<HirBodyNode<'_>> = Vec::new();
    let id: InteractionId = InteractionId::from_key("link:host:execute");
    let presented: HostUpdate = host_update_with_navigation(id.clone(), "Forest");
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();
    interactions
        .add(
            id.clone(),
            MacroInteraction::new("Forest", &body, CapturedMacroLocals::empty()),
        )
        .expect("动作应可登记");
    let mut pending: HostPendingExecutions<EngineMirContinuation<'_, '_, u64>> =
        HostPendingExecutions::new();
    let params: Value = Value::Null;

    let result: HostDriveResult = HostApi::advance_macro_interaction_mir(
        &mut pending,
        &mut interactions,
        &mut state,
        &mut story,
        &bytecode,
        HostMirAdvanceRequest {
            presented: &presented,
            input: HostInput::activate(id.clone()),
            params: &params,
            identity: RuntimeExecutionIdentity::new(21, 1),
            limits: EngineExecutionLimits {
                passages: 1,
                includes: 0,
            },
            language: None,
        },
        |_body, state, _requests, _scopes| {
            let _previous: Option<Value> = state.variables_set("clicked", Value::Boolean(true));
            Ok::<BodyExecution, Diagnostic>(BodyExecution::default())
        },
        |_phase, _context, _state| Ok::<(), Diagnostic>(()),
        |_invocation,
         _state,
         _requests,
         _scopes|
         -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, u64>,
            crate::engine::EngineMirMacroCallbackFailure<&'static str>,
        > { panic!("Forest 不应请求 Macro") },
    )
    .expect("Interaction 正文和目标 Passage 应在同一事务中完成");

    let HostDriveResult::Ready(update) = result else {
        panic!("同步正文与目标不应暂停");
    };
    assert_eq!(update.current(), "Forest");
    assert_eq!(state.variables_get("clicked"), Some(&Value::Boolean(true)));
    assert!(!interactions.has(&id));
}

#[test]
fn host_rolls_back_and_restores_macro_interaction_when_its_body_fails() {
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
                name: "Forest",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let mut state: State = State::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    story.goto("Start").expect("测试应先位于 Start");
    let body: Vec<HirBodyNode<'_>> = Vec::new();
    let id: InteractionId = InteractionId::from_key("link:host:rollback");
    let presented: HostUpdate = host_update_with_navigation(id.clone(), "Forest");
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();
    interactions
        .add(
            id.clone(),
            MacroInteraction::new("Forest", &body, CapturedMacroLocals::empty()),
        )
        .expect("动作应可登记");
    let mut pending: HostPendingExecutions<EngineMirContinuation<'_, '_, u64>> =
        HostPendingExecutions::new();
    let params: Value = Value::Null;

    let result = HostApi::advance_macro_interaction_mir(
        &mut pending,
        &mut interactions,
        &mut state,
        &mut story,
        &bytecode,
        HostMirAdvanceRequest {
            presented: &presented,
            input: HostInput::activate(id.clone()),
            params: &params,
            identity: RuntimeExecutionIdentity::new(22, 1),
            limits: EngineExecutionLimits {
                passages: 1,
                includes: 0,
            },
            language: None,
        },
        |_body, state, _requests, _scopes| {
            let _previous: Option<Value> = state.variables_set("leaked", Value::Boolean(true));
            Err::<BodyExecution, Diagnostic>(Diagnostic::new(
                "test.interaction.failed",
                DiagnosticSeverity::Error,
                "测试正文失败",
            ))
        },
        |_phase, _context, _state| Ok::<(), Diagnostic>(()),
        |_invocation,
         _state,
         _requests,
         _scopes|
         -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, u64>,
            crate::engine::EngineMirMacroCallbackFailure<&'static str>,
        > { panic!("失败正文不应进入目标 Passage") },
    );
    let Err(error) = result else {
        panic!("正文失败必须回滚");
    };

    assert_eq!(error.diagnostic.code, "test.interaction.failed");
    assert_eq!(state.variables_get("leaked"), None);
    assert_eq!(story.current().map(|entry| entry.name), Some("Start"));
    assert!(interactions.has(&id));
}

#[test]
fn host_mir_chain_presents_and_activates_an_author_link() {
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
                    kind: HirBodyKind::Macro(HirMacro {
                        name: "link",
                        arguments: HirMacroArguments::Raw("[[进入森林|Forest]]"),
                        syntax_kind: MacroSyntaxKind::Container,
                        body: Vec::new(),
                    }),
                    span: Span {
                        start: 0,
                        end: 1,
                        line: 1,
                        column: 1,
                    },
                }],
            },
            HirPassage {
                source: &source.path,
                name: "Forest",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Text("森林深处"),
                    span: Span {
                        start: 1,
                        end: 2,
                        line: 2,
                        column: 1,
                    },
                }],
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let mut state: State = State::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut pending: HostPendingExecutions<EngineMirContinuation<'_, '_, u64>> =
        HostPendingExecutions::new();
    let params: Value = Value::Null;
    let start_identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(8, 1);

    let started: HostDriveResult = HostApi::start_mir(
        &mut pending,
        &mut state,
        &mut story,
        &bytecode,
        HostMirRequest {
            params: &params,
            identity: start_identity,
            limits: EngineExecutionLimits {
                passages: 1,
                includes: 0,
            },
            language: None,
        },
        |_passage, _state, _requests, _limits| {
            Ok::<BodyExecution, Diagnostic>(BodyExecution::default())
        },
        |_phase, _context, _state| Ok::<(), Diagnostic>(()),
        |invocation: EngineMirMacroInvocation<'_>,
         _state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         scopes: MacroLocalScopes<Value>| {
            let call: HirMacro<'_> = invocation.call.as_hir();
            let identity: RuntimeExecutionIdentity = invocation.identity;
            assert_eq!(call.name, "link");
            let raw: &str = match call.arguments {
                HirMacroArguments::Raw(raw) => raw,
                _ => panic!("link 应保留 Raw 参数"),
            };
            let parsed = parse_argument_list(raw).expect("link 参数应有效");
            let arguments: Vec<Value> = prepare_argument_values(&parsed, |_expression| {
                Err::<Value, &'static str>("静态 link 不应求值 Expression")
            })
            .expect("静态 Interaction Target 应可准备");
            let execution: BodyExecution = link(&arguments, identity).expect("link 应产生导航语义");
            Ok::<_, crate::engine::EngineMirMacroCallbackFailure<&'static str>>(
                MacroResumeOutcome::Complete {
                    output: RuntimeMacroExecution {
                        execution,
                        includes_entered: 0,
                    },
                    scopes,
                },
            )
        },
    )
    .expect("Start 应产生作者链接");
    let HostDriveResult::Ready(started) = started else {
        panic!("同步 link 不应产生异步等待");
    };
    let [SurfaceNode::Navigation { id, label, target, .. }] = started.surface().nodes()
    else {
        panic!("Start 应只显示一个作者导航动作");
    };
    assert_eq!(label.as_units(), TextValue::from("进入森林").as_units());
    assert_eq!(target, "Forest");

    let advanced: HostDriveResult = HostApi::advance_mir(
        &mut pending,
        &mut state,
        &mut story,
        &bytecode,
        HostMirAdvanceRequest {
            presented: &started,
            input: HostInput::activate(id.clone()),
            params: &params,
            identity: RuntimeExecutionIdentity::new(8, 2),
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
        > { panic!("Forest 不应请求 Macro") },
    )
    .expect("玩家激活作者链接后应进入 Forest");
    let HostDriveResult::Ready(advanced) = advanced else {
        panic!("Forest 应直接产生可呈现更新");
    };

    assert_eq!(advanced.current(), "Forest");
    assert!(matches!(
        advanced.surface().nodes(),
        [SurfaceNode::Text(text), SurfaceNode::SafeReturn { target, .. }]
            if text.as_units() == TextValue::from("森林深处").as_units() && target == "Start"
    ));
}

#[test]
fn host_mir_begin_owns_the_initial_async_boundary() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Widgets",
                tags: vec!["widget"],
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Widget(HirWidget {
                        name: "initialize",
                        body: vec![HirBodyNode {
                            kind: HirBodyKind::Set(Box::new(
                                parse("$initialized = true").expect("Widget set 应可解析"),
                            )),
                            span: Span {
                                start: 0,
                                end: 1,
                                line: 1,
                                column: 1,
                            },
                        }],
                    }),
                    span: Span {
                        start: 0,
                        end: 1,
                        line: 1,
                        column: 1,
                    },
                }],
            },
            HirPassage {
                source: &source.path,
                name: "StoryInit",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Macro(HirMacro {
                        name: "initialize",
                        arguments: HirMacroArguments::None,
                        syntax_kind: MacroSyntaxKind::Inline,
                        body: Vec::new(),
                    }),
                    span: Span {
                        start: 1,
                        end: 2,
                        line: 2,
                        column: 1,
                    },
                }],
            },
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Macro(HirMacro {
                        name: "wait",
                        arguments: HirMacroArguments::None,
                        syntax_kind: MacroSyntaxKind::Inline,
                        body: Vec::new(),
                    }),
                    span: Span {
                        start: 0,
                        end: 1,
                        line: 1,
                        column: 1,
                    },
                }],
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let registration: WidgetRegistrationReport =
        register_story_widgets(&mut definitions, &compiled);
    let mut init_locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut state: State = State::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut pending: HostPendingExecutions<EngineMirContinuation<'_, '_, u64>> =
        HostPendingExecutions::new();
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(3, 1);
    let params: Value = Value::Null;

    let result: HostDriveResult = HostApi::start_mir(
        &mut pending,
        &mut state,
        &mut story,
        &bytecode,
        HostMirRequest {
            params: &params,
            identity,
            limits: EngineExecutionLimits {
                passages: 1,
                includes: 0,
            },
            language: None,
        },
        |passage, state, requests, limits| {
            assert_eq!(passage.name, "StoryInit");
            let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
                RuntimeExecutionContext::new(&definitions, state, requests, &mut init_locals);
            runtime
                .execute_passage_with_includes(passage, limits.includes)
                .map_err(|_error| {
                    Diagnostic::new(
                        "test.story_init.failed",
                        DiagnosticSeverity::Error,
                        "StoryInit Widget 执行失败",
                    )
                })
        },
        |_phase, _context, _state| Ok::<(), Diagnostic>(()),
        |invocation: EngineMirMacroInvocation<'_>,
         _state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         mut scopes: MacroLocalScopes<Value>| {
            let call_identity: RuntimeExecutionIdentity = invocation.identity;
            scopes.enter_call(Vec::new());
            Ok::<_, crate::engine::EngineMirMacroCallbackFailure<&'static str>>(
                MacroResumeOutcome::Pending(MacroSuspension {
                    identity: call_identity,
                    handle: 41_u64,
                    scopes: scopes.suspend().expect("活动调用帧应能暂停"),
                }),
            )
        },
    )
    .expect("Host 应保存首次异步边界");

    assert!(matches!(
        result,
        HostDriveResult::Pending { execution } if execution.identity() == identity
    ));
    assert_eq!(registration.registered, 1);
    assert_eq!(registration.replaced, 0);
    assert_eq!(
        state.variables_get("initialized"),
        Some(&Value::Boolean(true))
    );
    assert!(pending.has(HostExecutionToken::from_identity(identity)));

    let repeated = HostApi::start_mir(
        &mut pending,
        &mut state,
        &mut story,
        &bytecode,
        HostMirRequest {
            params: &params,
            identity: RuntimeExecutionIdentity::new(3, 2),
            limits: EngineExecutionLimits {
                passages: 1,
                includes: 0,
            },
            language: None,
        },
        |_passage, _state, _requests, _limits| panic!("重复启动不得再次执行 StoryInit"),
        |_phase, _context, _state| Ok::<(), Diagnostic>(()),
        |_invocation,
         _state,
         _requests,
         _scopes|
         -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, u64>,
            crate::engine::EngineMirMacroCallbackFailure<&'static str>,
        > { panic!("重复启动必须在进入 VM 前被拒绝") },
    )
    .err()
    .expect("活动 Story 不得再次启动");

    assert_eq!(repeated.diagnostic.code, "engine.start.already_started");
    assert!(pending.has(HostExecutionToken::from_identity(identity)));

    HostApi::cancel_pending(
        &mut pending,
        &mut state,
        &mut story,
        HostExecutionToken::from_identity(identity),
    )
    .expect("取消首次异步执行应恢复启动前检查点");
    assert_eq!(state.variables_get("initialized"), None);
    assert!(story.current().is_none());
}

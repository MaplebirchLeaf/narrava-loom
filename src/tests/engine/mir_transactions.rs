// MIR 执行、挂起恢复与事务一致性。
// 本文件被上级 `engine` 测试模块直接包含，以共享统一的测试夹具。
#[test]
fn mir_vm_commits_state_goto_history_and_output_through_engine() {
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
                        kind: HirBodyKind::Set(Box::new(parse("$score = 4").expect("set 应有效"))),
                        span: TweeSpan {
                            start: 0,
                            end: 1,
                            line: 1,
                            column: 1,
                        },
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Text("A"),
                        span: TweeSpan {
                            start: 1,
                            end: 2,
                            line: 1,
                            column: 2,
                        },
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Goto(Box::new(parse("\"Next\"").expect("goto 应有效"))),
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
                name: "Next",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Text("B"),
                    span: TweeSpan {
                        start: 0,
                        end: 1,
                        line: 1,
                        column: 1,
                    },
                }],
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("测试 Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let navigation: EngineNavigationChain<'_, '_> = Engine::navigate_mir_chain(
        &mut state,
        &mut story,
        &bytecode,
        "Start",
        EngineExecutionLimits {
            passages: 2,
            includes: 0,
        },
    )
    .expect("MIR 导航链应通过 Engine 提交");

    let text: String = navigation
        .output
        .nodes()
        .iter()
        .filter_map(|node| match node {
            SurfaceNode::Text(text) => text.to_unicode_string(),
            _ => None,
        })
        .collect();
    assert_eq!(text, "AB");
    assert_eq!(navigation.entries.len(), 2);
    assert_eq!(state.variables_get("score"), Some(&Value::Number(4.0)));
    assert_eq!(story.current().map(|passage| passage.name), Some("Next"));
}

#[test]
fn mir_vm_error_rolls_back_state_and_story_through_engine() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Broken",
            tags: Vec::new(),
            body: vec![
                HirBodyNode {
                    kind: HirBodyKind::Set(Box::new(parse("$score = 4").expect("set 应有效"))),
                    span: TweeSpan {
                        start: 0,
                        end: 1,
                        line: 1,
                        column: 1,
                    },
                },
                HirBodyNode {
                    kind: HirBodyKind::Print(crate::hir::HirPrint::Expression(
                        parse("missing").expect("未知 global 语法仍有效"),
                    )),
                    span: TweeSpan {
                        start: 1,
                        end: 2,
                        line: 1,
                        column: 2,
                    },
                },
            ],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("测试 Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    let _previous: Option<Value> = state.variables_set("score", Value::Number(1.0));

    let result = Engine::navigate_mir_chain(
        &mut state,
        &mut story,
        &bytecode,
        "Broken",
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
    );

    assert!(matches!(
        result,
        Err(EngineNavigationError::Execution(
            EngineRequestedExecutionError::Runtime(EngineMirExecutionError::Vm(
                MirExecutionError::Evaluation(_)
            ))
        ))
    ));
    assert_eq!(state.variables_get("score"), Some(&Value::Number(1.0)));
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

#[test]
fn mir_include_limit_failure_rolls_back_the_engine_transaction() {
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
                        kind: HirBodyKind::Set(Box::new(
                            parse("$changed = true").expect("set 应有效"),
                        )),
                        span: TweeSpan {
                            start: 0,
                            end: 1,
                            line: 1,
                            column: 1,
                        },
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Include(Box::new(
                            parse("\"Details\"").expect("include 应有效"),
                        )),
                        span: TweeSpan {
                            start: 1,
                            end: 2,
                            line: 1,
                            column: 2,
                        },
                    },
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Details",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("测试 Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let result = Engine::navigate_mir_chain(
        &mut state,
        &mut story,
        &bytecode,
        "Start",
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
    );

    assert!(matches!(
        result,
        Err(EngineNavigationError::Execution(
            EngineRequestedExecutionError::Runtime(EngineMirExecutionError::IncludeLimitExceeded {
                limit: 0
            })
        ))
    ));
    assert!(state.variables_get("changed").is_none());
    assert_eq!(story.current(), None);
}

#[test]
fn unresolved_mir_macro_rolls_back_the_engine_transaction() {
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
                    kind: HirBodyKind::Set(Box::new(parse("$changed = true").expect("set 应有效"))),
                    span: TweeSpan {
                        start: 0,
                        end: 1,
                        line: 1,
                        column: 1,
                    },
                },
                HirBodyNode {
                    kind: HirBodyKind::Macro(HirMacro {
                        name: "notice",
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
            ],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("Macro Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let result = Engine::navigate_mir_chain(
        &mut state,
        &mut story,
        &bytecode,
        "Start",
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
    );

    assert!(matches!(
        result,
        Err(EngineNavigationError::Execution(
            EngineRequestedExecutionError::Runtime(EngineMirExecutionError::MacroPending)
        ))
    ));
    assert!(state.variables_get("changed").is_none());
    assert_eq!(story.current(), None);
}

#[test]
fn engine_resumes_vm_after_the_shared_macro_runtime_completes() {
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
                        kind: HirBodyKind::Macro(HirMacro {
                            name: "notice",
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
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Text("完成"),
                        span: TweeSpan {
                            start: 1,
                            end: 2,
                            line: 1,
                            column: 2,
                        },
                    },
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Included",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Text("包含"),
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
    let widget: HirWidget<'_> = HirWidget {
        name: "notice",
        body: vec![
            HirBodyNode {
                kind: HirBodyKind::Text("宏前"),
                span: TweeSpan {
                    start: 0,
                    end: 1,
                    line: 1,
                    column: 1,
                },
            },
            HirBodyNode {
                kind: HirBodyKind::Include(Box::new(
                    parse(r#""Included""#).expect("include 目标应有效"),
                )),
                span: TweeSpan {
                    start: 1,
                    end: 2,
                    line: 1,
                    column: 2,
                },
            },
            HirBodyNode {
                kind: HirBodyKind::Text("宏后"),
                span: TweeSpan {
                    start: 2,
                    end: 3,
                    line: 1,
                    column: 3,
                },
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("Macro Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let _previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        register_widget(&mut definitions, &widget);
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let result: EngineNavigationChain<'_, '_> = Engine::navigate_mir_chain_with_macros(
        &mut state,
        &mut story,
        &bytecode,
        "Start",
        EngineExecutionLimits {
            passages: 1,
            includes: 1,
        },
        |call, state, requests, remaining_includes| {
            let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
                RuntimeExecutionContext::new(&definitions, state, requests, &mut locals);
            runtime.execute_macro_with_includes(call, remaining_includes)
        },
    )
    .expect("Engine 应在同步 Widget 完成后继续 VM");

    let text: String = result
        .output
        .nodes()
        .iter()
        .filter_map(|node| match node {
            SurfaceNode::Text(text) => text.to_unicode_string(),
            _ => None,
        })
        .collect();
    assert_eq!(text, "宏前包含宏后完成");
    assert_eq!(story.current().map(|entry| entry.name), Some("Start"));
    assert_eq!(locals.args(), None);

    let mut limited_story: Story<'_, '_> = Story::new(&compiled);
    let mut limited_state: State = State::new();
    let limited = Engine::navigate_mir_chain_with_macros(
        &mut limited_state,
        &mut limited_story,
        &bytecode,
        "Start",
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |call, state, requests, remaining_includes| {
            let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
                RuntimeExecutionContext::new(&definitions, state, requests, &mut locals);
            runtime.execute_macro_with_includes(call, remaining_includes)
        },
    );

    assert!(matches!(
        limited,
        Err(EngineNavigationError::Execution(
            EngineRequestedExecutionError::Runtime(EngineMirMacroExecutionError::Macro(
                RuntimeExecutionError::IncludeLimitExceeded { limit: 0 }
            ))
        ))
    ));
    assert_eq!(limited_story.current(), None);
    assert_eq!(locals.args(), None);
}

#[test]
fn engine_rolls_back_when_a_macro_leaks_an_internal_control_signal() {
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
                    kind: HirBodyKind::Set(Box::new(parse("$changed = true").expect("set 应有效"))),
                    span: TweeSpan {
                        start: 0,
                        end: 1,
                        line: 1,
                        column: 1,
                    },
                },
                HirBodyNode {
                    kind: HirBodyKind::Macro(HirMacro {
                        name: "invalidControl",
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
            ],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&compiled).expect("Macro Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let result = Engine::navigate_mir_chain_with_macros(
        &mut state,
        &mut story,
        &bytecode,
        "Start",
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |_call, _state, _requests, _remaining_includes| {
            Ok::<RuntimeMacroExecution, &'static str>(RuntimeMacroExecution {
                execution: execution(BodyControl::BreakLoop),
                includes_entered: 0,
            })
        },
    );

    assert!(matches!(
        result,
        Err(EngineNavigationError::Execution(
            EngineRequestedExecutionError::Runtime(
                EngineMirMacroExecutionError::UnexpectedMacroControl(BodyControl::BreakLoop)
            )
        ))
    ));
    assert!(state.variables_get("changed").is_none());
    assert_eq!(story.current(), None);
}

#[test]
fn engine_mir_transaction_uses_the_selected_translation_until_commit() {
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
                    span: TweeSpan {
                        start: 0,
                        end: 1,
                        line: 1,
                        column: 1,
                    },
                },
                HirBodyNode {
                    kind: HirBodyKind::Print(crate::hir::HirPrint::Expression(
                        parse("$item").expect("物品表达式应有效"),
                    )),
                    span: TweeSpan {
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
    let template: I18nTemplate = I18nTemplate::new(
        "zh-CN",
        BTreeMap::from([(
            String::from("items"),
            BTreeMap::from([(String::from("Iron Sword"), String::from("铁剑"))]),
        )]),
        BTreeMap::from([(
            String::from("p5:Start:body.0"),
            I18nTemplateMessage::new(
                "Found {$item}",
                "发现{$item}",
                BTreeMap::from([(String::from("$item"), String::from("items"))]),
            ),
        )]),
    );
    let translation: I18nValidatedTemplate = mir
        .i18n()
        .validate(template)
        .expect("目标语言应通过当前 Story 校验");
    let mut state: State = State::new();
    let _previous: Option<Value> =
        state.variables_set("item", Value::String(TextValue::from("Iron Sword")));
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let params: Value = Value::Undefined;

    let resumed = Engine::begin_mir_chain(
        &mut state,
        &mut story,
        &bytecode,
        EngineMirBeginRequest {
            name: "Start",
            params: &params,
            identity: RuntimeExecutionIdentity::new(1, 1),
            limits: EngineExecutionLimits {
                passages: 1,
                includes: 0,
            },
            language: Some(&I18nRuntimeLanguage::Translation(translation)),
        },
        |_phase, _context, _state| Ok::<(), Diagnostic>(()),
    )
    .unwrap_or_else(|_| panic!("Engine 应执行到 Halted"));
    let committed = resumed
        .commit_halted(&mut state, &mut story, |_phase, _context, _state| {
            Ok::<(), Diagnostic>(())
        })
        .unwrap_or_else(|_| panic!("Halted 事务应提交"));

    assert!(matches!(
        committed.navigation.output.nodes(),
        [SurfaceNode::Text(text)]
            if text.to_unicode_string().as_deref() == Some("发现铁剑")
    ));
}

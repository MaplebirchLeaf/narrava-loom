// include 的位置、递归、跳转与退出语义。
// 本文件被上级 `engine` 测试模块直接包含，以共享统一的测试夹具。
#[test]
fn runtime_executes_include_at_its_source_position_without_navigation() {
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
                name: "Start",
                tags: Vec::new(),
                body: vec![
                    HirBodyNode {
                        kind: HirBodyKind::Set(Box::new(
                            parse(r#"$order = "start""#).expect("首次 set 应可解析"),
                        )),
                        span,
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Include(Box::new(
                            parse(r#""Included""#).expect("include 目标应可解析"),
                        )),
                        span,
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Set(Box::new(
                            parse(r#"$order += "-end""#).expect("末尾 set 应可解析"),
                        )),
                        span,
                    },
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Included",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Set(Box::new(
                        parse(r#"$order += "-included""#).expect("include set 应可解析"),
                    )),
                    span,
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
            runtime.execute_passage_with_includes(passage, 4)
        },
    )
    .expect("include 应在 Start 的两个 set 之间执行");

    assert_eq!(navigation.entered.passage().name, "Start");
    assert_eq!(navigation.requested, None);
    assert_eq!(
        state.variables_get("order"),
        Some(&Value::string("start-included-end"))
    );
    assert_eq!(story.current().map(|passage| passage.name), Some("Start"));
    assert_eq!(story.history().len(), 1);
}

#[test]
fn runtime_include_limit_rolls_back_recursive_includes() {
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
                name: "Start",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Include(Box::new(
                        parse(r#""Loop""#).expect("首次 include 目标应可解析"),
                    )),
                    span,
                }],
            },
            HirPassage {
                source: &source.path,
                name: "Loop",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Include(Box::new(
                        parse(r#""Loop""#).expect("循环 include 目标应可解析"),
                    )),
                    span,
                }],
            },
        ],
    };
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    let _score: Option<Value> = state.variables_set("score", Value::Number(1.0));

    let result: Result<
        EngineRequestedNavigation<'_, '_>,
        EngineNavigationError<
            EngineRequestedExecutionError<RuntimeExecutionError<StoryRuntimeRequestError>>,
        >,
    > = Engine::navigate_with_requests(
        &mut state,
        &mut story,
        "Start",
        |passage: &HirPassage<'_>,
         state: &mut State,
         requests: &mut StoryRuntimeRequests<'_, '_, '_>| {
            let _score: Option<Value> = state.variables_set("score", Value::Number(9.0));
            let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
                RuntimeExecutionContext::new(&definitions, state, requests, &mut locals);
            runtime.execute_passage_with_includes(passage, 2)
        },
    );

    assert_eq!(
        result,
        Err(EngineNavigationError::Execution(
            EngineRequestedExecutionError::Runtime(RuntimeExecutionError::IncludeLimitExceeded {
                limit: 2
            })
        ))
    );
    assert_eq!(state.variables_get("score"), Some(&Value::Number(1.0)));
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

#[test]
fn goto_inside_include_stops_both_bodies_and_enters_the_requested_passage() {
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
                name: "Start",
                tags: Vec::new(),
                body: vec![
                    HirBodyNode {
                        kind: HirBodyKind::Include(Box::new(
                            parse(r#""Middle""#).expect("include 目标应可解析"),
                        )),
                        span,
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Set(Box::new(
                            parse("$outerAfter = true").expect("外层 set 应可解析"),
                        )),
                        span,
                    },
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Middle",
                tags: Vec::new(),
                body: vec![
                    HirBodyNode {
                        kind: HirBodyKind::Goto(Box::new(
                            parse(r#""End""#).expect("goto 目标应可解析"),
                        )),
                        span,
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Set(Box::new(
                            parse("$includeAfter = true").expect("include 后 set 应可解析"),
                        )),
                        span,
                    },
                ],
            },
            HirPassage {
                source: &source.path,
                name: "End",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Set(Box::new(
                        parse("$enteredEnd = true").expect("End set 应可解析"),
                    )),
                    span,
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
            let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
                RuntimeExecutionContext::new(&definitions, state, requests, &mut locals);
            runtime.execute_passage_with_includes(passage, limits.includes)
        },
    )
    .expect("include 内 goto 应交给 Engine 确认并继续执行 End");

    let names: Vec<&str> = navigation
        .entries
        .iter()
        .map(|entry: &StoryHistoryEntry<'_, '_>| entry.passage().name)
        .collect();
    assert_eq!(names, vec!["Start", "End"]);
    assert_eq!(state.variables_get("outerAfter"), None);
    assert_eq!(state.variables_get("includeAfter"), None);
    assert_eq!(
        state.variables_get("enteredEnd"),
        Some(&Value::Boolean(true))
    );
}

#[test]
fn exit_inside_include_returns_to_the_outer_passage() {
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
                name: "Start",
                tags: Vec::new(),
                body: vec![
                    HirBodyNode {
                        kind: HirBodyKind::Include(Box::new(
                            parse(r#""Included""#).expect("include 目标应可解析"),
                        )),
                        span,
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Set(Box::new(
                            parse("$outerAfter = true").expect("外层 set 应可解析"),
                        )),
                        span,
                    },
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Included",
                tags: Vec::new(),
                body: vec![
                    HirBodyNode {
                        kind: HirBodyKind::Exit,
                        span,
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Set(Box::new(
                            parse("$includeAfter = true").expect("include 后 set 应可解析"),
                        )),
                        span,
                    },
                ],
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
            runtime.execute_passage_with_includes(passage, 4)
        },
    )
    .expect("include 内 exit 应返回外层正文");

    assert_eq!(navigation.entered.passage().name, "Start");
    assert_eq!(navigation.requested, None);
    assert_eq!(state.variables_get("includeAfter"), None);
    assert_eq!(
        state.variables_get("outerAfter"),
        Some(&Value::Boolean(true))
    );
    assert_eq!(story.history().len(), 1);
}

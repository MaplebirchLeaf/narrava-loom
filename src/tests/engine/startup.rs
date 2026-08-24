// 启动、StoryInit、重启与失败回滚。
// 本文件被上级 `engine` 测试模块直接包含，以共享统一的测试夹具。
#[test]
fn engine_start_returns_the_initial_chain_and_rejects_repeated_startup() {
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
    let limits: EngineExecutionLimits = EngineExecutionLimits {
        passages: 1,
        includes: 0,
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    let mut executions: usize = 0;

    let started: EngineStart<'_, '_> = Engine::start(
        &mut state,
        &mut story,
        "Start",
        limits,
        |_passage: &HirPassage<'_>,
         _state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            executions += 1;
            Ok::<BodyExecution, &'static str>(execution(BodyControl::Continue))
        },
    )
    .expect("未启动的 Story 应进入 Start");

    assert_eq!(started.initial.passage().name, "Start");
    assert_eq!(started.current.passage().name, "Start");
    assert_eq!(started.entries.len(), 1);
    assert_eq!(executions, 1);

    let repeated: Result<EngineStart<'_, '_>, EngineStartError<&'static str>> = Engine::start(
        &mut state,
        &mut story,
        "Start",
        limits,
        |_passage: &HirPassage<'_>,
         _state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            panic!("重复启动不应进入 Runtime");
        },
    );

    assert_eq!(
        repeated,
        Err(EngineStartError::AlreadyStarted {
            current: String::from("Start")
        })
    );
    assert_eq!(executions, 1);
    assert_eq!(story.history().len(), 1);
}

#[test]
fn engine_start_with_lifecycle_renders_and_displays_the_initial_passage() {
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
    let params: Value = Value::String(String::from("Launcher").into());
    let phases: RefCell<Vec<PassageLifecyclePhase>> = RefCell::new(Vec::new());
    let limits: EngineExecutionLimits = EngineExecutionLimits {
        passages: 1,
        includes: 0,
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let started: EngineStart<'_, '_> = Engine::start_with_lifecycle(
        &mut state,
        &mut story,
        "Start",
        &params,
        limits,
        |phase: PassageLifecyclePhase,
         context: PassageLifecycleContext<'_, '_, '_, '_>,
         state: &mut State| {
            phases.borrow_mut().push(phase);
            if phase == PassageLifecyclePhase::Init {
                assert_eq!(context.params(), &params);
                let _origin: Option<Value> =
                    state.temporary_set("origin", context.params().clone());
            }
            Ok::<(), &'static str>(())
        },
        |_passage: &HirPassage<'_>,
         state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            assert_eq!(state.temporary_get("origin"), Some(&params));
            Ok::<BodyExecution, &'static str>(execution(BodyControl::Continue))
        },
    )
    .expect("首次启动应进入 Passage 生命周期");

    assert_eq!(started.current.passage().name, "Start");
    assert_eq!(
        phases.into_inner(),
        vec![
            PassageLifecyclePhase::Init,
            PassageLifecyclePhase::Start,
            PassageLifecyclePhase::Render,
            PassageLifecyclePhase::Display,
        ]
    );
}

#[test]
fn exit_tag_executes_the_passage_without_render_or_display() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Transition",
            tags: vec!["exit"],
            body: Vec::new(),
        }],
    };
    let phases: RefCell<Vec<PassageLifecyclePhase>> = RefCell::new(Vec::new());
    let mut executed: bool = false;
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let _started: EngineStart<'_, '_> = Engine::start_with_lifecycle(
        &mut state,
        &mut story,
        "Transition",
        &Value::Undefined,
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |phase: PassageLifecyclePhase,
         _context: PassageLifecycleContext<'_, '_, '_, '_>,
         _state: &mut State| {
            phases.borrow_mut().push(phase);
            Ok::<(), &'static str>(())
        },
        |_passage: &HirPassage<'_>,
         state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            executed = true;
            let _transitioned: Option<Value> =
                state.variables_set("transitioned", Value::Boolean(true));
            Ok::<BodyExecution, &'static str>(execution(BodyControl::Continue))
        },
    )
    .expect("exit Passage 应完成逻辑执行");

    assert!(executed);
    assert_eq!(
        state.variables_get("transitioned"),
        Some(&Value::Boolean(true))
    );
    assert_eq!(
        phases.into_inner(),
        vec![PassageLifecyclePhase::Init, PassageLifecyclePhase::Start]
    );
}

#[test]
fn engine_start_executes_story_init_before_the_initial_passage() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "StoryInit",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let executions: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let lifecycle_phases: RefCell<Vec<PassageLifecyclePhase>> = RefCell::new(Vec::new());
    let params: Value = Value::Undefined;
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let started: EngineStart<'_, '_> = Engine::start_with_lifecycle(
        &mut state,
        &mut story,
        "Start",
        &params,
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |phase: PassageLifecyclePhase,
         _context: PassageLifecycleContext<'_, '_, '_, '_>,
         _state: &mut State| {
            lifecycle_phases.borrow_mut().push(phase);
            Ok::<(), &'static str>(())
        },
        |passage: &HirPassage<'_>,
         state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            executions.borrow_mut().push(passage.name.to_owned());
            if passage.name == "StoryInit" {
                let _initialized: Option<Value> =
                    state.variables_set("initialized", Value::Boolean(true));
            } else {
                assert_eq!(
                    state.variables_get("initialized"),
                    Some(&Value::Boolean(true))
                );
            }
            Ok::<BodyExecution, &'static str>(execution(BodyControl::Continue))
        },
    )
    .expect("StoryInit 应先于起始 Passage 执行");

    assert_eq!(started.current.passage().name, "Start");
    assert_eq!(
        executions.into_inner(),
        vec![String::from("StoryInit"), String::from("Start")]
    );
    assert_eq!(
        lifecycle_phases.into_inner(),
        vec![
            PassageLifecyclePhase::Init,
            PassageLifecyclePhase::Start,
            PassageLifecyclePhase::Render,
            PassageLifecyclePhase::Display,
        ]
    );
    assert_eq!(story.history().len(), 1);
}

#[test]
fn real_runtime_story_init_sets_state_before_the_initial_passage() {
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
                name: "StoryInit",
                tags: Vec::new(),
                body: vec![
                    HirBodyNode {
                        kind: HirBodyKind::Set(Box::new(
                            parse("$health = 100").expect("variables 初始化表达式应可解析"),
                        )),
                        span,
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Set(Box::new(
                            parse(r#"setup.difficulty = "normal""#)
                                .expect("setup 初始化表达式应可解析"),
                        )),
                        span,
                    },
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    // 起始 Passage 必须从 State 读取 StoryInit 已提交的数据。
                    kind: HirBodyKind::Set(Box::new(
                        parse(r#"$observed = $health == 100 && setup.difficulty == "normal""#)
                            .expect("起始 Passage 读取表达式应可解析"),
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

    let started: EngineStart<'_, '_> = Engine::start(
        &mut state,
        &mut story,
        "Start",
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
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
    .expect("真实 Runtime 应依次执行 StoryInit 与起始 Passage");

    assert_eq!(started.current.passage().name, "Start");
    assert_eq!(state.variables_get("health"), Some(&Value::Number(100.0)));
    assert_eq!(state.variables_get("observed"), Some(&Value::Boolean(true)));
    assert_eq!(story.history().len(), 1);
    assert_eq!(story.history()[0].passage().name, "Start");
}

#[test]
fn real_runtime_story_init_include_preserves_order_without_history() {
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
                name: "StoryInit",
                tags: Vec::new(),
                body: vec![
                    HirBodyNode {
                        kind: HirBodyKind::Set(Box::new(
                            parse(r#"$order = "init""#).expect("初始化 set 应可解析"),
                        )),
                        span,
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Include(Box::new(
                            parse(r#""InitShared""#).expect("初始化 include 应可解析"),
                        )),
                        span,
                    },
                    HirBodyNode {
                        kind: HirBodyKind::Set(Box::new(
                            parse(r#"$order += "-after""#).expect("include 后 set 应可解析"),
                        )),
                        span,
                    },
                ],
            },
            HirPassage {
                source: &source.path,
                name: "InitShared",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Set(Box::new(
                        parse(r#"$order += "-included""#).expect("include 正文应可解析"),
                    )),
                    span,
                }],
            },
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Set(Box::new(
                        parse(r#"$order += "-start""#).expect("起始 Passage set 应可解析"),
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

    let started: EngineStart<'_, '_> = Engine::start(
        &mut state,
        &mut story,
        "Start",
        EngineExecutionLimits {
            passages: 1,
            includes: 1,
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
    .expect("StoryInit include 应在原位置执行后继续启动");

    assert_eq!(started.current.passage().name, "Start");
    assert_eq!(
        state.variables_get("order"),
        Some(&Value::string("init-included-after-start"))
    );
    assert_eq!(story.history().len(), 1);
    assert_eq!(story.history()[0].passage().name, "Start");
}

#[test]
fn host_registers_widgets_before_story_init_uses_them() {
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
                body: vec![HirBodyNode {
                    kind: HirBodyKind::Widget(HirWidget {
                        name: "initialize",
                        body: vec![HirBodyNode {
                            kind: HirBodyKind::Set(Box::new(
                                parse("$initializedByWidget = true").expect("Widget set 应可解析"),
                            )),
                            span,
                        }],
                    }),
                    span,
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
                        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                        body: Vec::new(),
                    }),
                    span,
                }],
            },
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut definitions: MacroDefinitions<
        MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>,
    > = MacroDefinitions::new();
    let report: WidgetRegistrationReport = register_story_widgets(&mut definitions, &compiled);
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let started: EngineStart<'_, '_> = Engine::start(
        &mut state,
        &mut story,
        "Start",
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
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
    .expect("StoryInit 应可调用宿主预先注册的 Widget");

    assert_eq!(report.registered, 1);
    assert_eq!(report.replaced, 0);
    assert_eq!(started.current.passage().name, "Start");
    assert_eq!(
        state.variables_get("initializedByWidget"),
        Some(&Value::Boolean(true))
    );
    assert_eq!(story.history().len(), 1);
}

#[test]
fn engine_new_game_ends_the_old_passage_resets_domains_and_starts_again() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Old",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let params: Value = Value::String(String::from("NewGameMenu").into());
    let phases: RefCell<Vec<String>> = RefCell::new(Vec::new());
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let old_id = story.goto("Old").expect("旧 Passage 应可导航").id();
    let mut state: State = State::new();
    let _global: Option<Value> = state.global_set("helper", Value::Boolean(true));
    let _variable: Option<Value> = state.variables_set("score", Value::Number(8.0));
    let _temporary: Option<Value> = state.temporary_set("panel", Value::Boolean(true));

    let new_game: EngineNewGame<'_, '_> = Engine::new_game_with_lifecycle(
        &mut state,
        &mut story,
        "Start",
        &params,
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |phase: PassageLifecyclePhase,
         context: PassageLifecycleContext<'_, '_, '_, '_>,
         state: &mut State| {
            phases
                .borrow_mut()
                .push(format!("{phase:?}:{}", context.entry().passage().name));
            if phase == PassageLifecyclePhase::Init {
                assert_eq!(context.params(), &params);
                assert_eq!(state.variables_get("score"), None);
                assert_eq!(state.temporary_get("panel"), None);
                assert_eq!(state.global_get("helper"), Some(&Value::Boolean(true)));
            }
            Ok::<(), &'static str>(())
        },
        |_passage: &HirPassage<'_>,
         _state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            Ok::<BodyExecution, &'static str>(execution(BodyControl::Continue))
        },
    )
    .expect("新游戏应重置两个领域并重新启动");

    assert_eq!(new_game.state.variables_removed, 1);
    assert_eq!(new_game.state.temporary_removed, 1);
    assert_eq!(new_game.history_removed, 1);
    assert_eq!(new_game.start.current.passage().name, "Start");
    assert_ne!(new_game.start.current.id(), old_id);
    assert_eq!(
        phases.into_inner(),
        vec![
            String::from("End:Old"),
            String::from("Init:Start"),
            String::from("Start:Start"),
            String::from("Render:Start"),
            String::from("Display:Start"),
        ]
    );
}

#[test]
fn engine_executes_story_init_without_navigation_or_passage_lifecycle() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "StoryInit",
            tags: Vec::new(),
            body: Vec::new(),
        }],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let initialized: EngineStoryInit = Engine::execute_story_init(
        &mut state,
        &mut story,
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |passage: &HirPassage<'_>,
         state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            assert_eq!(passage.name, "StoryInit");
            let _health: Option<Value> = state.variables_set("health", Value::Number(100.0));
            let _setup: Value = state.setup_set(Value::String(String::from("normal").into()));
            Ok::<BodyExecution, &'static str>(execution(BodyControl::Continue))
        },
    )
    .expect("StoryInit 逻辑应成功执行");

    assert_eq!(initialized, EngineStoryInit::Executed);
    assert_eq!(state.variables_get("health"), Some(&Value::Number(100.0)));
    assert_eq!(
        state.setup_get(),
        &Value::String(String::from("normal").into())
    );
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

#[test]
fn engine_rejects_story_init_goto_and_restores_state() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "StoryInit",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    let _score: Option<Value> = state.variables_set("score", Value::Number(1.0));

    let result: Result<
        EngineStoryInit,
        EngineNavigationError<EngineRequestedExecutionError<&str>>,
    > = Engine::execute_story_init(
        &mut state,
        &mut story,
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |_passage: &HirPassage<'_>,
         state: &mut State,
         requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            let _score: Option<Value> = state.variables_set("score", Value::Number(9.0));
            requests.goto("Start").expect("普通目标应可建立请求");
            Ok(execution(BodyControl::StopPassage))
        },
    );

    assert_eq!(
        result,
        Err(EngineNavigationError::Execution(
            EngineRequestedExecutionError::StoryInitGotoUnsupported
        ))
    );
    assert_eq!(state.variables_get("score"), Some(&Value::Number(1.0)));
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

#[test]
fn engine_start_rolls_back_story_init_when_the_initial_passage_fails() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let compiled: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "StoryInit",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    let _score: Option<Value> = state.variables_set("score", Value::Number(1.0));

    let result: Result<EngineStart<'_, '_>, EngineStartError<&str>> = Engine::start(
        &mut state,
        &mut story,
        "Start",
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |passage: &HirPassage<'_>,
         state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            if passage.name == "StoryInit" {
                let _score: Option<Value> = state.variables_set("score", Value::Number(9.0));
                let _initialized: Option<Value> =
                    state.variables_set("initialized", Value::Boolean(true));
                Ok(execution(BodyControl::Continue))
            } else {
                Err("initial passage failed")
            }
        },
    );

    assert_eq!(
        result,
        Err(EngineStartError::Execution(
            EngineNavigationError::Execution(EngineRequestedExecutionError::Runtime(
                "initial passage failed"
            ))
        ))
    );
    assert_eq!(state.variables_get("score"), Some(&Value::Number(1.0)));
    assert_eq!(state.variables_get("initialized"), None);
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

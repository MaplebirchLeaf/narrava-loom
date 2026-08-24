// 导航提交、回滚、链式跳转与运行时错误。
// 本文件被上级 `engine` 测试模块直接包含，以共享统一的测试夹具。
#[test]
fn navigation_commits_story_state_and_clears_previous_temporary_values() {
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
    let _old_temporary: Option<Value> = state.temporary_set("old", Value::Boolean(true));

    let navigation: EngineNavigation<'_, '_, &'static str> = Engine::navigate(
        &mut state,
        &mut story,
        "Start",
        |passage: &HirPassage<'_>, state: &mut State| {
            assert_eq!(passage.name, "Start");
            assert!(!state.temporary_has("old"));
            let _score: Option<Value> = state.variables_set("score", Value::Number(1.0));
            let _turn: Option<Value> = state.temporary_set("turn", Value::Number(1.0));
            Ok::<&'static str, &'static str>("rendered")
        },
    )
    .expect("成功执行应提交导航事务");

    assert_eq!(navigation.output, "rendered");
    assert_eq!(navigation.entry.passage().name, "Start");
    assert_eq!(
        story.current_entry().map(StoryHistoryEntry::id),
        Some(navigation.entry.id())
    );
    assert_eq!(state.variables_get("score"), Some(&Value::Number(1.0)));
    assert_eq!(state.temporary_get("turn"), Some(&Value::Number(1.0)));
}

#[test]
fn failed_navigation_restores_story_and_every_state_namespace() {
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
                name: "Broken",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let _start: &StoryHistoryEntry<'_, '_> = story.goto("Start").expect("Start 应可导航");
    let mut state: State = State::new();
    let _global: Option<Value> = state.global_set("mode", Value::string("before"));
    let _setup: Value = state.setup_set(Value::string("before"));
    let _variable: Option<Value> = state.variables_set("score", Value::Number(1.0));
    let _temporary: Option<Value> = state.temporary_set("turn", Value::Number(1.0));

    let result: Result<EngineNavigation<'_, '_, ()>, EngineNavigationError<&'static str>> =
        Engine::navigate(
            &mut state,
            &mut story,
            "Broken",
            |_passage: &HirPassage<'_>, state: &mut State| {
                let _global: Option<Value> = state.global_set("mode", Value::string("changed"));
                let _setup: Value = state.setup_set(Value::string("changed"));
                let _variable: Option<Value> = state.variables_set("score", Value::Number(9.0));
                let _temporary: Option<Value> = state.temporary_set("turn", Value::Number(9.0));
                Err("broken passage")
            },
        );

    assert_eq!(
        result,
        Err(EngineNavigationError::Execution("broken passage"))
    );
    assert_eq!(story.current().map(|passage| passage.name), Some("Start"));
    assert_eq!(story.history().len(), 1);
    assert_eq!(state.global_get("mode"), Some(&Value::string("before")));
    assert_eq!(state.setup_get(), &Value::string("before"));
    assert_eq!(state.variables_get("score"), Some(&Value::Number(1.0)));
    assert_eq!(state.temporary_get("turn"), Some(&Value::Number(1.0)));
}

#[test]
fn missing_navigation_target_does_not_clear_state_or_call_the_executor() {
    let compiled: HirStory<'_> = HirStory {
        passages: Vec::new(),
    };
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    let _temporary: Option<Value> = state.temporary_set("turn", Value::Number(3.0));
    let mut called: bool = false;

    let result: Result<EngineNavigation<'_, '_, ()>, EngineNavigationError<&'static str>> =
        Engine::navigate(
            &mut state,
            &mut story,
            "Missing",
            |_passage: &HirPassage<'_>, _state: &mut State| {
                called = true;
                Ok(())
            },
        );

    assert_eq!(
        result,
        Err(EngineNavigationError::Navigation(
            StoryNavigationError::MissingPassage(String::from("Missing"))
        ))
    );
    assert!(!called);
    assert_eq!(state.temporary_get("turn"), Some(&Value::Number(3.0)));
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

#[test]
fn runtime_passage_execution_commits_through_the_engine_transaction() {
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
            body: vec![HirBodyNode {
                kind: HirBodyKind::Set(Box::new(parse("$score = 4").expect("set 表达式应可解析"))),
                span: TweeSpan {
                    start: 0,
                    end: 0,
                    line: 1,
                    column: 1,
                },
            }],
        }],
    };
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut runtime_story: EngineRuntimeStory = EngineRuntimeStory;
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let navigation: EngineNavigation<'_, '_, BodyExecution> = Engine::navigate(
        &mut state,
        &mut story,
        "Start",
        |passage: &HirPassage<'_>, state: &mut State| {
            let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
                RuntimeExecutionContext::new(&definitions, state, &mut runtime_story, &mut locals);
            runtime.execute_passage(passage)
        },
    )
    .expect("真实 Runtime Passage 应通过 Engine 提交");

    assert_eq!(navigation.output.control, BodyControl::Continue);
    assert_eq!(navigation.entry.passage().name, "Start");
    assert_eq!(state.variables_get("score"), Some(&Value::Number(4.0)));
    assert_eq!(story.current().map(|passage| passage.name), Some("Start"));
}

#[test]
fn engine_chain_carries_accumulated_render_output() {
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
                    kind: HirBodyKind::Text("你好，"),
                    span: TweeSpan {
                        start: 0,
                        end: 9,
                        line: 1,
                        column: 1,
                    },
                },
                HirBodyNode {
                    kind: HirBodyKind::Text("世界。"),
                    span: TweeSpan {
                        start: 9,
                        end: 18,
                        line: 1,
                        column: 10,
                    },
                },
            ],
        }],
    };
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut runtime_story: EngineRuntimeStory = EngineRuntimeStory;
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    let navigation: EngineNavigationChain<'_, '_> = Engine::navigate_chain_with_requests(
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
         limits: EngineExecutionLimits| {
            let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
                RuntimeExecutionContext::new(&definitions, state, &mut runtime_story, &mut locals);
            runtime.execute_passage_with_includes(passage, limits.includes)
        },
    )
    .expect("含文本的 Passage 应通过 Engine 提交");

    assert_eq!(navigation.entries.len(), 1);
    // Engine 按执行顺序接收整条链的有序输出：静态正文进入 Text。
    assert_eq!(navigation.output.len(), 2);
    assert_eq!(
        navigation.output.nodes()[0],
        PresentationNode::Text(TextValue::from("你好，"))
    );
    assert_eq!(
        navigation.output.nodes()[1],
        PresentationNode::Text(TextValue::from("世界。"))
    );
}

#[test]
fn display_phase_exposes_passage_output_to_host() {
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
            body: vec![HirBodyNode {
                kind: HirBodyKind::Text("森林入口。"),
                span: TweeSpan {
                    start: 0,
                    end: 15,
                    line: 1,
                    column: 1,
                },
            }],
        }],
    };
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut runtime_story: EngineRuntimeStory = EngineRuntimeStory;
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let params: Value = Value::Undefined;
    let displayed: RefCell<Option<PresentationOutput>> = RefCell::new(None);
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();

    Engine::navigate_chain_with_lifecycle(
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
         _state: &mut State| {
            if phase == PassageLifecyclePhase::Display {
                *displayed.borrow_mut() = context.output().cloned();
            }
            Ok(())
        },
        |passage: &HirPassage<'_>,
         state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         limits: EngineExecutionLimits| {
            let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
                RuntimeExecutionContext::new(&definitions, state, &mut runtime_story, &mut locals);
            runtime.execute_passage_with_includes(passage, limits.includes)
        },
    )
    .expect("Display 阶段应把本跳输出交给宿主");

    let output: PresentationOutput = displayed
        .borrow()
        .clone()
        .expect("Display 阶段应收到语义输出");
    assert_eq!(output.len(), 1);
    assert_eq!(
        output.nodes()[0],
        PresentationNode::Text(TextValue::from("森林入口。"))
    );
}

#[test]
fn engine_appends_safe_return_to_latest_navigation_target() {
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

    let navigation: EngineNavigationChain<'_, '_> = Engine::navigate_chain_with_requests(
        &mut state,
        &mut story,
        "Start",
        EngineExecutionLimits {
            passages: 2,
            includes: 0,
        },
        |passage: &HirPassage<'_>,
         _state: &mut State,
         requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            if passage.name == "Start" {
                // 第一跳：作者导航动作 + 发往 End 的 goto。
                requests.goto("End").expect("End 导航请求应有效");
                Ok::<BodyExecution, &'static str>(BodyExecution {
                    control: BodyControl::StopPassage,
                    output: PresentationOutput::from_nodes(vec![PresentationNode::Navigation {
                        role: crate::presentation::NavigationRole::Link,
                        id: InteractionId::from_key("start:choice:0"),
                        label: TextValue::from("去往"),
                        target: String::from("End"),
                    }]),
                })
            } else {
                // 第二跳：没有任何作者导航动作。
                Ok::<BodyExecution, &'static str>(BodyExecution {
                    control: BodyControl::Continue,
                    output: PresentationOutput::default(),
                })
            }
        },
    )
    .expect("两跳链应通过 Engine 提交");

    assert_eq!(navigation.entries.len(), 2);
    // 输出按执行顺序累积：Start 的作者导航 + End 追加的 SafeReturn。
    assert_eq!(navigation.output.len(), 2);
    assert!(matches!(
        navigation.output.nodes()[1],
        PresentationNode::SafeReturn { ref target, .. } if target == "Start"
    ));
}

#[test]
fn engine_skips_safe_return_when_output_has_navigation() {
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

    let navigation: EngineNavigationChain<'_, '_> = Engine::navigate_chain_with_requests(
        &mut state,
        &mut story,
        "Start",
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |_passage: &HirPassage<'_>,
         _state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            Ok::<BodyExecution, &'static str>(BodyExecution {
                control: BodyControl::Continue,
                output: PresentationOutput::from_nodes(vec![PresentationNode::Navigation {
                    role: crate::presentation::NavigationRole::Link,
                    id: InteractionId::from_key("start:choice:0"),
                    label: TextValue::from("去往"),
                    target: String::from("Forest"),
                }]),
            })
        },
    )
    .expect("含导航动作的 Passage 应通过 Engine 提交");

    assert_eq!(navigation.output.len(), 1);
    assert!(matches!(
        navigation.output.nodes()[0],
        PresentationNode::Navigation { .. }
    ));
}

#[test]
fn engine_skips_safe_return_when_history_has_no_target() {
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

    let navigation: EngineNavigationChain<'_, '_> = Engine::navigate_chain_with_requests(
        &mut state,
        &mut story,
        "Start",
        EngineExecutionLimits {
            passages: 1,
            includes: 0,
        },
        |_passage: &HirPassage<'_>,
         _state: &mut State,
         _requests: &mut StoryRuntimeRequests<'_, '_, '_>,
         _limits: EngineExecutionLimits| {
            Ok::<BodyExecution, &'static str>(BodyExecution {
                control: BodyControl::Continue,
                output: PresentationOutput::default(),
            })
        },
    )
    .expect("无导航动作的首跳应通过 Engine 提交");

    // 首跳历史中没有可安全返回的目标，不能生成指向不存在目标的 Link。
    assert!(navigation.output.is_empty());
}

#[test]
fn runtime_passage_error_rolls_back_through_the_engine_transaction() {
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
            body: vec![HirBodyNode {
                // Return 的返回值调用域尚未定义，Runtime 仍报告 UnsupportedNode。
                kind: HirBodyKind::Return(None),
                span: TweeSpan {
                    start: 0,
                    end: 0,
                    line: 1,
                    column: 1,
                },
            }],
        }],
    };
    let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &'static str>>> =
        MacroDefinitions::new();
    let mut runtime_story: EngineRuntimeStory = EngineRuntimeStory;
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let mut story: Story<'_, '_> = Story::new(&compiled);
    let mut state: State = State::new();
    let _score: Option<Value> = state.variables_set("score", Value::Number(1.0));

    let result: Result<
        EngineNavigation<'_, '_, BodyExecution>,
        EngineNavigationError<RuntimeExecutionError<&'static str>>,
    > = Engine::navigate(
        &mut state,
        &mut story,
        "Broken",
        |passage: &HirPassage<'_>, state: &mut State| {
            let mut runtime: RuntimeExecutionContext<'_, '_, '_, _, _> =
                RuntimeExecutionContext::new(&definitions, state, &mut runtime_story, &mut locals);
            runtime.execute_passage(passage)
        },
    );

    assert!(matches!(result, Err(EngineNavigationError::Execution(_))));
    assert_eq!(state.variables_get("score"), Some(&Value::Number(1.0)));
    assert_eq!(story.current(), None);
    assert!(story.history().is_empty());
}

// hir.rs 测试分片 02：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn reports_action_expression_error_at_argument() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<goto @>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "goto", "@");

    let error: HirError = HirStory::lower(&story).expect_err("非法 Passage Expression 必须失败");

    assert_eq!(error.diagnostic.code, "expression.invalid_variable");
    assert_eq!(error.diagnostic.location.expect("应保留参数位置").line, 2);
}

#[test]
fn lowers_unset_writable_targets() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let targets: [&str; 3] = ["$cache", "$profile.name", "$items[@index]"];

    for target in targets {
        let content: String = format!(":: Start\n<<unset {target}>>\n");
        let story: Story<'_> = story_with_macro(&source, &content, "unset", target);
        let hir: HirStory<'_> = HirStory::lower(&story).expect("可写目标应进入 unset HIR");

        assert!(matches!(
            hir.passages[0].body[0].kind,
            HirBodyKind::Unset(_)
        ));
    }
}

#[test]
fn rejects_invalid_unset_target() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let target: &str = "$profile?.name";
    let content: String = format!(":: Start\n<<unset {target}>>\n");
    let story: Story<'_> = story_with_macro(&source, &content, "unset", target);

    let error: HirError = HirStory::lower(&story).expect_err("可选链不能作为删除目标");

    assert_eq!(error.diagnostic.code, "hir.invalid_unset_target");
    assert!(error.diagnostic.location.is_some());
}

#[test]
fn lowers_switch_cases_and_default() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<switch $value>>\n<<case 1>>\n一\n<<case 2>>\n二\n<<default>>\n其它\n<</switch>>\n";
    let switch_start: usize = content.find("<<switch").expect("测试 switch 应存在");
    let switch_argument: usize = content.find("$value").expect("测试参数应存在");
    let case_one_start: usize = content.find("<<case 1>>").expect("测试 case 应存在");
    let case_two_start: usize = content.find("<<case 2>>").expect("测试 case 应存在");
    let default_start: usize = content.find("<<default>>").expect("测试 default 应存在");
    let closing_start: usize = content.find("<</switch>>").expect("测试闭合符应存在");
    let case_node = |start: usize, value: &'static str, body: &'static str, end: usize| BodyNode {
        kind: BodyNodeKind::Macro(MacroNode {
            name: "case",
            arguments: value,
            arguments_span: Span {
                start: start + "<<case ".len(),
                end: start + "<<case ".len() + value.len(),
                line: 3,
                column: 8,
            },
            syntax_kind: crate::twee::MacroSyntaxKind::Inline,
            body: vec![BodyNode {
                kind: BodyNodeKind::Text(body),
                span: Span {
                    start: start + "<<case 1>>\n".len(),
                    end,
                    line: 4,
                    column: 1,
                },
            }],
        }),
        span: Span {
            start,
            end,
            line: 3,
            column: 1,
        },
    };
    let story: Story<'_> = Story::from_passages(vec![Passage {
        source: &source.path,
        content,
        name: "Start",
        tags: Vec::new(),
        body: vec![BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name: "switch",
                arguments: "$value",
                arguments_span: Span {
                    start: switch_argument,
                    end: switch_argument + "$value".len(),
                    line: 2,
                    column: 10,
                },
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: vec![
                    case_node(case_one_start, "1", "一\n", case_two_start),
                    case_node(case_two_start, "2", "二\n", default_start),
                    BodyNode {
                        kind: BodyNodeKind::Macro(MacroNode {
                            name: "default",
                            arguments: "",
                            arguments_span: Span {
                                start: default_start + "<<default".len(),
                                end: default_start + "<<default".len(),
                                line: 7,
                                column: 10,
                            },
                            syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                            body: vec![BodyNode {
                                kind: BodyNodeKind::Text("其它\n"),
                                span: Span {
                                    start: default_start + "<<default>>\n".len(),
                                    end: closing_start,
                                    line: 8,
                                    column: 1,
                                },
                            }],
                        }),
                        span: Span {
                            start: default_start,
                            end: closing_start,
                            line: 7,
                            column: 1,
                        },
                    },
                ],
            }),
            span: Span {
                start: switch_start,
                end: content.len(),
                line: 2,
                column: 1,
            },
        }],
        span: Span {
            start: 0,
            end: content.len(),
            line: 1,
            column: 1,
        },
    }])
    .expect("单个 Passage 应有效");

    let hir: HirStory<'_> = HirStory::lower(&story).expect("switch 应进入结构化 HIR");
    let HirBodyKind::Switch(switch) = &hir.passages[0].body[0].kind else {
        panic!("switch 应为专用 HIR");
    };

    assert_eq!(switch.cases.len(), 2);
    assert!(switch.default.is_some());
}

#[test]
fn rejects_orphan_switch_clause() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<case 1>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "case", "1");

    let error: HirError = HirStory::lower(&story).expect_err("case 不能脱离 switch");

    assert_eq!(error.diagnostic.code, "hir.orphan_clause");
}

#[test]
fn lowers_widget_name_without_parameter_declarations() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<widget greet>><</widget>>\n";
    let mut story: Story<'_> = story_with_macro(&source, content, "widget", "greet");
    story.passages[0].tags.push("widget");

    let hir: HirStory<'_> = HirStory::lower(&story).expect("Widget 声明应有效");
    let HirBodyKind::Widget(widget) = &hir.passages[0].body[0].kind else {
        panic!("widget 应进入专用 HIR");
    };

    assert_eq!(widget.name, "greet");
    assert!(!hir.passages[0].emits_text());
}

#[test]
fn rejects_widget_definition_outside_a_widget_tagged_passage() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<widget greet>><</widget>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "widget", "greet");

    let error: HirError =
        HirStory::lower(&story).expect_err("Widget 定义 Passage 必须声明 widget Tag");

    assert_eq!(error.diagnostic.code, "hir.widget_tag_required");
}

#[test]
fn rejects_duplicate_widget_names_across_twee_passages() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let mut first: Story<'_> = story_with_macro(
        &source,
        ":: WidgetsA [widget]\n<<widget greet>><</widget>>\n",
        "widget",
        "greet",
    );
    first.passages[0].name = "WidgetsA";
    first.passages[0].tags.push("widget");
    let mut second: Story<'_> = story_with_macro(
        &source,
        ":: WidgetsB [widget]\n<<widget greet>><</widget>>\n",
        "widget",
        "greet",
    );
    second.passages[0].name = "WidgetsB";
    second.passages[0].tags.push("widget");
    let story: Story<'_> =
        Story::from_passages(vec![first.passages.remove(0), second.passages.remove(0)])
            .expect("PassageName 各不相同");

    let error: HirError = HirStory::lower(&story).expect_err("Twee Widget 名称必须全局唯一");

    assert_eq!(error.diagnostic.code, "hir.duplicate_widget");
    assert_eq!(error.diagnostic.location.expect("应定位第二个定义").line, 2);
}

#[test]
fn rejects_widget_definition_nested_inside_another_macro() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let cases: [(&str, &str); 2] = [("if", "true"), ("silently", "")];

    for (outer_name, outer_arguments) in cases {
        let content: String = format!(
            ":: Widgets [widget]\n<<{outer_name}{separator}{outer_arguments}>><<widget greet>><</widget>><</{outer_name}>>\n",
            separator = if outer_arguments.is_empty() { "" } else { " " },
        );
        let mut story: Story<'_> = story_with_macro(&source, &content, outer_name, outer_arguments);
        story.passages[0].tags.push("widget");
        let BodyNodeKind::Macro(outer_macro) = &mut story.passages[0].body[0].kind else {
            panic!("测试正文应为外层 Macro");
        };
        let widget_start: usize = content.find("<<widget").expect("应包含 Widget");
        outer_macro.body.push(BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name: "widget",
                arguments: "greet",
                arguments_span: Span {
                    start: widget_start + "<<widget ".len(),
                    end: widget_start + "<<widget greet".len(),
                    line: 2,
                    column: 1,
                },
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: Vec::new(),
            }),
            span: Span {
                start: widget_start,
                end: content
                    .find(&format!("<</{outer_name}>>"))
                    .expect("应包含外层结尾"),
                line: 2,
                column: 1,
            },
        });

        let error: HirError =
            HirStory::lower(&story).expect_err("Widget 定义只能位于 Passage 顶层");

        assert_eq!(error.diagnostic.code, "hir.nested_widget");
    }
}

#[test]
fn rejects_non_widget_node_in_widget_tagged_passage() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Widgets [widget]\n<<set $ready = true>>\n";
    let mut story: Story<'_> = story_with_macro(&source, content, "set", "$ready = true");
    story.passages[0].tags.push("widget");

    let error: HirError = HirStory::lower(&story).expect_err("Widget 定义容器不能混入可执行节点");

    assert_eq!(error.diagnostic.code, "hir.invalid_widget_content");
    assert_eq!(error.diagnostic.location.expect("应定位无效节点").line, 2);
}

#[test]
fn allows_formatting_whitespace_in_widget_tagged_passage() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Widgets [widget]\n\n<<widget greet>><</widget>>\n";
    let mut story: Story<'_> = story_with_macro(&source, content, "widget", "greet");
    story.passages[0].tags.push("widget");
    story.passages[0].body.insert(
        0,
        BodyNode {
            kind: BodyNodeKind::Text("\n"),
            span: Span {
                start: 20,
                end: 21,
                line: 2,
                column: 1,
            },
        },
    );

    HirStory::lower(&story).expect("排版空白不应成为可执行内容");
}

#[test]
fn accepts_a_quoted_widget_identifier_without_storing_the_quotes() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Widgets [widget]\n<<widget \"greet\">><</widget>>\n";
    let mut story: Story<'_> = story_with_macro(&source, content, "widget", "\"greet\"");
    story.passages[0].tags.push("widget");

    let hir = HirStory::lower(&story).expect("引号内的合法标识符应成为 Widget 名");
    let HirBodyKind::Widget(widget) = &hir.passages[0].body[0].kind else {
        panic!("应降低为 Widget")
    };
    assert_eq!(widget.name, "greet");
}

#[test]
fn rejects_widget_without_name() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<widget>><</widget>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "widget", "");

    let error: HirError = HirStory::lower(&story).expect_err("Widget 必须声明名称");

    assert_eq!(error.diagnostic.code, "hir.invalid_widget_name");
}

#[test]
fn rejects_invalid_widget_name() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<widget 9greet>><</widget>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "widget", "9greet");

    let error: HirError = HirStory::lower(&story).expect_err("Widget 名称必须是有效标识符");

    assert_eq!(error.diagnostic.code, "hir.invalid_widget_name");
}

#[test]
fn rejects_widget_parameter_declarations() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<widget greet @name>><</widget>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "widget", "greet @name");

    let error: HirError = HirStory::lower(&story).expect_err("Widget 不声明命名参数");

    assert_eq!(error.diagnostic.code, "hir.unexpected_widget_arguments");
}

#[test]
fn rejects_container_syntax_when_calling_a_widget() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let definition_content: &str = ":: Widgets [widget]\n<<widget card>><</widget>>\n";
    let call_content: &str = ":: Start\n<<card>><</card>>\n";
    let definition = Passage {
        source: &source.path,
        content: definition_content,
        name: "Widgets",
        tags: vec!["widget"],
        body: vec![BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name: "widget",
                arguments: "card",
                arguments_span: Span { start: 30, end: 34, line: 2, column: 10 },
                syntax_kind: crate::twee::MacroSyntaxKind::Container,
                body: Vec::new(),
            }),
            span: Span { start: 21, end: definition_content.len(), line: 2, column: 1 },
        }],
        span: Span { start: 0, end: definition_content.len(), line: 1, column: 1 },
    };
    let call = Passage {
        source: &source.path,
        content: call_content,
        name: "Start",
        tags: Vec::new(),
        body: vec![BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name: "card",
                arguments: "",
                arguments_span: Span { start: 16, end: 16, line: 2, column: 7 },
                syntax_kind: crate::twee::MacroSyntaxKind::Container,
                body: Vec::new(),
            }),
            span: Span { start: 9, end: call_content.len(), line: 2, column: 1 },
        }],
        span: Span { start: 0, end: call_content.len(), line: 1, column: 1 },
    };
    let story: Story<'_> = Story::from_passages(vec![definition, call]).expect("Passage 应有效");

    let error: HirError = HirStory::lower(&story).expect_err("Widget 调用必须是 Inline");

    assert_eq!(error.diagnostic.code, "hir.widget_call_must_be_inline");
}

#[test]
fn lowers_capture_local_variable_list() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<capture @name @index>><</capture>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "capture", "@name @index");

    let hir: HirStory<'_> = HirStory::lower(&story).expect("capture 应接受 @ 局部变量列表");
    let HirBodyKind::Capture(capture) = &hir.passages[0].body[0].kind else {
        panic!("capture 应进入专用 HIR");
    };

    assert_eq!(capture.locals, vec!["name", "index"]);
}

#[test]
fn rejects_capture_without_local_variables() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<capture $name>><</capture>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "capture", "$name");

    let error: HirError = HirStory::lower(&story).expect_err("capture 只能接受 @ 局部变量");

    assert_eq!(error.diagnostic.code, "hir.invalid_capture_variable");
}

#[test]
fn rejects_capture_without_arguments() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<capture>><</capture>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "capture", "");

    let error: HirError = HirStory::lower(&story).expect_err("capture 至少需要一个局部变量");

    assert_eq!(error.diagnostic.code, "hir.invalid_capture_arguments");
}

#[test]
fn rejects_duplicate_capture_local_variable() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<capture @name @name>><</capture>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "capture", "@name @name");

    let error: HirError = HirStory::lower(&story).expect_err("capture 不应重复捕获同名变量");

    assert_eq!(error.diagnostic.code, "hir.duplicate_capture_variable");
}

#[test]
fn widget_does_not_inherit_outer_loop_context() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str =
        ":: Start\n<<while true>>\n<<widget greet>>\n<<break>>\n<</widget>>\n<</while>>\n";
    let while_start: usize = content.find("<<while").expect("测试 while 应存在");
    let widget_start: usize = content.find("<<widget").expect("测试 widget 应存在");
    let break_start: usize = content.find("<<break").expect("测试 break 应存在");
    let story: Story<'_> = Story::from_passages(vec![Passage {
        source: &source.path,
        content,
        name: "Start",
        tags: Vec::new(),
        body: vec![BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name: "while",
                arguments: "true",
                arguments_span: Span {
                    start: while_start + "<<while ".len(),
                    end: while_start + "<<while true".len(),
                    line: 2,
                    column: 9,
                },
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: vec![BodyNode {
                    kind: BodyNodeKind::Macro(MacroNode {
                        name: "widget",
                        arguments: "greet",
                        arguments_span: Span {
                            start: widget_start + "<<widget ".len(),
                            end: widget_start + "<<widget greet".len(),
                            line: 3,
                            column: 10,
                        },
                        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                        body: vec![BodyNode {
                            kind: BodyNodeKind::Macro(MacroNode {
                                name: "break",
                                arguments: "",
                                arguments_span: Span {
                                    start: break_start + "<<break".len(),
                                    end: break_start + "<<break".len(),
                                    line: 4,
                                    column: 8,
                                },
                                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                                body: Vec::new(),
                            }),
                            span: Span {
                                start: break_start,
                                end: break_start + "<<break>>\n".len(),
                                line: 4,
                                column: 1,
                            },
                        }],
                    }),
                    span: Span {
                        start: widget_start,
                        end: content.find("<</widget>>").expect("测试闭合标记应存在")
                            + "<</widget>>\n".len(),
                        line: 3,
                        column: 1,
                    },
                }],
            }),
            span: Span {
                start: while_start,
                end: content.len(),
                line: 2,
                column: 1,
            },
        }],
        span: Span {
            start: 0,
            end: content.len(),
            line: 1,
            column: 1,
        },
    }])
    .expect("单个 Passage 应有效");

    let error: HirError =
        HirStory::lower(&story).expect_err("Widget 内的 break 不得使用外层循环上下文");

    assert_eq!(error.diagnostic.code, "hir.loop_control_outside_loop");
}

#[test]
fn lowers_exit_without_restricting_it_to_a_loop() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<exit>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "exit", "");

    let hir: HirStory<'_> = HirStory::lower(&story).expect("exit 可用于 Passage 当前执行域");

    assert!(matches!(hir.passages[0].body[0].kind, HirBodyKind::Exit));
}

#[test]
fn lowers_return_to_a_reserved_hir_node_without_runtime_semantics() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<return $value>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "return", "$value");

    let hir: HirStory<'_> = HirStory::lower(&story).expect("return 应进入保留 HIR 节点");

    assert!(matches!(
        hir.passages[0].body[0].kind,
        HirBodyKind::Return(Some(_))
    ));
}

#[test]
fn rejects_exit_arguments() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<exit 1>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "exit", "1");

    let error: HirError = HirStory::lower(&story).expect_err("exit 不接受返回值或其他参数");

    assert_eq!(error.diagnostic.code, "hir.unexpected_macro_arguments");
}

fn story_with_macro<'source>(
    source: &'source Source,
    content: &'source str,
    name: &'source str,
    arguments: &'source str,
) -> Story<'source> {
    let macro_start: usize = content.find("<<").expect("测试 Macro 应存在");
    let arguments_start: usize = if arguments.is_empty() {
        macro_start + "<<".len() + name.len()
    } else {
        content.find(arguments).expect("测试参数应存在")
    };
    Story::from_passages(vec![Passage {
        source: &source.path,
        content,
        name: "Start",
        tags: Vec::new(),
        body: vec![BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name,
                arguments,
                arguments_span: Span {
                    start: arguments_start,
                    end: arguments_start + arguments.len(),
                    line: 2,
                    column: arguments_start - macro_start + 1,
                },
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: Vec::new(),
            }),
            span: Span {
                start: macro_start,
                end: content.len(),
                line: 2,
                column: 1,
            },
        }],
        span: Span {
            start: 0,
            end: content.len(),
            line: 1,
            column: 1,
        },
    }])
    .expect("单个 Passage 应有效")
}

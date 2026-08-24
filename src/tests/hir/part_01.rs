// hir.rs 测试分片 01：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn lowers_text_and_print_to_hir() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n位置：<<print $place>>\n";
    let story: Story<'_> = Story::from_passages(vec![Passage {
        source: &source.path,
        content,
        name: "Start",
        tags: Vec::new(),
        body: vec![
            BodyNode {
                kind: BodyNodeKind::Text("位置："),
                span: Span {
                    start: 9,
                    end: 18,
                    line: 2,
                    column: 1,
                },
            },
            BodyNode {
                kind: BodyNodeKind::Macro(MacroNode {
                    name: "print",
                    arguments: "$place",
                    arguments_span: Span {
                        start: 26,
                        end: 32,
                        line: 2,
                        column: 12,
                    },
                    syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                    body: Vec::new(),
                }),
                span: Span {
                    start: 18,
                    end: 34,
                    line: 2,
                    column: 4,
                },
            },
        ],
        span: Span {
            start: 0,
            end: 35,
            line: 1,
            column: 1,
        },
    }])
    .expect("单个 Passage 应有效");

    let hir: HirStory<'_> = HirStory::lower(&story).expect("有效 Expression 应进入 HIR");

    assert_eq!(hir.passages[0].name, "Start");
    assert!(matches!(
        hir.passages[0].body[0].kind,
        HirBodyKind::Text("位置：")
    ));
    assert!(matches!(
        hir.passages[0].body[1].kind,
        HirBodyKind::Print(_)
    ));
}

#[test]
fn reports_print_parse_error_at_twee_location() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n位置：<<print ${@}>>\n";
    let story: Story<'_> = Story::from_passages(vec![Passage {
        source: &source.path,
        content,
        name: "Start",
        tags: Vec::new(),
        body: vec![BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name: "print",
                arguments: "${@}",
                arguments_span: Span {
                    start: 26,
                    end: 30,
                    line: 2,
                    column: 12,
                },
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: Vec::new(),
            }),
            span: Span {
                start: 18,
                end: 32,
                line: 2,
                column: 4,
            },
        }],
        span: Span {
            start: 0,
            end: 33,
            line: 1,
            column: 1,
        },
    }])
    .expect("单个 Passage 应有效");

    let error: HirError = HirStory::lower(&story).expect_err("无名称 @ 变量必须失败");

    assert_eq!(error.diagnostic.code, "expression.invalid_variable");
    assert_eq!(error.diagnostic.location.expect("应有 Twee 位置").start, 28);
}

#[test]
fn lowers_generic_macro_without_binding_runtime_definition() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: Story<'_> = Story::build(std::slice::from_ref(&source)).expect("示例 Story 应有效");

    let hir: HirStory<'_> = HirStory::lower(&story).expect("通用 Macro 应进入 HIR");
    let HirBodyKind::Macro(link) = &hir.passages[0].body[1].kind else {
        panic!("示例的 link 应保留为通用 HIR Macro");
    };

    assert_eq!(link.name, "link");
    assert_eq!(
        link.arguments,
        HirMacroArguments::Raw("[[查看四周|LookAround]]")
    );
    assert!(link.body.is_empty());
}

#[test]
fn recursively_lowers_macro_body() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<if $ready>>\n<<print @name>>\n<<elseif $other>>\n其它\n<<else>>\n默认\n<</if>>\n";
    let expression_start: usize = content.find("@name").expect("测试 Expression 应存在");
    let elseif_start: usize = content.find("<<elseif").expect("测试 elseif 应存在");
    let elseif_argument_start: usize = content.find("$other").expect("测试条件应存在");
    let else_start: usize = content.find("<<else>>").expect("测试 else 应存在");
    let print: BodyNode<'_> = BodyNode {
        kind: BodyNodeKind::Macro(MacroNode {
            name: "print",
            arguments: "@name",
            arguments_span: Span {
                start: expression_start,
                end: expression_start + "@name".len(),
                line: 3,
                column: 9,
            },
            syntax_kind: crate::twee::MacroSyntaxKind::Inline,
            body: Vec::new(),
        }),
        span: Span {
            start: expression_start - "<<print ".len(),
            end: expression_start + "@name>>".len(),
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
                name: "if",
                arguments: "$ready",
                arguments_span: Span {
                    start: 14,
                    end: 20,
                    line: 2,
                    column: 6,
                },
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: vec![
                    print,
                    BodyNode {
                        kind: BodyNodeKind::Macro(MacroNode {
                            name: "elseif",
                            arguments: "$other",
                            arguments_span: Span {
                                start: elseif_argument_start,
                                end: elseif_argument_start + "$other".len(),
                                line: 4,
                                column: 10,
                            },
                            syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                            body: vec![BodyNode {
                                kind: BodyNodeKind::Text("其它\n"),
                                span: Span {
                                    start: elseif_argument_start + "$other>>\n".len(),
                                    end: else_start,
                                    line: 5,
                                    column: 1,
                                },
                            }],
                        }),
                        span: Span {
                            start: elseif_start,
                            end: else_start,
                            line: 4,
                            column: 1,
                        },
                    },
                    BodyNode {
                        kind: BodyNodeKind::Macro(MacroNode {
                            name: "else",
                            arguments: "",
                            arguments_span: Span {
                                start: else_start + "<<else".len(),
                                end: else_start + "<<else".len(),
                                line: 6,
                                column: 7,
                            },
                            syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                            body: vec![BodyNode {
                                kind: BodyNodeKind::Text("默认\n"),
                                span: Span {
                                    start: else_start + "<<else>>\n".len(),
                                    end: content.find("<</if>>").expect("测试闭合符应存在"),
                                    line: 7,
                                    column: 1,
                                },
                            }],
                        }),
                        span: Span {
                            start: else_start,
                            end: content.find("<</if>>").expect("测试闭合符应存在"),
                            line: 6,
                            column: 1,
                        },
                    },
                ],
            }),
            span: Span {
                start: 9,
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

    let hir: HirStory<'_> = HirStory::lower(&story).expect("嵌套正文应递归进入 HIR");
    let HirBodyKind::If(if_node) = &hir.passages[0].body[0].kind else {
        panic!("外层节点应为结构化 if");
    };

    assert_eq!(if_node.branches.len(), 2);
    assert!(matches!(
        if_node.branches[0].body[0].kind,
        HirBodyKind::Print(_)
    ));
    assert!(matches!(
        if_node.branches[1].body[0].kind,
        HirBodyKind::Text("其它\n")
    ));
    assert!(matches!(
        if_node.fallback.as_ref().expect("应保留 else")[0].kind,
        HirBodyKind::Text("默认\n")
    ));
}

#[test]
fn keeps_unknown_macro_arguments_raw() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: Story<'_> = Story::build(std::slice::from_ref(&source)).expect("示例 Story 应有效");
    let hir: HirStory<'_> = HirStory::lower(&story).expect("未知参数语法不应由编译器解释");
    let HirBodyKind::Macro(link) = &hir.passages[0].body[1].kind else {
        panic!("示例第二个节点应为 link Macro");
    };

    assert_eq!(
        link.arguments,
        HirMacroArguments::Raw("[[查看四周|LookAround]]")
    );
}

#[test]
fn reports_logical_macro_expression_error_at_argument_location() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<while @>>\n<</while>>\n";
    let argument_start: usize = content.find('@').expect("测试参数应存在");
    let story: Story<'_> = Story::from_passages(vec![Passage {
        source: &source.path,
        content,
        name: "Start",
        tags: Vec::new(),
        body: vec![BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name: "while",
                arguments: "@",
                arguments_span: Span {
                    start: argument_start,
                    end: argument_start + 1,
                    line: 2,
                    column: 9,
                },
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: Vec::new(),
            }),
            span: Span {
                start: 9,
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

    let error: HirError = HirStory::lower(&story).expect_err("非法 while 条件必须失败");
    let location = error.diagnostic.location.expect("错误应指向 Macro 参数");

    assert_eq!(error.diagnostic.code, "expression.invalid_variable");
    assert_eq!(location.start, argument_start);
    assert_eq!(location.line, 2);
    assert_eq!(location.column, 9);
}

#[test]
fn lowers_three_for_argument_forms() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let cases: [(&str, &str); 4] = [
        ("@key in $object", "in"),
        ("@value of $items", "of"),
        ("@index range 1 to $end step 2", "range"),
        (
            "@index range number('1 to 2') to $end step number('3 step 4')",
            "range",
        ),
    ];

    for (arguments, expected_kind) in cases {
        let content: String = format!(":: Start\n<<for {arguments}>>\n<</for>>\n");
        let story: Story<'_> = story_with_macro(&source, &content, "for", arguments);
        let hir: HirStory<'_> = HirStory::lower(&story).expect("有效 for 参数应进入 HIR");
        let HirBodyKind::For(for_node) = &hir.passages[0].body[0].kind else {
            panic!("for 应转换为专用 HIR 节点");
        };

        match (&for_node.kind, expected_kind) {
            (HirForKind::In { .. }, "in")
            | (HirForKind::Of { .. }, "of")
            | (HirForKind::Range { .. }, "range") => {}
            _ => panic!("for 参数形式不匹配"),
        }
    }
}

#[test]
fn rejects_non_variable_for_target() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let arguments: &str = "$items[0] of $items";
    let content: String = format!(":: Start\n<<for {arguments}>>\n<</for>>\n");
    let story: Story<'_> = story_with_macro(&source, &content, "for", arguments);

    let error: HirError = HirStory::lower(&story).expect_err("for 目标不能是索引位置");

    assert_eq!(error.diagnostic.code, "hir.invalid_for_target");
    assert!(error.diagnostic.location.is_some());
}

#[test]
fn lowers_while_and_accepts_loop_control_in_its_body() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<while $ready>>\n<<break>>\n<</while>>\n";
    let while_arguments_start: usize = content.find("$ready").expect("测试条件应存在");
    let break_start: usize = content.find("<<break>>").expect("测试 break 应存在");
    let story: Story<'_> = Story::from_passages(vec![Passage {
        source: &source.path,
        content,
        name: "Start",
        tags: Vec::new(),
        body: vec![BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name: "while",
                arguments: "$ready",
                arguments_span: Span {
                    start: while_arguments_start,
                    end: while_arguments_start + "$ready".len(),
                    line: 2,
                    column: 9,
                },
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: vec![BodyNode {
                    kind: BodyNodeKind::Macro(MacroNode {
                        name: "break",
                        arguments: "",
                        arguments_span: Span {
                            start: break_start + "<<break".len(),
                            end: break_start + "<<break".len(),
                            line: 3,
                            column: 8,
                        },
                        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                        body: Vec::new(),
                    }),
                    span: Span {
                        start: break_start,
                        end: break_start + "<<break>>\n".len(),
                        line: 3,
                        column: 1,
                    },
                }],
            }),
            span: Span {
                start: 9,
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

    let hir: HirStory<'_> = HirStory::lower(&story).expect("循环内 break 应有效");
    let HirBodyKind::While(while_node) = &hir.passages[0].body[0].kind else {
        panic!("while 应转换为专用 HIR");
    };

    assert!(matches!(while_node.body[0].kind, HirBodyKind::Break));
}

#[test]
fn rejects_loop_control_outside_loop() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<continue>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "continue", "");

    let error: HirError = HirStory::lower(&story).expect_err("循环外 continue 必须失败");

    assert_eq!(error.diagnostic.code, "hir.loop_control_outside_loop");
}

#[test]
fn rejects_loop_control_arguments() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let content: &str = ":: Start\n<<break now>>\n";
    let story: Story<'_> = story_with_macro(&source, content, "break", "now");

    let error: HirError = HirStory::lower(&story).expect_err("break 不接受参数");

    assert_eq!(error.diagnostic.code, "hir.unexpected_macro_arguments");
}

#[test]
fn normalizes_set_to_and_equals_to_assignment() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let cases: [&str; 2] = ["$test to \"test\"", "$test = \"test\""];

    for arguments in cases {
        let content: String = format!(":: Start\n<<set {arguments}>>\n");
        let story: Story<'_> = story_with_macro(&source, &content, "set", arguments);
        let hir: HirStory<'_> = HirStory::lower(&story).expect("两种 set 拼写都应有效");
        let HirBodyKind::Set(assignment) = &hir.passages[0].body[0].kind else {
            panic!("set 应降低为专用赋值节点");
        };

        assert!(matches!(
            assignment.kind,
            crate::expression::ExpressionKind::Assignment {
                operator: crate::expression::AssignmentOperator::Assign,
                ..
            }
        ));
    }
}

#[test]
fn set_to_ignores_nested_keyword_text() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let arguments: &str = "$test to string('from to destination')";
    let content: String = format!(":: Start\n<<set {arguments}>>\n");
    let story: Story<'_> = story_with_macro(&source, &content, "set", arguments);

    let hir: HirStory<'_> = HirStory::lower(&story).expect("字符串内 to 不应切分参数");

    assert!(matches!(hir.passages[0].body[0].kind, HirBodyKind::Set(_)));
}

#[test]
fn rejects_invalid_set_target_and_missing_separator() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let invalid_target: &str = "1 to $value";
    let missing_separator: &str = "$value \"text\"";
    let invalid_content: String = format!(":: Start\n<<set {invalid_target}>>\n");
    let missing_content: String = format!(":: Start\n<<set {missing_separator}>>\n");

    let target_error: HirError = HirStory::lower(&story_with_macro(
        &source,
        &invalid_content,
        "set",
        invalid_target,
    ))
    .expect_err("字面量不能作为 set 目标");
    let separator_error: HirError = HirStory::lower(&story_with_macro(
        &source,
        &missing_content,
        "set",
        missing_separator,
    ))
    .expect_err("set 缺少分隔符必须失败");

    assert_eq!(target_error.diagnostic.code, "hir.invalid_set_target");
    assert_eq!(separator_error.diagnostic.code, "hir.invalid_set_arguments");
}

#[test]
fn lowers_run_include_and_goto_as_distinct_actions() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let cases: [(&str, &str); 3] = [("run", "$count++"), ("include", "$next"), ("goto", "'End'")];

    for (name, arguments) in cases {
        let content: String = format!(":: Start\n<<{name} {arguments}>>\n");
        let story: Story<'_> = story_with_macro(&source, &content, name, arguments);
        let hir: HirStory<'_> = HirStory::lower(&story).expect("有效动作参数应进入 HIR");

        match (name, &hir.passages[0].body[0].kind) {
            ("run", HirBodyKind::Run(_))
            | ("include", HirBodyKind::Include(_))
            | ("goto", HirBodyKind::Goto(_)) => {}
            _ => panic!("动作 Macro 应保持不同 HIR 语义"),
        }
    }
}

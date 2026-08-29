// twee.rs 测试分片 01：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn parse_fragment_keeps_native_text_literal_and_parses_macro() {
    let source: SourcePath = SourcePath::fragment();
    // Native Twee 正文不会自动解释 `${}`。
    let inline: Vec<BodyNode<'_>> = parse_fragment("你好 ${$x}", &source).expect("片段应可解析");
    assert_eq!(inline.len(), 1);
    assert_eq!(inline[0].kind, BodyNodeKind::Text("你好 ${$x}"));

    // 同一行完整闭合的通用 Macro 外壳解析为 Macro 节点。
    let macro_line: Vec<BodyNode<'_>> =
        parse_fragment("<<link [[去|X]]>><</link>>", &source).expect("宏片段应可解析");
    assert!(matches!(
        macro_line[0].kind,
        BodyNodeKind::Macro(MacroNode {
            name: "link",
            syntax_kind: crate::twee::MacroSyntaxKind::Container,
            ..
        })
    ));
}

#[test]
fn parse_fragment_accepts_inline_slot_and_replace_containers() {
    let source = SourcePath::fragment();
    let nodes = parse_fragment(
        r#"<<slot "abc">><</slot>><<replace "abc">>新内容<</replace>>"#,
        &source,
    )
    .expect("同一行 slot 与 replace 容器应可解析");

    assert!(matches!(
        nodes.as_slice(),
        [
            BodyNode { kind: BodyNodeKind::Macro(MacroNode { name: "slot", .. }), .. },
            BodyNode { kind: BodyNodeKind::Macro(MacroNode { name: "replace", .. }), .. }
        ]
    ), "{nodes:#?}");
}

#[test]
fn silently_text_inside_print_literal_is_not_a_container() {
    let source: SourcePath = SourcePath::fragment();
    let nodes: Vec<BodyNode<'_>> = parse_fragment(r#"<<print `<<silently>>`>>"#, &source)
        .expect("print 字面参数里的 Macro 字符应被忽略");
    let BodyNodeKind::Macro(print) = &nodes[0].kind else {
        panic!("应只解析外层 print");
    };

    assert_eq!(nodes.len(), 1);
    assert_eq!(print.name, "print");
    assert_eq!(print.arguments, "`<<silently>>`");
    assert_eq!(print.syntax_kind, crate::twee::MacroSyntaxKind::Inline);
}

#[test]
fn macro_header_requires_top_level_left_shift_to_be_grouped() {
    let source: SourcePath = SourcePath::fragment();
    let grouped: Vec<BodyNode<'_>> =
        parse_fragment("<<print ($count << 1)>>", &source).expect("分组位移应可解析");
    let BodyNodeKind::Macro(print) = &grouped[0].kind else {
        panic!("print 应解析为 Macro");
    };
    assert_eq!(print.arguments, "($count << 1)");

    let error: ParseError<'_> =
        parse_fragment("<<print $count << 1>>", &source).expect_err("顶层位移必须拒绝");
    assert_eq!(error.kind, ParseErrorKind::UngroupedMacroShift);
    assert_eq!(error.diagnostic().code, "twee.ungrouped_macro_shift");
}

#[test]
fn lexes_passage_declaration_name_and_tags() {
    let source: Source = Source {
        path: SourcePath::from_path(Path::new("story/tags.twee")).expect("测试路径应有效"),
        kind: SourceKind::Twee,
        content: ":: Hall [opening forest]\n正文\n".to_owned(),
    };
    let tokens: Vec<Token<'_>> = lex(&source);

    assert_eq!(tokens.len(), 2);
    assert_eq!(
        tokens[0].kind,
        TokenKind::PassageDeclaration {
            name: "Hall",
            tags: vec!["opening", "forest"],
        }
    );
    assert_eq!(tokens[0].span.line, 1);
    assert_eq!(tokens[1].kind, TokenKind::Text("正文\n"));
}

#[test]
fn keeps_link_macro_as_text() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = lex(&source);

    assert_eq!(
        tokens[2].kind,
        TokenKind::Text("<<link [[查看四周|LookAround]]>><</link>>\n")
    );
}

#[test]
fn groups_declaration_and_text_into_passage() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = lex(&source);
    let passages: Vec<Passage<'_>> = parse(&tokens).expect("有效 Token 应组成 Passage");

    assert_eq!(passages.len(), 1);
    assert_eq!(passages[0].name, "Start");
    assert!(passages[0].tags.is_empty());
    assert_eq!(
        passages[0].body,
        vec![
            BodyNode {
                kind: BodyNodeKind::Text("你在森林中醒来。\n"),
                span: crate::twee::Span {
                    start: 9,
                    end: 34,
                    line: 2,
                    column: 1,
                },
            },
            BodyNode {
                kind: BodyNodeKind::Macro(MacroNode {
                    name: "link",
                    arguments: "[[查看四周|LookAround]]",
                    arguments_span: crate::twee::Span {
                        start: 41,
                        end: 68,
                        line: 3,
                        column: 8,
                    },
                    syntax_kind: crate::twee::MacroSyntaxKind::Container,
                    body: Vec::new(),
                }),
                span: crate::twee::Span {
                    start: 34,
                    end: 80,
                    line: 3,
                    column: 1,
                },
            },
        ]
    );
}

#[test]
fn keeps_markup_like_content_as_text_without_native_surface_semantics() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let markup_like_text: &str =
        "第一行<br>\n<div class=\"dialog\" data-speaker=\"Author\">正文</div>\n";
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(&source, TokenKind::Text(markup_like_text), 8, 78, 2),
    ];

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("标签文本应作为普通正文保留");

    assert_eq!(passages[0].body.len(), 1);
    assert_eq!(
        passages[0].body[0].kind,
        BodyNodeKind::Text(markup_like_text)
    );
}

#[test]
fn removes_twee_comment_from_passage_body() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let text: &str = "之前/% 仅供作者阅读 %/之后\n";
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(&source, TokenKind::Text(text), 8, 48, 2),
    ];

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("闭合注释应被忽略");

    assert_eq!(passages[0].body.len(), 2);
    assert_eq!(passages[0].body[0].kind, BodyNodeKind::Text("之前"));
    assert_eq!(passages[0].body[1].kind, BodyNodeKind::Text("之后\n"));
}

#[test]
fn removes_multiline_comment_before_and_inside_passage() {
    let mut source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    source.content = String::from(
        "/%\n:: Hidden\n<<set $wrong = true>>\n%/\n:: Start\n正文\n/% ${$hidden}\n<<goto Hidden>> %/\n结束\n",
    );
    let tokens: Vec<Token<'_>> = lex(&source);

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("跨行注释应整体忽略");

    assert_eq!(passages.len(), 1);
    assert_eq!(passages[0].name, "Start");
    assert_eq!(passages[0].body.len(), 2);
    assert_eq!(passages[0].body[0].kind, BodyNodeKind::Text("正文\n"));
    assert_eq!(passages[0].body[1].kind, BodyNodeKind::Text("结束\n"));
}

#[test]
fn reports_unclosed_twee_comment() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(&source, TokenKind::Text("正文/% 未闭合"), 8, 28, 2),
    ];

    let error: ParseError<'_> = parse(&tokens).expect_err("未闭合注释必须报告错误");

    assert_eq!(error.kind, ParseErrorKind::UnclosedComment);
    assert_eq!(error.diagnostic().code, "twee.unclosed_comment");
    assert_eq!(error.span.line, 2);
    assert_eq!(error.span.column, 3);
}

#[test]
fn keeps_expression_like_text_literal() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(
            &source,
            TokenKind::Text("位置：${$LocationName}。\n"),
            8,
            37,
            2,
        ),
    ];
    let passages: Vec<Passage<'_>> = parse(&tokens).expect("字面正文应成功解析");

    assert_eq!(passages[0].body.len(), 1);
    assert_eq!(
        passages[0].body[0].kind,
        BodyNodeKind::Text("位置：${$LocationName}。\n")
    );
}

#[test]
fn keeps_nested_braces_as_literal_text() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(
            &source,
            TokenKind::Text("结果：${({ value: \"Map}\" }).value}。"),
            8,
            51,
            2,
        ),
    ];
    let passages: Vec<Passage<'_>> = parse(&tokens).expect("字面正文应保留嵌套花括号");
    assert_eq!(
        passages[0].body[0].kind,
        BodyNodeKind::Text("结果：${({ value: \"Map}\" }).value}。")
    );
}

#[test]
fn keeps_variable_link_arguments_in_link_macro() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(
            &source,
            TokenKind::Text("<<link [[$LocationName|$Location]]>><</link>>\n"),
            8,
            57,
            2,
        ),
    ];
    let passages: Vec<Passage<'_>> = parse(&tokens).expect("变量链接参数应成功保留");
    let BodyNodeKind::Macro(link) = &passages[0].body[0].kind else {
        panic!("变量链接仍应属于 link Macro");
    };

    assert_eq!(link.name, "link");
    assert_eq!(link.arguments, "[[$LocationName|$Location]]");
}

#[test]
fn keeps_unclosed_interpolation_like_text_literal() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(&source, TokenKind::Text("${$Location\n"), 8, 20, 2),
    ];
    let passages: Vec<Passage<'_>> = parse(&tokens).expect("普通正文不解析插值外壳");
    assert_eq!(
        passages[0].body[0].kind,
        BodyNodeKind::Text("${$Location\n")
    );
}

#[test]
fn groups_multiline_macro_body() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(&source, TokenKind::Text("<<widget greet>>\n"), 8, 25, 2),
        token(&source, TokenKind::Text("你好。\n"), 25, 35, 3),
        token(&source, TokenKind::Text("<</widget>>\n"), 35, 48, 4),
    ];
    let passages: Vec<Passage<'_>> = parse(&tokens).expect("跨行 Macro 应可解析");

    assert_eq!(
        passages[0].body,
        vec![BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name: "widget",
                arguments: "greet",
                arguments_span: crate::twee::Span {
                    start: 17,
                    end: 22,
                    line: 2,
                    column: 10,
                },
                syntax_kind: crate::twee::MacroSyntaxKind::Container,
                body: vec![BodyNode {
                    kind: BodyNodeKind::Text("你好。\n"),
                    span: crate::twee::Span {
                        start: 25,
                        end: 35,
                        line: 3,
                        column: 1,
                    },
                }],
            }),
            span: crate::twee::Span {
                start: 8,
                end: 48,
                line: 2,
                column: 1,
            },
        }]
    );
}

#[test]
fn groups_if_branch_clauses_without_individual_closings() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(&source, TokenKind::Text("<<if $first>>\n"), 8, 22, 2),
        token(&source, TokenKind::Text("第一段\n"), 22, 32, 3),
        token(&source, TokenKind::Text("<<elseif $second>>\n"), 32, 52, 4),
        token(&source, TokenKind::Text("第二段\n"), 52, 62, 5),
        token(&source, TokenKind::Text("<<else>>\n"), 62, 71, 6),
        token(&source, TokenKind::Text("最后一段\n"), 71, 84, 7),
        token(&source, TokenKind::Text("<</if>>\n"), 84, 92, 8),
    ];

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("if 子句不需要独立闭合符");
    let BodyNodeKind::Macro(if_node) = &passages[0].body[0].kind else {
        panic!("正文应包含 if Macro");
    };
    let BodyNodeKind::Macro(elseif_node) = &if_node.body[1].kind else {
        panic!("第二项应为 elseif 子句");
    };
    let BodyNodeKind::Macro(else_node) = &if_node.body[2].kind else {
        panic!("第三项应为 else 子句");
    };

    assert_eq!(if_node.name, "if");
    assert_eq!(if_node.body[0].kind, BodyNodeKind::Text("第一段\n"));
    assert_eq!(elseif_node.arguments, "$second");
    assert_eq!(elseif_node.body[0].kind, BodyNodeKind::Text("第二段\n"));
    assert_eq!(else_node.arguments, "");
    assert_eq!(else_node.body[0].kind, BodyNodeKind::Text("最后一段\n"));
}

#[test]
fn groups_switch_clauses_without_individual_closings() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(&source, TokenKind::Text("<<switch $value>>\n"), 8, 26, 2),
        token(&source, TokenKind::Text("<<case 1>>\n"), 26, 37, 3),
        token(&source, TokenKind::Text("第一项\n"), 37, 47, 4),
        token(&source, TokenKind::Text("<<case 2>>\n"), 47, 58, 5),
        token(&source, TokenKind::Text("第二项\n"), 58, 68, 6),
        token(&source, TokenKind::Text("<<default>>\n"), 68, 80, 7),
        token(&source, TokenKind::Text("默认项\n"), 80, 90, 8),
        token(&source, TokenKind::Text("<</switch>>\n"), 90, 103, 9),
    ];

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("switch 子句共用外层闭合符");
    let BodyNodeKind::Macro(switch) = &passages[0].body[0].kind else {
        panic!("正文应包含 switch Macro");
    };

    assert_eq!(switch.name, "switch");
    assert_eq!(switch.body.len(), 3);
    assert!(matches!(
        switch.body[0].kind,
        BodyNodeKind::Macro(MacroNode { name: "case", .. })
    ));
    assert!(matches!(
        switch.body[2].kind,
        BodyNodeKind::Macro(MacroNode {
            name: "default",
            ..
        })
    ));
}

#[test]
fn keeps_standalone_loop_control_macros_inline() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(&source, TokenKind::Text("<<break>>\n"), 8, 18, 2),
        token(&source, TokenKind::Text("<<continue>>\n"), 18, 31, 3),
    ];

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("循环控制 Macro 不需要闭合标签");

    assert!(matches!(
        passages[0].body[0].kind,
        BodyNodeKind::Macro(MacroNode { name: "break", .. })
    ));
    assert!(matches!(
        passages[0].body[1].kind,
        BodyNodeKind::Macro(MacroNode {
            name: "continue",
            ..
        })
    ));
}

#[test]
fn keeps_set_macro_inline() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = vec![
        token(
            &source,
            TokenKind::PassageDeclaration {
                name: "Start",
                tags: Vec::new(),
            },
            0,
            8,
            1,
        ),
        token(
            &source,
            TokenKind::Text("<<set $test to \"test\">>\n"),
            8,
            32,
            2,
        ),
    ];

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("set 不需要闭合标签");

    assert!(matches!(
        passages[0].body[0].kind,
        BodyNodeKind::Macro(MacroNode { name: "set", .. })
    ));
}

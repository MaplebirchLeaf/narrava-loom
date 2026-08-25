// twee.rs 测试分片 02：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn keeps_expression_action_macros_inline() {
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
        token(&source, TokenKind::Text("<<run $count++>>\n"), 8, 25, 2),
        token(&source, TokenKind::Text("<<include $next>>\n"), 25, 43, 3),
        token(&source, TokenKind::Text("<<goto 'End'>>\n"), 43, 58, 4),
    ];

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("动作 Macro 不需要闭合标签");
    let names: Vec<&str> = passages[0]
        .body
        .iter()
        .map(|node: &BodyNode<'_>| match &node.kind {
            BodyNodeKind::Macro(node) => node.name,
            _ => panic!("动作节点应为 Macro"),
        })
        .collect();

    assert_eq!(names, vec!["run", "include", "goto"]);
}

#[test]
fn keeps_unset_macro_inline() {
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
        token(&source, TokenKind::Text("<<unset $cache>>\n"), 8, 26, 2),
    ];

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("unset 不需要闭合标签");

    assert!(matches!(
        passages[0].body[0].kind,
        BodyNodeKind::Macro(MacroNode { name: "unset", .. })
    ));
}

#[test]
fn keeps_exit_macro_inline() {
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
        token(&source, TokenKind::Text("<<exit>>\n"), 8, 17, 2),
    ];

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("exit 不需要闭合标签");

    assert!(matches!(
        passages[0].body[0].kind,
        BodyNodeKind::Macro(MacroNode { name: "exit", .. })
    ));
}

#[test]
fn keeps_return_macro_inline_for_future_callable_scopes() {
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
        token(&source, TokenKind::Text("<<return $value>>\n"), 8, 26, 2),
    ];

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("return 是无闭合标签的保留语法");

    assert!(matches!(
        passages[0].body[0].kind,
        BodyNodeKind::Macro(MacroNode { name: "return", .. })
    ));
}

#[test]
fn dynamic_macro_shape_follows_its_explicit_closing_syntax() {
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
        token(&source, TokenKind::Text("<<inlineCard>>\n"), 8, 23, 2),
        token(&source, TokenKind::Text("<<panel>>\n"), 23, 33, 3),
        token(&source, TokenKind::Text("正文\n"), 33, 40, 4),
        token(&source, TokenKind::Text("<</panel>>\n"), 40, 52, 5),
    ];

    let passages: Vec<Passage<'_>> = parse(&tokens).expect("动态 Macro 应由源码形态分组");
    let BodyNodeKind::Macro(inline) = &passages[0].body[0].kind else {
        panic!("第一项应为 Inline Macro");
    };
    let BodyNodeKind::Macro(container) = &passages[0].body[1].kind else {
        panic!("第二项应为 Container Macro");
    };
    assert_eq!(inline.syntax_kind, crate::twee::MacroSyntaxKind::Inline);
    assert!(inline.body.is_empty());
    assert_eq!(
        container.syntax_kind,
        crate::twee::MacroSyntaxKind::Container
    );
    assert_eq!(container.body.len(), 1);
}

#[test]
fn groups_nested_macros_by_closing_level() {
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
        token(&source, TokenKind::Text("<<if $outer>>\n"), 8, 22, 2),
        token(&source, TokenKind::Text("<<if _inner>>\n"), 22, 36, 3),
        token(&source, TokenKind::Text("正文。\n"), 36, 46, 4),
        token(&source, TokenKind::Text("<</if>>\n"), 46, 55, 5),
        token(&source, TokenKind::Text("<</if>>\n"), 55, 64, 6),
    ];
    let passages: Vec<Passage<'_>> = parse(&tokens).expect("嵌套 Macro 应可解析");
    let BodyNodeKind::Macro(outer) = &passages[0].body[0].kind else {
        panic!("外层应为 Macro");
    };
    let BodyNodeKind::Macro(inner) = &outer.body[0].kind else {
        panic!("内层应为 Macro");
    };

    assert_eq!(passages[0].body.len(), 1);
    assert_eq!(outer.name, "if");
    assert_eq!(outer.arguments, "$outer");
    assert_eq!(passages[0].body[0].span.end, 64);
    assert_eq!(inner.name, "if");
    assert_eq!(inner.arguments, "_inner");
    assert_eq!(inner.body.len(), 1);
    assert_eq!(outer.body[0].span.end, 55);
    assert_eq!(inner.body[0].kind, BodyNodeKind::Text("正文。\n"));
}

#[test]
fn rejects_text_before_first_declaration() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = lex(&source);
    let result: Result<Vec<Passage<'_>>, ParseError<'_>> = parse(&tokens[1..]);
    let error: ParseError<'_> = result.expect_err("游离文本必须被拒绝");

    assert_eq!(error.kind, ParseErrorKind::TextBeforeDeclaration);
    assert_eq!(error.span.line, 2);
}

#[test]
fn rejects_empty_passage_name() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let tokens: Vec<Token<'_>> = vec![Token {
        source: &source.path,
        content: &source.content,
        kind: TokenKind::PassageDeclaration {
            name: "",
            tags: Vec::new(),
        },
        span: crate::twee::Span {
            start: 0,
            end: 3,
            line: 1,
            column: 1,
        },
    }];
    let result: Result<Vec<Passage<'_>>, ParseError<'_>> = parse(&tokens);
    let error: ParseError<'_> = result.expect_err("空名称必须被拒绝");

    assert_eq!(error.kind, ParseErrorKind::EmptyPassageName);
}

#[test]
fn rejects_unclosed_macro() {
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
        token(&source, TokenKind::Text("<<if $ready>>\n"), 8, 22, 2),
        token(&source, TokenKind::Text("正文。\n"), 22, 32, 3),
    ];
    let result: Result<Vec<Passage<'_>>, ParseError<'_>> = parse(&tokens);
    let error: ParseError<'_> = result.expect_err("未闭合 Macro 必须被拒绝");

    assert_eq!(error.kind, ParseErrorKind::UnclosedMacro { name: "if" });
    assert_eq!(error.span.line, 2);

    let diagnostic: Diagnostic = error.diagnostic();
    assert_eq!(diagnostic.code, "twee.unclosed_macro");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "Macro `if` 缺少闭合符");
    let location = diagnostic.location.expect("Twee 错误应保留源码位置");
    assert_eq!(location.source, "story/main.twee");
    assert_eq!(location.start, 8);
    assert_eq!(location.end, 22);
    assert_eq!(location.line, 2);
    assert_eq!(location.column, 1);
}

#[test]
fn rejects_mismatched_macro_closing() {
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
        token(&source, TokenKind::Text("<</if>>\n"), 25, 34, 3),
    ];
    let result: Result<Vec<Passage<'_>>, ParseError<'_>> = parse(&tokens);
    let error: ParseError<'_> = result.expect_err("错名闭合符必须被拒绝");

    assert_eq!(
        error.kind,
        ParseErrorKind::MismatchedMacroClosing {
            expected: "widget",
            found: "if",
        }
    );
    assert_eq!(error.span.line, 3);

    let diagnostic: Diagnostic = error.diagnostic();
    assert_eq!(diagnostic.code, "twee.mismatched_macro_closing");
    assert_eq!(
        diagnostic.message,
        "Macro 闭合名称不匹配，预期 `widget`，实际 `if`"
    );
}

#[test]
fn rejects_unexpected_macro_closing() {
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
        token(&source, TokenKind::Text("<</if>>\n"), 8, 17, 2),
    ];
    let result: Result<Vec<Passage<'_>>, ParseError<'_>> = parse(&tokens);
    let error: ParseError<'_> = result.expect_err("孤立闭合符必须被拒绝");

    assert_eq!(
        error.kind,
        ParseErrorKind::UnexpectedMacroClosing { name: "if" }
    );
    assert_eq!(error.span.line, 2);
}

#[test]
fn rejects_duplicate_passage_name() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let passages: Vec<Passage<'_>> =
        vec![passage(&source, "Start", 1), passage(&source, "Start", 4)];
    let result: Result<(), SemanticError<'_>> = validate(&passages);
    let error: SemanticError<'_> = result.expect_err("重复名称必须被拒绝");

    assert_eq!(error.kind, SemanticErrorKind::DuplicatePassageName);
    assert_eq!(error.span.line, 4);

    let diagnostic: Diagnostic = error.diagnostic();
    assert_eq!(diagnostic.code, "twee.duplicate_passage_name");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "Passage 名称 `Start` 重复");
    let location = diagnostic.location.expect("语义错误应保留声明位置");
    assert_eq!(location.source, "story/main.twee");
    assert_eq!(location.line, 4);
    assert_eq!(location.column, 1);
}

#[test]
fn passage_name_comparison_is_case_sensitive() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let passages: Vec<Passage<'_>> =
        vec![passage(&source, "Start", 1), passage(&source, "start", 4)];
    let result: Result<(), SemanticError<'_>> = validate(&passages);

    assert!(result.is_ok());
}

#[test]
fn rejects_tags_on_every_special_passage() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");

    for name in ["Start", "StoryInit", "Header", "Footer", "Bar", "BarStowed"] {
        let mut special: Passage<'_> = passage(&source, name, 1);
        special.tags = vec!["invalid"];
        let error: SemanticError<'_> = validate(&[special]).expect_err("特殊 Passage 必须拒绝 Tag");

        assert_eq!(error.kind, SemanticErrorKind::SpecialPassageTags);
        assert_eq!(error.name, name);
        assert_eq!(error.diagnostic().code, "twee.special_passage_tags");
    }
}

#[test]
fn ordinary_passage_still_accepts_tags() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例源码应可读取");
    let mut ordinary: Passage<'_> = passage(&source, "Hall", 1);
    ordinary.tags = vec!["hub", "indoor"];

    assert!(validate(&[ordinary]).is_ok());
}

#[test]
fn builds_story_from_twee_sources() {
    let sources: SourceList =
        SourceList::discover(Path::new("src/tests/fixtures/game")).expect("示例项目源码应可发现");
    let story: Story<'_> = Story::build(&sources.items).expect("示例源码应组成 Story");

    assert_eq!(story.passages.len(), 1);
    assert_eq!(story.passages[0].name, "Start");
}

#[test]
fn finds_start_passage_with_case_sensitive_name() {
    let sources: SourceList =
        SourceList::discover(Path::new("src/tests/fixtures/game")).expect("示例项目源码应可发现");
    let story: Story<'_> = Story::build(&sources.items).expect("示例源码应组成 Story");

    assert!(story.passage("Start").is_some());
    assert!(story.passage("start").is_none());
}

#[test]
fn rejects_duplicate_names_across_sources() {
    let first: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Twee 应可读取");
    let second: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("scripts/main.ts"),
    )
    .expect("第二份示例 Source 应可读取");
    let passages: Vec<Passage<'_>> =
        vec![passage(&first, "Start", 1), passage(&second, "Start", 1)];
    let result: Result<Story<'_>, StoryError<'_>> = Story::from_passages(passages);
    let error: StoryError<'_> = result.expect_err("跨 Source 重复名称必须被拒绝");

    assert!(matches!(
        &error,
        StoryError::Semantic(SemanticError {
            kind: SemanticErrorKind::DuplicatePassageName,
            ..
        })
    ));
    assert_eq!(error.diagnostic().code, "twee.duplicate_passage_name");
}

fn passage<'source>(source: &'source Source, name: &'source str, line: usize) -> Passage<'source> {
    Passage {
        source: &source.path,
        content: &source.content,
        name,
        tags: Vec::new(),
        body: Vec::new(),
        span: crate::twee::Span {
            start: 0,
            end: 0,
            line,
            column: 1,
        },
    }
}

fn token<'source>(
    source: &'source Source,
    kind: TokenKind<'source>,
    start: usize,
    end: usize,
    line: usize,
) -> Token<'source> {
    Token {
        source: &source.path,
        content: &source.content,
        kind,
        span: crate::twee::Span {
            start,
            end,
            line,
            column: 1,
        },
    }
}

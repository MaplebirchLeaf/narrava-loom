// expression.rs 测试分片 01：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn owned_expression_json_round_trip_rebuilds_the_same_ast() {
    let parsed = parse("setup.items[1]?.name ?? ($score >= 3 ? 'ready' : 'wait')")
        .expect("复合表达式应可解析");
    let owned = OwnedExpression::from(&parsed);

    let json = serde_json::to_string(&owned).expect("拥有型表达式应可序列化");
    let decoded: OwnedExpression = serde_json::from_str(&json).expect("拥有型表达式应可反序列化");
    let rebuilt = decoded.as_expression();

    assert_eq!(rebuilt, parsed);
}

#[test]
fn lexes_state_and_macro_variables() {
    let tokens: Vec<Token<'_>> = lex("$score _target @item").expect("变量应成功解析");

    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Variable {
                    scope: VariableScope::Variables,
                    name: "score",
                },
                span: Span { start: 0, end: 6 },
            },
            Token {
                kind: TokenKind::Variable {
                    scope: VariableScope::Temporary,
                    name: "target",
                },
                span: Span { start: 7, end: 14 },
            },
            Token {
                kind: TokenKind::Variable {
                    scope: VariableScope::Local,
                    name: "item",
                },
                span: Span { start: 15, end: 20 },
            },
        ]
    );
}

#[test]
fn rejects_unknown_characters_and_empty_variable_names() {
    assert_eq!(
        lex("1 # 2"),
        Err(LexError::UnexpectedCharacter(Span { start: 2, end: 3 }))
    );
    assert_eq!(
        lex("@"),
        Err(LexError::InvalidVariable(Span { start: 0, end: 1 }))
    );
}

#[test]
fn converts_lexer_errors_without_inventing_source_location() {
    let local_span: Span = Span { start: 2, end: 3 };
    let unexpected: Diagnostic = LexError::UnexpectedCharacter(local_span).diagnostic();
    let variable: Diagnostic = LexError::InvalidVariable(Span { start: 0, end: 1 }).diagnostic();
    let string: Diagnostic = LexError::UnclosedString(Span { start: 0, end: 7 }).diagnostic();

    assert_eq!(unexpected.code, "expression.unexpected_character");
    assert_eq!(unexpected.severity, DiagnosticSeverity::Error);
    assert_eq!(unexpected.message, "Expression 包含无法识别的字符");
    assert_eq!(unexpected.location, None);
    assert_eq!(LexError::UnexpectedCharacter(local_span).span(), local_span);
    assert_eq!(variable.code, "expression.invalid_variable");
    assert_eq!(string.code, "expression.unclosed_string");
}

#[test]
fn converts_parser_errors_to_stable_diagnostics() {
    let local_span: Span = Span { start: 0, end: 1 };
    let assignment: Diagnostic = ParseError::InvalidAssignmentTarget(local_span).diagnostic();
    let group: Diagnostic = ParseError::UnclosedGroup(Span { start: 0, end: 1 }).diagnostic();
    let expected: Diagnostic = ParseError::ExpectedExpression.diagnostic();
    let lexed: Diagnostic =
        ParseError::Lex(LexError::UnclosedString(Span { start: 0, end: 4 })).diagnostic();

    assert_eq!(assignment.code, "expression.invalid_assignment_target");
    assert_eq!(assignment.message, "Expression 不是可赋值目标");
    assert_eq!(assignment.location, None);
    assert_eq!(
        ParseError::InvalidAssignmentTarget(local_span).span(),
        Some(local_span)
    );
    assert_eq!(ParseError::ExpectedExpression.span(), None);
    assert_eq!(group.code, "expression.unclosed_group");
    assert_eq!(expected.code, "expression.expected_expression");
    assert_eq!(lexed.code, "expression.unclosed_string");
}

#[test]
fn lexes_integer_and_decimal_numbers() {
    let tokens: Vec<Token<'_>> = lex("12 3.5").expect("数字应成功解析");

    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Number("12"),
                span: Span { start: 0, end: 2 },
            },
            Token {
                kind: TokenKind::Number("3.5"),
                span: Span { start: 3, end: 6 },
            },
        ]
    );
}

#[test]
fn lexes_single_and_double_quoted_strings() {
    let tokens: Vec<Token<'_>> = lex(r#"'forest' "say \"hi\"""#).expect("字符串应成功解析");

    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::String("forest"),
                span: Span { start: 0, end: 8 },
            },
            Token {
                kind: TokenKind::String(r#"say \"hi\""#),
                span: Span { start: 9, end: 21 },
            },
        ]
    );
}

#[test]
fn rejects_unclosed_string() {
    let error: LexError = lex("'forest").expect_err("未闭合字符串应报错");

    assert_eq!(error, LexError::UnclosedString(Span { start: 0, end: 7 }));
}

#[test]
fn lexes_value_keywords_without_matching_identifier_prefixes() {
    let tokens: Vec<Token<'_>> =
        lex("true false null undefined trueValue").expect("值关键字应成功解析");

    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Boolean(true),
                span: Span { start: 0, end: 4 },
            },
            Token {
                kind: TokenKind::Boolean(false),
                span: Span { start: 5, end: 10 },
            },
            Token {
                kind: TokenKind::Null,
                span: Span { start: 11, end: 15 },
            },
            Token {
                kind: TokenKind::Undefined,
                span: Span { start: 16, end: 25 },
            },
            Token {
                kind: TokenKind::Identifier("trueValue"),
                span: Span { start: 26, end: 35 },
            },
        ]
    );
}

#[test]
fn lexes_literal_punctuation_with_individual_spans() {
    let tokens: Vec<Token<'_>> = lex("()[]{},:").expect("基础标点应成功解析");

    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::LeftParen,
                span: Span { start: 0, end: 1 }
            },
            Token {
                kind: TokenKind::RightParen,
                span: Span { start: 1, end: 2 }
            },
            Token {
                kind: TokenKind::LeftBracket,
                span: Span { start: 2, end: 3 }
            },
            Token {
                kind: TokenKind::RightBracket,
                span: Span { start: 3, end: 4 }
            },
            Token {
                kind: TokenKind::LeftBrace,
                span: Span { start: 4, end: 5 }
            },
            Token {
                kind: TokenKind::RightBrace,
                span: Span { start: 5, end: 6 }
            },
            Token {
                kind: TokenKind::Comma,
                span: Span { start: 6, end: 7 }
            },
            Token {
                kind: TokenKind::Colon,
                span: Span { start: 7, end: 8 }
            },
        ]
    );
}

#[test]
fn parses_single_basic_value() {
    let expression: Expression<'_> = parse("3.5").expect("数字值应成功解析");

    assert_eq!(
        expression,
        Expression {
            kind: ExpressionKind::Number("3.5"),
            span: Span { start: 0, end: 3 },
        }
    );
}

#[test]
fn parses_single_variable_reference() {
    let expression: Expression<'_> = parse("@item").expect("变量引用应成功解析");

    assert_eq!(
        expression,
        Expression {
            kind: ExpressionKind::Variable {
                scope: VariableScope::Local,
                name: "item",
            },
            span: Span { start: 0, end: 5 },
        }
    );
}

#[test]
fn parses_global_name_and_setup_root() {
    let global: Expression<'_> = parse("hero").expect("global 名称应成功解析");
    let setup: Expression<'_> = parse("setup").expect("setup 根节点应成功解析");

    assert_eq!(
        global,
        Expression {
            kind: ExpressionKind::Global("hero"),
            span: Span { start: 0, end: 4 },
        }
    );
    assert_eq!(
        setup,
        Expression {
            kind: ExpressionKind::Setup,
            span: Span { start: 0, end: 5 },
        }
    );
}

#[test]
fn parses_member_access_with_property_span() {
    let expression: Expression<'_> = parse("setup.player").expect("成员访问应成功解析");

    assert_eq!(
        expression,
        Expression {
            kind: ExpressionKind::Member {
                target: Box::new(Expression {
                    kind: ExpressionKind::Setup,
                    span: Span { start: 0, end: 5 },
                }),
                property: "player",
                property_span: Span { start: 6, end: 12 },
            },
            span: Span { start: 0, end: 12 },
        }
    );
}

#[test]
fn rejects_member_access_without_property() {
    let error: ParseError = parse("setup.").expect_err("缺少成员名应报错");

    assert_eq!(
        error,
        ParseError::ExpectedMemberName(Span { start: 5, end: 6 })
    );
}

#[test]
fn parses_index_access_with_expression_key() {
    let expression: Expression<'_> = parse("hero[@index]").expect("索引访问应成功解析");

    assert_eq!(
        expression,
        Expression {
            kind: ExpressionKind::Index {
                target: Box::new(Expression {
                    kind: ExpressionKind::Global("hero"),
                    span: Span { start: 0, end: 4 },
                }),
                index: Box::new(Expression {
                    kind: ExpressionKind::Variable {
                        scope: VariableScope::Local,
                        name: "index",
                    },
                    span: Span { start: 5, end: 11 },
                }),
            },
            span: Span { start: 0, end: 12 },
        }
    );
}

#[test]
fn rejects_unclosed_index_access() {
    let error: ParseError = parse("hero[@index").expect_err("未闭合索引访问应报错");

    assert_eq!(error, ParseError::UnclosedIndex(Span { start: 4, end: 5 }));
}

#[test]
fn parses_empty_and_populated_calls() {
    let empty: Expression<'_> = parse("random()").expect("空参数调用应成功解析");
    let populated: Expression<'_> = parse("random(1,@max)").expect("调用参数应成功解析");

    assert_eq!(
        empty,
        Expression {
            kind: ExpressionKind::Call {
                callee: Box::new(Expression {
                    kind: ExpressionKind::Global("random"),
                    span: Span { start: 0, end: 6 },
                }),
                arguments: Vec::new(),
            },
            span: Span { start: 0, end: 8 },
        }
    );
    assert_eq!(
        populated,
        Expression {
            kind: ExpressionKind::Call {
                callee: Box::new(Expression {
                    kind: ExpressionKind::Global("random"),
                    span: Span { start: 0, end: 6 },
                }),
                arguments: vec![
                    Expression {
                        kind: ExpressionKind::Number("1"),
                        span: Span { start: 7, end: 8 },
                    },
                    Expression {
                        kind: ExpressionKind::Variable {
                            scope: VariableScope::Local,
                            name: "max",
                        },
                        span: Span { start: 9, end: 13 },
                    },
                ],
            },
            span: Span { start: 0, end: 14 },
        }
    );
}

#[test]
fn parses_whitespace_separated_expression_list() {
    let expressions: Vec<Expression<'_>> =
        parse_list("'Author' ($count + 1) { active: true } max(1, 2)")
            .expect("Macro 实参列表应可解析");

    assert_eq!(expressions.len(), 4);
    assert!(matches!(
        expressions[0].kind,
        ExpressionKind::String("Author")
    ));
    assert!(matches!(expressions[1].kind, ExpressionKind::Group(_)));
    assert!(matches!(expressions[2].kind, ExpressionKind::Object(_)));
    assert!(matches!(expressions[3].kind, ExpressionKind::Call { .. }));
}

#[test]
fn parses_empty_expression_list() {
    let expressions: Vec<Expression<'_>> = parse_list("").expect("空实参列表应有效");

    assert!(expressions.is_empty());
}

#[test]
fn rejects_commas_between_macro_arguments() {
    let error: ParseError = parse_list("1, 2").expect_err("Macro 实参之间不使用逗号");

    assert!(matches!(error, ParseError::UnexpectedToken(_)));
}

#[test]
fn rejects_unclosed_call() {
    let error: ParseError = parse("random(@max").expect_err("未闭合调用应报错");

    assert_eq!(error, ParseError::UnclosedCall(Span { start: 6, end: 7 }));
}

#[test]
fn parses_optional_member_index_and_call() {
    let member: Expression<'_> = parse("hero?.profile").expect("可选成员应成功解析");
    let index: Expression<'_> = parse("hero?.[@key]").expect("可选索引应成功解析");
    let call: Expression<'_> = parse("hero?.(@arg)").expect("可选调用应成功解析");

    assert!(matches!(
        &member.kind,
        ExpressionKind::OptionalMember {
            property: "profile",
            ..
        }
    ));
    assert_eq!(member.span, Span { start: 0, end: 13 });

    assert!(matches!(&index.kind, ExpressionKind::OptionalIndex { .. }));
    assert_eq!(index.span, Span { start: 0, end: 12 });

    assert!(matches!(
        &call.kind,
        ExpressionKind::OptionalCall { arguments, .. } if arguments.len() == 1
    ));
    assert_eq!(call.span, Span { start: 0, end: 12 });
}

#[test]
fn classifies_assignable_targets() {
    for source in [
        "$score",
        "_temporary",
        "@local",
        "hero",
        "setup.value",
        "hero[@key]",
        "($score)",
    ] {
        let expression: Expression<'_> = parse(source).expect("可赋值示例应成功解析");
        assert!(expression.is_assignable_target(), "{source} 应可赋值");
    }

    for source in [
        "setup",
        "1",
        "random()",
        "hero?.profile",
        "hero?.profile.name",
    ] {
        let expression: Expression<'_> = parse(source).expect("不可赋值示例应成功解析");
        assert!(!expression.is_assignable_target(), "{source} 不应可赋值");
    }
}

#[test]
fn parses_non_mutating_unary_operators() {
    let cases: [(&str, UnaryOperator, Span); 6] = [
        (
            "!@ready",
            UnaryOperator::LogicalNot,
            Span { start: 0, end: 7 },
        ),
        (
            "not @ready",
            UnaryOperator::LogicalNot,
            Span { start: 0, end: 10 },
        ),
        (
            "~@bits",
            UnaryOperator::BitwiseNot,
            Span { start: 0, end: 6 },
        ),
        (
            "+@value",
            UnaryOperator::Positive,
            Span { start: 0, end: 7 },
        ),
        (
            "-hero.value",
            UnaryOperator::Negative,
            Span { start: 0, end: 11 },
        ),
        (
            "typeof @value",
            UnaryOperator::TypeOf,
            Span { start: 0, end: 13 },
        ),
    ];

    for (source, expected, span) in cases {
        let expression: Expression<'_> = parse(source).expect("一元表达式应成功解析");
        let ExpressionKind::Unary { operator, operand } = &expression.kind else {
            panic!("{source} 应生成 Unary AST");
        };

        assert_eq!(*operator, expected);
        assert_eq!(expression.span, span);
        if source == "-hero.value" {
            assert!(matches!(operand.kind, ExpressionKind::Member { .. }));
        }
    }
}

#[test]
fn parses_prefix_and_postfix_updates() {
    let cases: [(&str, UpdateOperator, UpdatePosition, Span); 4] = [
        (
            "++$score",
            UpdateOperator::Increment,
            UpdatePosition::Prefix,
            Span { start: 0, end: 8 },
        ),
        (
            "--setup.value",
            UpdateOperator::Decrement,
            UpdatePosition::Prefix,
            Span { start: 0, end: 13 },
        ),
        (
            "$score++",
            UpdateOperator::Increment,
            UpdatePosition::Postfix,
            Span { start: 0, end: 8 },
        ),
        (
            "setup.value--",
            UpdateOperator::Decrement,
            UpdatePosition::Postfix,
            Span { start: 0, end: 13 },
        ),
    ];

    for (source, expected_operator, expected_position, span) in cases {
        let expression: Expression<'_> = parse(source).expect("更新表达式应成功解析");
        let ExpressionKind::Update {
            operator,
            position,
            target,
        } = &expression.kind
        else {
            panic!("{source} 应生成 Update AST");
        };

        assert_eq!(*operator, expected_operator);
        assert_eq!(*position, expected_position);
        assert!(target.is_assignable_target());
        assert_eq!(expression.span, span);
    }
}

#[test]
fn rejects_updates_for_non_assignable_targets() {
    let prefix: ParseError = parse("++1").expect_err("字面量不能自增");
    let postfix: ParseError = parse("hero?.name++").expect_err("可选链不能自增");

    assert_eq!(
        prefix,
        ParseError::InvalidAssignmentTarget(Span { start: 2, end: 3 })
    );
    assert_eq!(
        postfix,
        ParseError::InvalidAssignmentTarget(Span { start: 0, end: 10 })
    );
}

#[test]
fn parses_power_as_right_associative_after_unary() {
    let power: Expression<'_> = parse("2 ** 3 ** 2").expect("幂运算应成功解析");
    let unary_left: Expression<'_> = parse("-2 ** 3").expect("一元操作数应成功解析");

    let ExpressionKind::Binary {
        operator,
        left,
        right,
    } = &power.kind
    else {
        panic!("应生成 Binary AST");
    };
    assert_eq!(*operator, BinaryOperator::Power);
    assert!(matches!(left.kind, ExpressionKind::Number("2")));
    assert!(matches!(
        right.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::Power,
            ..
        }
    ));
    assert_eq!(power.span, Span { start: 0, end: 11 });

    let ExpressionKind::Binary { left, .. } = &unary_left.kind else {
        panic!("一元表达式应成为幂运算左操作数");
    };
    assert!(matches!(left.kind, ExpressionKind::Unary { .. }));
}

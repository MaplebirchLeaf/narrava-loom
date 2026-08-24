// expression.rs 测试分片 02：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn parses_multiplicative_operators_with_power_precedence() {
    let cases: [(&str, BinaryOperator); 4] = [
        ("8 * 2", BinaryOperator::Multiply),
        ("8 / 2", BinaryOperator::Divide),
        ("8 // 2", BinaryOperator::IntegerDivide),
        ("8 % 2", BinaryOperator::Remainder),
    ];
    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("乘法层表达式应成功解析");
        assert!(matches!(
            expression.kind,
            ExpressionKind::Binary { operator, .. } if operator == expected
        ));
    }

    let left_associative: Expression<'_> = parse("8 / 4 * 2").expect("左结合应成功解析");
    let power_first: Expression<'_> = parse("2 * 3 ** 2").expect("幂优先级应成功解析");
    assert!(matches!(
        left_associative.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::Multiply,
            left,
            ..
        } if matches!(left.kind, ExpressionKind::Binary {
            operator: BinaryOperator::Divide,
            ..
        })
    ));
    assert!(matches!(
        power_first.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::Multiply,
            right,
            ..
        } if matches!(right.kind, ExpressionKind::Binary {
            operator: BinaryOperator::Power,
            ..
        })
    ));
}

#[test]
fn parses_additive_operators_with_multiplicative_precedence() {
    let left_associative: Expression<'_> = parse("10 - 3 + 1").expect("加减应成功解析");
    let multiply_first: Expression<'_> = parse("2 + 3 * 4").expect("乘法优先级应成功解析");

    assert!(matches!(
        left_associative.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::Add,
            left,
            ..
        } if matches!(left.kind, ExpressionKind::Binary {
            operator: BinaryOperator::Subtract,
            ..
        })
    ));
    assert!(matches!(
        multiply_first.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::Add,
            right,
            ..
        } if matches!(right.kind, ExpressionKind::Binary {
            operator: BinaryOperator::Multiply,
            ..
        })
    ));
}

#[test]
fn parses_shift_operators_with_additive_precedence() {
    let cases: [(&str, BinaryOperator); 3] = [
        ("8 << 1", BinaryOperator::ShiftLeft),
        ("8 >> 1", BinaryOperator::ShiftRight),
        ("8 >>> 1", BinaryOperator::UnsignedShiftRight),
    ];
    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("位移表达式应成功解析");
        assert!(matches!(
            expression.kind,
            ExpressionKind::Binary { operator, .. } if operator == expected
        ));
    }

    let left_associative: Expression<'_> = parse("16 >> 1 >>> 1").expect("位移应左结合");
    let addition_first: Expression<'_> = parse("1 << 2 + 1").expect("加法应优先解析");
    assert!(matches!(
        left_associative.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::UnsignedShiftRight,
            left,
            ..
        } if matches!(left.kind, ExpressionKind::Binary {
            operator: BinaryOperator::ShiftRight,
            ..
        })
    ));
    assert!(matches!(
        addition_first.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::ShiftLeft,
            right,
            ..
        } if matches!(right.kind, ExpressionKind::Binary {
            operator: BinaryOperator::Add,
            ..
        })
    ));
}

#[test]
fn parses_comparisons_aliases_and_three_way_operator() {
    let cases: [(&str, BinaryOperator); 9] = [
        ("1 < 2", BinaryOperator::Less),
        ("1 <= 2", BinaryOperator::LessEqual),
        ("1 > 2", BinaryOperator::Greater),
        ("1 >= 2", BinaryOperator::GreaterEqual),
        ("1 <=> 2", BinaryOperator::ThreeWayCompare),
        ("1 lt 2", BinaryOperator::Less),
        ("1 lte 2", BinaryOperator::LessEqual),
        ("1 gt 2", BinaryOperator::Greater),
        ("1 gte 2", BinaryOperator::GreaterEqual),
    ];
    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("比较表达式应成功解析");
        assert!(matches!(
            expression.kind,
            ExpressionKind::Binary { operator, .. } if operator == expected
        ));
    }

    let chained: Expression<'_> = parse("1 < 2 < 3").expect("连续比较应左结合");
    let addition_first: Expression<'_> = parse("1 + 2 >= 3").expect("加法应优先解析");
    assert!(matches!(
        chained.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::Less,
            left,
            ..
        } if matches!(left.kind, ExpressionKind::Binary {
            operator: BinaryOperator::Less,
            ..
        })
    ));
    assert!(matches!(
        addition_first.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::GreaterEqual,
            left,
            ..
        } if matches!(left.kind, ExpressionKind::Binary {
            operator: BinaryOperator::Add,
            ..
        })
    ));
}

#[test]
fn parses_keyword_comparison_operators() {
    let cases: [(&str, BinaryOperator); 3] = [
        ("hero instanceof Player", BinaryOperator::InstanceOf),
        ("@key in hero", BinaryOperator::In),
        ("@key notin hero", BinaryOperator::NotIn),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("关键字比较应成功解析");
        assert!(matches!(
            expression.kind,
            ExpressionKind::Binary { operator, .. } if operator == expected
        ));
    }
}

#[test]
fn parses_between_with_four_boundary_forms() {
    let cases: [(&str, BetweenBounds); 4] = [
        ("@value between() 1 10", BetweenBounds::OpenOpen),
        ("@value between(] 1 10", BetweenBounds::OpenClosed),
        ("@value between[) 1 10", BetweenBounds::ClosedOpen),
        ("@value between[] 1 10", BetweenBounds::ClosedClosed),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("区间判断应成功解析");
        assert!(matches!(
            expression.kind,
            ExpressionKind::Between {
                bounds,
                value,
                lower,
                upper,
            } if bounds == expected
                && matches!(value.kind, ExpressionKind::Variable { name: "value", .. })
                && matches!(lower.kind, ExpressionKind::Number("1"))
                && matches!(upper.kind, ExpressionKind::Number("10"))
        ));
    }
}

#[test]
fn parses_equality_operators_and_aliases() {
    let cases: [(&str, BinaryOperator); 7] = [
        ("1 == '1'", BinaryOperator::Equal),
        ("1 != '1'", BinaryOperator::NotEqual),
        ("1 === 1", BinaryOperator::StrictEqual),
        ("1 !== '1'", BinaryOperator::StrictNotEqual),
        ("1 equ '1'", BinaryOperator::Equal),
        ("1 is 1", BinaryOperator::StrictEqual),
        ("1 isnot '1'", BinaryOperator::StrictNotEqual),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("相等表达式应成功解析");
        assert!(matches!(
            expression.kind,
            ExpressionKind::Binary { operator, .. } if operator == expected
        ));
    }

    let comparison_first: Expression<'_> = parse("1 < 2 === true").expect("比较应优先解析");
    assert!(matches!(
        comparison_first.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::StrictEqual,
            left,
            ..
        } if matches!(left.kind, ExpressionKind::Binary {
            operator: BinaryOperator::Less,
            ..
        })
    ));
}

#[test]
fn parses_bitwise_operators_with_distinct_precedence() {
    let expression: Expression<'_> = parse("1 | 2 ^ 3 & 4 == 4").expect("按位表达式应成功解析");
    let ExpressionKind::Binary {
        operator: BinaryOperator::BitwiseOr,
        right: xor,
        ..
    } = expression.kind
    else {
        panic!("最外层应为按位或");
    };
    let ExpressionKind::Binary {
        operator: BinaryOperator::BitwiseXor,
        right: and,
        ..
    } = xor.kind
    else {
        panic!("按位异或应高于按位或");
    };
    let ExpressionKind::Binary {
        operator: BinaryOperator::BitwiseAnd,
        right: equality,
        ..
    } = and.kind
    else {
        panic!("按位与应高于按位异或");
    };
    assert!(matches!(
        equality.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::Equal,
            ..
        }
    ));
}

#[test]
fn parses_logical_operators_and_aliases() {
    let cases: [(&str, BinaryOperator); 4] = [
        ("true && false", BinaryOperator::LogicalAnd),
        ("true and false", BinaryOperator::LogicalAnd),
        ("true || false", BinaryOperator::LogicalOr),
        ("true or false", BinaryOperator::LogicalOr),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("逻辑表达式应成功解析");
        assert!(matches!(
            expression.kind,
            ExpressionKind::Binary { operator, .. } if operator == expected
        ));
    }

    let expression: Expression<'_> = parse("true || false && 1 | 2").expect("逻辑与应高于逻辑或");
    let ExpressionKind::Binary {
        operator: BinaryOperator::LogicalOr,
        right: logical_and,
        ..
    } = expression.kind
    else {
        panic!("最外层应为逻辑或");
    };
    let ExpressionKind::Binary {
        operator: BinaryOperator::LogicalAnd,
        right: bitwise_or,
        ..
    } = logical_and.kind
    else {
        panic!("逻辑与应高于逻辑或");
    };
    assert!(matches!(
        bitwise_or.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::BitwiseOr,
            ..
        }
    ));
}

#[test]
fn parses_nullish_coalescing_and_grouped_logical_operands() {
    let chained: Expression<'_> =
        parse("null ?? undefined ?? 'fallback'").expect("空值合并链应成功解析");
    assert!(matches!(
        chained.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::NullishCoalesce,
            left,
            ..
        } if matches!(left.kind, ExpressionKind::Binary {
            operator: BinaryOperator::NullishCoalesce,
            ..
        })
    ));

    parse("(null ?? false) || true").expect("括号应允许空值合并与逻辑或组合");
    parse("null ?? (false && true)").expect("括号应允许空值合并与逻辑与组合");
}

#[test]
fn rejects_unparenthesized_nullish_and_logical_mixing() {
    let cases: [&str; 4] = [
        "null ?? false || true",
        "null ?? false && true",
        "false || null ?? true",
        "false && null ?? true",
    ];

    for source in cases {
        let error: ParseError = parse(source).expect_err("无括号混用应报错");
        assert!(matches!(error, ParseError::MixedNullishLogical(_)));
    }
}

#[test]
fn parses_conditional_expression_as_right_associative() {
    let expression: Expression<'_> =
        parse("@ready ? 'yes' : @fallback ? 'later' : 'no'").expect("三目条件应成功解析");

    let ExpressionKind::Conditional {
        condition,
        consequent,
        alternate,
    } = expression.kind
    else {
        panic!("最外层应为条件表达式");
    };
    assert!(matches!(
        condition.kind,
        ExpressionKind::Variable { name: "ready", .. }
    ));
    assert!(matches!(consequent.kind, ExpressionKind::String("yes")));
    assert!(matches!(alternate.kind, ExpressionKind::Conditional { .. }));
}

#[test]
fn rejects_conditional_without_colon() {
    let error: ParseError = parse("@ready ? 'yes'").expect_err("缺少冒号应报错");

    assert_eq!(error, ParseError::ExpectedColon(Span { start: 7, end: 8 }));
}

#[test]
fn parses_basic_assignment_as_right_associative() {
    let expression: Expression<'_> = parse("@left = $right = 1").expect("赋值链应成功解析");
    let ExpressionKind::Assignment {
        operator: AssignmentOperator::Assign,
        target,
        value,
    } = expression.kind
    else {
        panic!("最外层应为赋值表达式");
    };
    assert!(matches!(
        target.kind,
        ExpressionKind::Variable {
            scope: VariableScope::Local,
            name: "left",
        }
    ));
    assert!(matches!(
        value.kind,
        ExpressionKind::Assignment {
            operator: AssignmentOperator::Assign,
            ..
        }
    ));
}

#[test]
fn rejects_assignment_to_non_assignable_expression() {
    let error: ParseError = parse("1 = 2").expect_err("字面量不可作为赋值目标");
    let compound_error: ParseError = parse("1 += 2").expect_err("复合赋值也必须检查目标");

    assert_eq!(
        error,
        ParseError::InvalidAssignmentTarget(Span { start: 0, end: 1 })
    );
    assert_eq!(compound_error, error);
}

#[test]
fn parses_arithmetic_compound_assignments() {
    let cases: [(&str, AssignmentOperator); 7] = [
        ("@value += 1", AssignmentOperator::Add),
        ("@value -= 1", AssignmentOperator::Subtract),
        ("@value *= 2", AssignmentOperator::Multiply),
        ("@value /= 2", AssignmentOperator::Divide),
        ("@value //= 2", AssignmentOperator::IntegerDivide),
        ("@value %= 2", AssignmentOperator::Remainder),
        ("@value **= 2", AssignmentOperator::Power),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("复合赋值应成功解析");
        assert!(matches!(
            expression.kind,
            ExpressionKind::Assignment { operator, .. } if operator == expected
        ));
    }
}

#[test]
fn parses_shift_and_bitwise_compound_assignments() {
    let cases: [(&str, AssignmentOperator); 6] = [
        ("@value <<= 1", AssignmentOperator::ShiftLeft),
        ("@value >>= 1", AssignmentOperator::ShiftRight),
        ("@value >>>= 1", AssignmentOperator::UnsignedShiftRight),
        ("@value &= 1", AssignmentOperator::BitwiseAnd),
        ("@value ^= 1", AssignmentOperator::BitwiseXor),
        ("@value |= 1", AssignmentOperator::BitwiseOr),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("位运算复合赋值应成功解析");
        assert!(matches!(
            expression.kind,
            ExpressionKind::Assignment { operator, .. } if operator == expected
        ));
    }
}

#[test]
fn parses_short_circuit_compound_assignments() {
    let cases: [(&str, AssignmentOperator); 3] = [
        ("@value &&= next", AssignmentOperator::LogicalAnd),
        ("@value ||= fallback", AssignmentOperator::LogicalOr),
        ("@value ??= fallback", AssignmentOperator::NullishCoalesce),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("短路复合赋值应成功解析");
        assert!(matches!(
            expression.kind,
            ExpressionKind::Assignment { operator, .. } if operator == expected
        ));
    }
}

#[test]
fn parses_grouped_expression_with_outer_span() {
    let expression: Expression<'_> = parse("(@item)").expect("分组表达式应成功解析");

    assert_eq!(
        expression,
        Expression {
            kind: ExpressionKind::Group(Box::new(Expression {
                kind: ExpressionKind::Variable {
                    scope: VariableScope::Local,
                    name: "item",
                },
                span: Span { start: 1, end: 6 },
            })),
            span: Span { start: 0, end: 7 },
        }
    );
}

#[test]
fn rejects_unclosed_group() {
    let error: ParseError = parse("(@item").expect_err("未闭合分组应报错");

    assert_eq!(error, ParseError::UnclosedGroup(Span { start: 0, end: 1 }));
}

#[test]
fn parses_empty_and_populated_arrays() {
    let empty: Expression<'_> = parse("[]").expect("空数组应成功解析");
    let populated: Expression<'_> = parse("[@aa, _bb]").expect("数组元素应成功解析");

    assert_eq!(
        empty,
        Expression {
            kind: ExpressionKind::Array(Vec::new()),
            span: Span { start: 0, end: 2 },
        }
    );
    assert_eq!(
        populated,
        Expression {
            kind: ExpressionKind::Array(vec![
                Expression {
                    kind: ExpressionKind::Variable {
                        scope: VariableScope::Local,
                        name: "aa",
                    },
                    span: Span { start: 1, end: 4 },
                },
                Expression {
                    kind: ExpressionKind::Variable {
                        scope: VariableScope::Temporary,
                        name: "bb",
                    },
                    span: Span { start: 6, end: 9 },
                },
            ]),
            span: Span { start: 0, end: 10 },
        }
    );
}

#[test]
fn rejects_unclosed_array() {
    let error: ParseError = parse("[@aa").expect_err("未闭合数组应报错");

    assert_eq!(error, ParseError::UnclosedArray(Span { start: 0, end: 1 }));
}

#[test]
fn parses_empty_and_populated_objects() {
    let empty: Expression<'_> = parse("{}").expect("空对象应成功解析");
    let populated: Expression<'_> =
        parse(r#"{name:@name,"score":$score}"#).expect("对象属性应成功解析");

    assert_eq!(
        empty,
        Expression {
            kind: ExpressionKind::Object(Vec::new()),
            span: Span { start: 0, end: 2 },
        }
    );
    assert_eq!(
        populated,
        Expression {
            kind: ExpressionKind::Object(vec![
                ObjectProperty {
                    key: ObjectKey::Identifier("name"),
                    key_span: Span { start: 1, end: 5 },
                    value: Expression {
                        kind: ExpressionKind::Variable {
                            scope: VariableScope::Local,
                            name: "name",
                        },
                        span: Span { start: 6, end: 11 },
                    },
                },
                ObjectProperty {
                    key: ObjectKey::String("score"),
                    key_span: Span { start: 12, end: 19 },
                    value: Expression {
                        kind: ExpressionKind::Variable {
                            scope: VariableScope::Variables,
                            name: "score",
                        },
                        span: Span { start: 20, end: 26 },
                    },
                },
            ]),
            span: Span { start: 0, end: 27 },
        }
    );
}

#[test]
fn rejects_unclosed_object() {
    let error: ParseError = parse("{name:@name").expect_err("未闭合对象应报错");

    assert_eq!(error, ParseError::UnclosedObject(Span { start: 0, end: 1 }));
}

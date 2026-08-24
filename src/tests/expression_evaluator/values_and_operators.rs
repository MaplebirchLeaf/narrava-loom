use super::*;

#[test]
fn empty_context_reads_scoped_variable_as_undefined() {
    let expression: Expression<'_> = parse("$score").expect("变量应成功解析");

    assert_eq!(evaluate(&expression), Ok(Value::Undefined));
}

#[test]
fn decodes_supported_string_escapes() {
    let expression: Expression<'_> = parse(
        r#""line\nquote:\" apostrophe:\' slash:\\ tab:\t return:\r back:\b form:\f vertical:\v nul:\0""#,
    )
    .expect("带受支持转义的字符串应成功解析");
    let value: Value = evaluate(&expression).expect("受支持的字符串转义应成功解码");

    assert_eq!(
        value,
        Value::string(
            "line\nquote:\" apostrophe:' slash:\\ tab:\t return:\r back:\u{0008} form:\u{000c} vertical:\u{000b} nul:\0",
        )
    );
}

#[test]
fn rejects_unknown_string_escape_at_its_source_span() {
    let expression: Expression<'_> = parse(r#""bad\q""#).expect("字符串结构应成功解析");
    let error: EvalError = evaluate(&expression).expect_err("未知字符串转义应报错");

    assert_eq!(
        error,
        EvalError::InvalidStringEscape(Span { start: 4, end: 6 })
    );
}

#[test]
fn decodes_hex_unicode_and_surrogate_pair_escapes() {
    let expression: Expression<'_> =
        parse(r#""\x41\u4E2D\uD83D\uDE00""#).expect("十六进制字符串应成功解析");
    let value: Value = evaluate(&expression).expect("合法十六进制转义应成功解码");

    assert_eq!(value, Value::string("A中😀"));
}

#[test]
fn rejects_malformed_hex_escape_at_the_whole_escape_span() {
    let cases: [(&str, Span); 2] = [
        (r#""bad\x4Z""#, Span { start: 4, end: 8 }),
        (r#""bad\u12""#, Span { start: 4, end: 8 }),
    ];

    for (source, expected_span) in cases {
        let expression: Expression<'_> = parse(source).expect("字符串结构应成功解析");
        let error: EvalError = evaluate(&expression).expect_err("无效十六进制转义应报错");

        assert_eq!(error, EvalError::InvalidStringEscape(expected_span));
    }
}

#[test]
fn rejects_unpaired_utf16_surrogates() {
    let cases: [(&str, Span); 2] = [
        (r#""\uD83D""#, Span { start: 1, end: 7 }),
        (r#""\uDE00""#, Span { start: 1, end: 7 }),
    ];

    for (source, expected_span) in cases {
        let expression: Expression<'_> = parse(source).expect("字符串结构应成功解析");
        let error: EvalError = evaluate(&expression).expect_err("孤立代理项应报错");

        assert_eq!(error, EvalError::InvalidStringEscape(expected_span));
    }
}

#[test]
fn evaluates_nested_array_and_object_literals() {
    let expression: Expression<'_> =
        parse(r#"{ name: "Narrava", "unicode\u0020key": [1, true, { empty: null }] }"#)
            .expect("嵌套复合字面量应成功解析");
    let value: Value = evaluate(&expression).expect("纯复合字面量应成功求值");

    assert_eq!(
        value,
        Value::object(vec![
            (String::from("name"), Value::string("Narrava")),
            (
                String::from("unicode key"),
                Value::array(vec![
                    Value::Number(1.0),
                    Value::Boolean(true),
                    Value::object(vec![(String::from("empty"), Value::Null)]),
                ]),
            ),
        ])
    );
}

#[test]
fn missing_variable_inside_composite_literal_is_undefined() {
    let expression: Expression<'_> = parse("[1, $score]").expect("数组应成功解析");

    assert_eq!(
        evaluate(&expression),
        Ok(Value::array(vec![Value::Number(1.0), Value::Undefined]))
    );
}

#[test]
fn later_object_property_overwrites_the_same_key() {
    let expression: Expression<'_> =
        parse("{ score: 1, other: true, score: 2 }").expect("重复对象键应成功解析");
    let value: Value = evaluate(&expression).expect("重复对象键应按 Web 语义求值");

    assert_eq!(
        value,
        Value::object(vec![
            (String::from("score"), Value::Number(2.0)),
            (String::from("other"), Value::Boolean(true)),
        ])
    );
}

#[test]
fn evaluates_logical_and_numeric_unary_operators() {
    let cases: [(&str, Value); 14] = [
        ("!undefined", Value::Boolean(true)),
        ("not []", Value::Boolean(false)),
        ("+null", Value::Number(0.0)),
        ("+true", Value::Number(1.0)),
        (r#"+" 12.5 ""#, Value::Number(12.5)),
        (r#"+"""#, Value::Number(0.0)),
        (r#"+"0x10""#, Value::Number(16.0)),
        (r#"+"0o10""#, Value::Number(8.0)),
        (r#"+"0b10""#, Value::Number(2.0)),
        (r#"+"Infinity""#, Value::Number(f64::INFINITY)),
        (r#"-"2""#, Value::Number(-2.0)),
        ("~0", Value::Number(-1.0)),
        ("~4294967295", Value::Number(0.0)),
        ("~4294967296", Value::Number(-1.0)),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("一元表达式应成功解析");
        let value: Value = evaluate(&expression).expect("受支持的一元表达式应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn invalid_numeric_string_becomes_nan() {
    let expression: Expression<'_> = parse(r#"+"Narrava""#).expect("一元表达式应成功解析");
    let value: Value = evaluate(&expression).expect("无效数值字符串应产生 NaN");
    let Value::Number(number) = value else {
        panic!("一元加应返回 Number");
    };

    assert!(number.is_nan());
}

#[test]
fn rejects_implicit_object_to_number_conversion() {
    let expression: Expression<'_> = parse("+[]").expect("一元表达式应成功解析");
    let error: EvalError = evaluate(&expression).expect_err("对象转换依赖尚未设计的原型规则");

    assert_eq!(
        error,
        EvalError::InvalidNumericConversion(Span { start: 1, end: 3 })
    );
}

#[test]
fn evaluates_typeof_for_values_without_runtime_context() {
    let cases: [(&str, &str); 7] = [
        ("typeof undefined", "undefined"),
        ("typeof null", "null"),
        ("typeof false", "boolean"),
        ("typeof 1", "number"),
        (r#"typeof "Narrava""#, "string"),
        ("typeof []", "array"),
        ("typeof {}", "object"),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("typeof 表达式应成功解析");
        let value: Value = evaluate(&expression).expect("纯值 typeof 应成功求值");

        assert_eq!(value, Value::string(expected));
    }
}

#[test]
fn typeof_missing_scoped_variable_is_undefined() {
    let expression: Expression<'_> = parse("typeof $score").expect("typeof 变量应成功解析");

    assert_eq!(evaluate(&expression), Ok(Value::string("undefined")));
}

#[test]
fn evaluates_arithmetic_binary_operators() {
    let cases: [(&str, Value); 7] = [
        ("1 + 2 * 3", Value::Number(7.0)),
        ("7 - 2", Value::Number(5.0)),
        ("3 * 4", Value::Number(12.0)),
        ("7 / 2", Value::Number(3.5)),
        ("-7 // 2", Value::Number(-3.0)),
        ("7 % 4", Value::Number(3.0)),
        ("2 ** 3 ** 2", Value::Number(512.0)),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("算术表达式应成功解析");
        let value: Value = evaluate(&expression).expect("算术表达式应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn addition_concatenates_when_either_scalar_is_a_string() {
    let cases: [(&str, &str); 4] = [
        (r#""score: " + 2"#, "score: 2"),
        (r#"1 + "2""#, "12"),
        (r#"null + "!""#, "null!"),
        (r#"-0 + "!""#, "0!"),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("字符串加法应成功解析");
        let value: Value = evaluate(&expression).expect("标量字符串加法应成功求值");

        assert_eq!(value, Value::string(expected));
    }
}

#[test]
fn division_by_zero_uses_web_number_results() {
    let positive: Expression<'_> = parse("1 / 0").expect("除法应成功解析");
    let zero: Expression<'_> = parse("0 / 0").expect("除法应成功解析");

    assert_eq!(
        evaluate(&positive).expect("非零除以零应产生无穷值"),
        Value::Number(f64::INFINITY)
    );
    let Value::Number(result) = evaluate(&zero).expect("零除以零应产生 NaN") else {
        panic!("除法必须返回 Number");
    };
    assert!(result.is_nan());
}

#[test]
fn rejects_object_conversion_in_arithmetic() {
    let cases: [(&str, EvalError); 2] = [
        (
            "[] * 2",
            EvalError::InvalidNumericConversion(Span { start: 0, end: 2 }),
        ),
        (
            r#""value: " + {}"#,
            EvalError::InvalidStringConversion(Span { start: 12, end: 14 }),
        ),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("算术表达式应成功解析");
        let error: EvalError = evaluate(&expression).expect_err("对象隐式转换必须报错");

        assert_eq!(error, expected);
    }
}

#[test]
fn evaluates_bitwise_and_shift_binary_operators() {
    let cases: [(&str, Value); 8] = [
        ("5 & 3", Value::Number(1.0)),
        ("5 | 2", Value::Number(7.0)),
        ("5 ^ 1", Value::Number(4.0)),
        ("1 << 33", Value::Number(2.0)),
        ("-8 >> 2", Value::Number(-2.0)),
        ("-1 >>> 1", Value::Number(2_147_483_647.0)),
        ("4294967296 | 1", Value::Number(1.0)),
        (r#""3" & 1"#, Value::Number(1.0)),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("按位表达式应成功解析");
        let value: Value = evaluate(&expression).expect("按位表达式应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn reports_operand_span_for_invalid_bitwise_conversion() {
    let expression: Expression<'_> = parse("1 << {}").expect("移位表达式应成功解析");
    let error: EvalError = evaluate(&expression).expect_err("对象不能隐式转换为移位数量");

    assert_eq!(
        error,
        EvalError::InvalidNumericConversion(Span { start: 5, end: 7 })
    );
}

#[test]
fn evaluates_relational_comparisons_and_aliases() {
    let cases: [(&str, bool); 10] = [
        ("2 < 3", true),
        ("3 <= 3", true),
        ("4 > 3", true),
        ("4 >= 4", true),
        ("2 lt 3", true),
        ("3 lte 3", true),
        ("4 gt 3", true),
        ("4 gte 4", true),
        (r#""10" < "2""#, true),
        (r#""2" < 10"#, true),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("关系表达式应成功解析");
        let value: Value = evaluate(&expression).expect("关系表达式应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn evaluates_between_with_four_boundary_forms() {
    let cases: [(&str, bool); 8] = [
        ("2 between() 1 3", true),
        ("1 between() 1 3", false),
        ("3 between(] 1 3", true),
        ("1 between(] 1 3", false),
        ("1 between[) 1 3", true),
        ("3 between[) 1 3", false),
        ("1 between[] 1 3", true),
        ("3 between[] 1 3", true),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("区间表达式应成功解析");
        let value: Value = evaluate(&expression).expect("区间表达式应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn between_uses_relational_string_and_error_rules() {
    let expression: Expression<'_> =
        parse(r#""b" between[] "a" "c""#).expect("字符串区间应成功解析");
    let value: Value = evaluate(&expression).expect("字符串区间应成功求值");
    assert_eq!(value, Value::Boolean(true));

    let expression: Expression<'_> = parse("2 between[] {} 3").expect("对象边界应成功解析");
    let error: EvalError = evaluate(&expression).expect_err("对象不能隐式转换为区间边界");
    assert_eq!(
        error,
        EvalError::InvalidNumericConversion(Span { start: 12, end: 14 })
    );
}

#[test]
fn evaluates_in_and_notin_membership() {
    let cases: [(&str, bool); 8] = [
        ("2 in [1, 2, 3]", true),
        (r#""2" in [1, 2, 3]"#, false),
        ("4 notin [1, 2, 3]", true),
        (r#""name" in { name: 1 }"#, true),
        (r#""missing" notin { name: 1 }"#, true),
        (r#""1" in { "1": true }"#, true),
        (r#""ava" in "Narrava""#, true),
        (r#""xyz" notin "Narrava""#, true),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("成员表达式应成功解析");
        let value: Value = evaluate(&expression).expect("成员表达式应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn membership_compares_object_identity_and_rejects_invalid_container() {
    let aggregate: Expression<'_> = parse("{} in [{}]").expect("集合成员表达式应成功解析");
    assert_eq!(
        evaluate(&aggregate).expect("Object 成员应按引用身份比较"),
        Value::Boolean(false)
    );

    let scalar: Expression<'_> = parse("1 in 2").expect("标量成员表达式应成功解析");
    assert_eq!(
        evaluate(&scalar).expect_err("标量不能作为成员容器"),
        EvalError::InvalidMembershipTarget(Span { start: 5, end: 6 })
    );
}

#[test]
fn evaluates_instanceof_with_builtin_prototype_chain() {
    let cases: [(&str, bool); 11] = [
        ("[] instanceof Array", true),
        ("[] instanceof Object", true),
        ("{} instanceof Object", true),
        (r#""Narrava" instanceof String"#, true),
        (r#""Narrava" instanceof Object"#, true),
        ("1 instanceof Number", true),
        ("true instanceof Boolean", true),
        ("1 instanceof String", false),
        ("null instanceof Object", false),
        ("undefined instanceof Object", false),
        ("{} instanceof Array", false),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("原型判断应成功解析");
        let value: Value = evaluate(&expression).expect("内置原型判断应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn native_methods_inherit_function_and_object_prototypes() {
    let cases: [(&str, bool); 4] = [
        ("[1].includes instanceof Function", true),
        ("[1].includes instanceof Object", true),
        (r#""Narrava".trim instanceof Function"#, true),
        ("1 instanceof Function", false),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Function 原型判断应成功解析");
        let value: Value = evaluate(&expression).expect("原生方法原型判断应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn instanceof_rejects_unknown_or_dynamic_prototype() {
    let unknown: Expression<'_> = parse("[] instanceof Player").expect("未知原型应成功解析");
    assert_eq!(
        evaluate(&unknown).expect_err("未知原型必须报错"),
        EvalError::InvalidPrototype(Span { start: 14, end: 20 })
    );

    let dynamic: Expression<'_> = parse(r#"[] instanceof "Array""#).expect("动态原型应成功解析");
    assert_eq!(
        evaluate(&dynamic).expect_err("instanceof 右侧必须是原型身份"),
        EvalError::InvalidPrototype(Span { start: 14, end: 21 })
    );
}

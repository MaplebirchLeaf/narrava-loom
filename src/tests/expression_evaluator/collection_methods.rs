use super::*;

#[test]
fn rejects_length_on_value_without_that_property() {
    let expression: Expression<'_> = parse("1.length").expect("数值成员访问应成功解析");
    assert_eq!(
        evaluate(&expression).expect_err("Number 首轮没有 length"),
        EvalError::UnknownMember(Span { start: 2, end: 8 })
    );
}

#[test]
fn evaluates_object_own_properties_with_dot_members() {
    let cases: [(&str, Value); 3] = [
        (r#"{ name: "Narrava" }.name"#, Value::string("Narrava")),
        ("{ count: 2 }.count", Value::Number(2.0)),
        (
            "{ nested: { ready: true } }.nested.ready",
            Value::Boolean(true),
        ),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("对象点属性应成功解析");
        let value: Value = evaluate(&expression).expect("对象自身属性应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn rejects_unknown_object_dot_member() {
    let expression: Expression<'_> = parse("{}.missing").expect("未知对象成员应成功解析");

    assert_eq!(
        evaluate(&expression).expect_err("普通点访问必须报告未知成员"),
        EvalError::UnknownMember(Span { start: 3, end: 10 })
    );
}

#[test]
fn evaluates_array_and_string_includes_methods() {
    let cases: [(&str, bool); 5] = [
        ("[1, 2, 3].includes(2)", true),
        (r#"[1, 2, 3].includes("2")"#, false),
        (r#""Narrava".includes("ava")"#, true),
        (r#""Narrava".includes("xyz")"#, false),
        (r#""😀Narrava".includes("😀")"#, true),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("includes 调用应成功解析");
        let value: Value = evaluate(&expression).expect("只读 includes 应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn evaluates_string_boundary_search_methods() {
    let cases: [(&str, bool); 8] = [
        (r#""Narrava".startsWith("Nar")"#, true),
        (r#""Narrava".startsWith("arr")"#, false),
        (r#""Narrava".endsWith("ava")"#, true),
        (r#""Narrava".endsWith("Narr")"#, false),
        (r#""Narrava".startsWith("")"#, true),
        (r#""Narrava".endsWith("")"#, true),
        (r#""😀Narrava".startsWith("😀")"#, true),
        (r#""Narrava😀".endsWith("😀")"#, true),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("字符串边界查找应成功解析");
        let value: Value = evaluate(&expression).expect("字符串边界查找应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn string_boundary_search_validates_call_arguments() {
    let missing: Expression<'_> = parse(r#""Narrava".startsWith()"#).expect("空参数调用应成功解析");
    assert_eq!(
        evaluate(&missing).expect_err("startsWith 必须接收一个参数"),
        EvalError::InvalidArgumentCount(Span { start: 0, end: 22 })
    );

    let wrong_type: Expression<'_> =
        parse(r#""Narrava".endsWith(false)"#).expect("不同参数类型仍应成功解析");
    assert_eq!(
        evaluate(&wrong_type).expect_err("endsWith 只接受字符串"),
        EvalError::InvalidStringConversion(Span { start: 19, end: 24 })
    );
}

#[test]
fn evaluates_string_trim_without_arguments() {
    let cases: [(&str, &str); 5] = [
        (r#""  Narrava\n".trim()"#, "Narrava"),
        (r#""\u3000Narrava\u3000".trim()"#, "Narrava"),
        (r#""\uFEFFNarrava\uFEFF".trim()"#, "Narrava"),
        (r#""\u0085Narrava\u0085".trim()"#, "\u{0085}Narrava\u{0085}"),
        (r#""Narrava".trim()"#, "Narrava"),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("trim 调用应成功解析");
        let value: Value = evaluate(&expression).expect("无参数 trim 应成功求值");

        assert_eq!(value, Value::string(expected), "表达式：{source}");
    }
}

#[test]
fn trim_rejects_extra_arguments() {
    let expression: Expression<'_> =
        parse(r#"" Narrava ".trim(1)"#).expect("额外参数调用仍应成功解析");

    assert_eq!(
        evaluate(&expression).expect_err("trim 不接受参数"),
        EvalError::InvalidArgumentCount(Span { start: 0, end: 19 })
    );
}

#[test]
fn evaluates_string_case_conversion_without_arguments() {
    let cases: [(&str, &str); 5] = [
        (r#""Narrava".toLowerCase()"#, "narrava"),
        (r#""Narrava".toUpperCase()"#, "NARRAVA"),
        (r#""ÄÖÜ".toLowerCase()"#, "äöü"),
        (r#""straße".toUpperCase()"#, "STRASSE"),
        (r#""😀Narrava".toUpperCase()"#, "😀NARRAVA"),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("大小写转换应成功解析");
        let value: Value = evaluate(&expression).expect("无参数大小写转换应成功求值");

        assert_eq!(value, Value::string(expected), "表达式：{source}");
    }
}

#[test]
fn string_case_conversion_rejects_extra_arguments() {
    let expression: Expression<'_> =
        parse(r#""Narrava".toLowerCase(1)"#).expect("额外参数调用仍应成功解析");

    assert_eq!(
        evaluate(&expression).expect_err("大小写转换不接受参数"),
        EvalError::InvalidArgumentCount(Span { start: 0, end: 24 })
    );
}

#[test]
fn evaluates_array_at_with_web_index_rules() {
    let cases: [(&str, Value); 9] = [
        ("[10, 20, 30].at(0)", Value::Number(10.0)),
        ("[10, 20, 30].at(-1)", Value::Number(30.0)),
        ("[10, 20, 30].at(1.9)", Value::Number(20.0)),
        (r#"[10, 20, 30].at("2")"#, Value::Number(30.0)),
        ("[10, 20, 30].at(true)", Value::Number(20.0)),
        (r#"[10, 20, 30].at(+"bad")"#, Value::Number(10.0)),
        ("[10, 20, 30].at(9)", Value::Undefined),
        ("[10, 20, 30].at(-4)", Value::Undefined),
        (r#"[10, 20, 30].at(+"Infinity")"#, Value::Undefined),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Array.at 调用应成功解析");
        let value: Value = evaluate(&expression).expect("有效 Array.at 应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn array_at_validates_its_argument() {
    let missing: Expression<'_> = parse("[].at()").expect("空参数调用应成功解析");
    assert_eq!(
        evaluate(&missing).expect_err("Array.at 必须接收一个索引"),
        EvalError::InvalidArgumentCount(Span { start: 0, end: 7 })
    );

    let aggregate: Expression<'_> = parse("[1].at({})").expect("对象索引仍应成功解析");
    assert_eq!(
        evaluate(&aggregate).expect_err("对象不能转换为数组索引"),
        EvalError::InvalidNumericConversion(Span { start: 7, end: 9 })
    );
}

#[test]
fn evaluates_array_slice_with_optional_relative_bounds() {
    let cases: [(&str, Value); 10] = [
        (
            "[1, 2, 3].slice()",
            Value::array(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ]),
        ),
        (
            "[1, 2, 3].slice(1)",
            Value::array(vec![Value::Number(2.0), Value::Number(3.0)]),
        ),
        (
            "[1, 2, 3].slice(1, 3)",
            Value::array(vec![Value::Number(2.0), Value::Number(3.0)]),
        ),
        (
            "[1, 2, 3].slice(-2)",
            Value::array(vec![Value::Number(2.0), Value::Number(3.0)]),
        ),
        (
            "[1, 2, 3].slice(0, -1)",
            Value::array(vec![Value::Number(1.0), Value::Number(2.0)]),
        ),
        ("[1, 2, 3].slice(9)", Value::array(Vec::new())),
        ("[1, 2, 3].slice(2, 1)", Value::array(Vec::new())),
        (
            r#"[1, 2, 3].slice("1", "2")"#,
            Value::array(vec![Value::Number(2.0)]),
        ),
        (
            r#"[1, 2, 3].slice(+"bad", 1)"#,
            Value::array(vec![Value::Number(1.0)]),
        ),
        (
            r#"[1, 2, 3].slice(0, +"Infinity")"#,
            Value::array(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ]),
        ),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Array.slice 调用应成功解析");
        let value: Value = evaluate(&expression).expect("有效 Array.slice 应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn array_slice_validates_arity_and_bound_types() {
    let excessive: Expression<'_> = parse("[1].slice(0, 1, 2)").expect("三个参数仍应成功解析");
    assert_eq!(
        evaluate(&excessive).expect_err("Array.slice 最多接收两个参数"),
        EvalError::InvalidArgumentCount(Span { start: 0, end: 18 })
    );

    let aggregate: Expression<'_> = parse("[1].slice(0,{})").expect("对象边界仍应成功解析");
    assert_eq!(
        evaluate(&aggregate).expect_err("对象不能转换为 slice 边界"),
        EvalError::InvalidNumericConversion(Span { start: 12, end: 14 })
    );
}

#[test]
fn evaluates_array_index_of_with_optional_start() {
    let cases: [(&str, f64); 10] = [
        ("[1, 2, 3].indexOf(2)", 1.0),
        (r#"[1, 2, 3].indexOf("2")"#, -1.0),
        (r#"[1, 2, 3].indexOf(+"bad")"#, -1.0),
        ("[1, 2, 1].indexOf(1, 1)", 2.0),
        ("[1, 2, 1].indexOf(1, -1)", 2.0),
        ("[1, 2, 1].indexOf(1, -9)", 0.0),
        (r#"[1, 2, 1].indexOf(1, "1")"#, 2.0),
        (r#"[1, 2, 1].indexOf(1, +"bad")"#, 0.0),
        (r#"[1, 2, 1].indexOf(1, +"Infinity")"#, -1.0),
        (r#"[1, 2, 1].indexOf(1, -"Infinity")"#, 0.0),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Array.indexOf 调用应成功解析");
        let value: Value = evaluate(&expression).expect("有效 Array.indexOf 应成功求值");

        assert_eq!(value, Value::Number(expected), "表达式：{source}");
    }
}

#[test]
fn array_index_of_validates_arguments() {
    let missing: Expression<'_> = parse("[].indexOf()").expect("空参数调用应成功解析");
    assert_eq!(
        evaluate(&missing).expect_err("Array.indexOf 至少需要搜索值"),
        EvalError::InvalidArgumentCount(Span { start: 0, end: 12 })
    );

    let aggregate: Expression<'_> = parse("[1].indexOf({})").expect("集合搜索值仍应成功解析");
    assert_eq!(
        evaluate(&aggregate).expect("Object 搜索值应按引用身份比较"),
        Value::Number(-1.0)
    );

    let invalid_start: Expression<'_> = parse("[1].indexOf(1,{})").expect("对象起点仍应成功解析");
    assert_eq!(
        evaluate(&invalid_start).expect_err("对象不能转换为搜索起点"),
        EvalError::InvalidNumericConversion(Span { start: 14, end: 16 })
    );
}

#[test]
fn evaluates_array_concat_with_single_level_spread() {
    let cases: [(&str, Value); 4] = [
        (
            "[1, 2].concat()",
            Value::array(vec![Value::Number(1.0), Value::Number(2.0)]),
        ),
        (
            "[1, 2].concat([3, 4], 5)",
            Value::array(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
                Value::Number(4.0),
                Value::Number(5.0),
            ]),
        ),
        (
            r#"["a"].concat("b", ["c"])"#,
            Value::array(vec![
                Value::string("a"),
                Value::string("b"),
                Value::string("c"),
            ]),
        ),
        (
            "[[1]].concat([[2]])",
            Value::array(vec![
                Value::array(vec![Value::Number(1.0)]),
                Value::array(vec![Value::Number(2.0)]),
            ]),
        ),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Array.concat 调用应成功解析");
        let value: Value = evaluate(&expression).expect("Array.concat 应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn evaluates_array_join_with_controlled_string_conversion() {
    let cases: [(&str, &str); 7] = [
        ("[].join()", ""),
        (r#"[1, true, null, undefined, "x"].join()"#, "1,true,,,x"),
        (r#"[1, 2, 3].join("-")"#, "1-2-3"),
        ("[1, 2].join(undefined)", "1,2"),
        ("[1, 2].join(null)", "1null2"),
        (r#"[[1, 2], [3]].join("|")"#, "1,2|3"),
        (r#"[[1, [2, 3]]].join("|")"#, "1,2,3"),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Array.join 调用应成功解析");
        let value: Value = evaluate(&expression).expect("有效 Array.join 应成功求值");

        assert_eq!(value, Value::string(expected), "表达式：{source}");
    }
}

#[test]
fn array_join_rejects_invalid_conversions_and_extra_arguments() {
    let object_element: Expression<'_> = parse("[{}].join()").expect("对象元素应成功解析");
    assert_eq!(
        evaluate(&object_element).expect_err("对象元素不能隐式转换为字符串"),
        EvalError::InvalidStringConversion(Span { start: 0, end: 11 })
    );

    let object_separator: Expression<'_> = parse("[1].join({})").expect("对象分隔符应成功解析");
    assert_eq!(
        evaluate(&object_separator).expect_err("对象分隔符不能转换为字符串"),
        EvalError::InvalidStringConversion(Span { start: 9, end: 11 })
    );

    let excessive: Expression<'_> = parse(r#"[1].join(",", "-")"#).expect("两个参数仍应成功解析");
    assert_eq!(
        evaluate(&excessive).expect_err("Array.join 最多接收一个参数"),
        EvalError::InvalidArgumentCount(Span { start: 0, end: 18 })
    );
}

#[test]
fn evaluates_string_slice_by_utf16_units() {
    let cases: [(&str, Value); 7] = [
        (r#""Narrava".slice()"#, Value::string("Narrava")),
        (r#""Narrava".slice(1, 4)"#, Value::string("arr")),
        (r#""Narrava".slice(-3)"#, Value::string("ava")),
        (r#""Narrava".slice(4, 2)"#, Value::string("")),
        (
            r#""😀A".slice(0, 1)"#,
            Value::String(TextValue::from_units(vec![0xD83D])),
        ),
        (
            r#""😀A".slice(1, 2)"#,
            Value::String(TextValue::from_units(vec![0xDE00])),
        ),
        (r#""😀A".slice(0, 2)"#, Value::string("😀")),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("String.slice 调用应成功解析");
        let value: Value = evaluate(&expression).expect("有效 String.slice 应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn string_slice_validates_arity_and_bound_types() {
    let excessive: Expression<'_> = parse(r#""x".slice(0,1,2)"#).expect("三个参数仍应成功解析");
    assert_eq!(
        evaluate(&excessive).expect_err("String.slice 最多接收两个参数"),
        EvalError::InvalidArgumentCount(Span { start: 0, end: 16 })
    );

    let aggregate: Expression<'_> = parse(r#""x".slice({})"#).expect("对象边界仍应成功解析");
    assert_eq!(
        evaluate(&aggregate).expect_err("对象不能转换为字符串切片边界"),
        EvalError::InvalidNumericConversion(Span { start: 10, end: 12 })
    );
}

#[test]
fn evaluates_string_split_with_utf16_and_limit_rules() {
    let cases: [(&str, Value); 8] = [
        (
            r#""Narrava".split()"#,
            Value::array(vec![Value::string("Narrava")]),
        ),
        (
            r#""a,b,c".split(",")"#,
            Value::array(vec![
                Value::string("a"),
                Value::string("b"),
                Value::string("c"),
            ]),
        ),
        (
            r#""a,,b,".split(",")"#,
            Value::array(vec![
                Value::string("a"),
                Value::string(""),
                Value::string("b"),
                Value::string(""),
            ]),
        ),
        (
            r#""abc".split("", 2)"#,
            Value::array(vec![Value::string("a"), Value::string("b")]),
        ),
        (
            r#""😀A".split("")"#,
            Value::array(vec![
                Value::String(TextValue::from_units(vec![0xD83D])),
                Value::String(TextValue::from_units(vec![0xDE00])),
                Value::string("A"),
            ]),
        ),
        (r#""".split("")"#, Value::array(Vec::new())),
        (r#""abc".split(undefined, 0)"#, Value::array(Vec::new())),
        (
            r#""123".split(2)"#,
            Value::array(vec![Value::string("1"), Value::string("3")]),
        ),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("String.split 调用应成功解析");
        let value: Value = evaluate(&expression).expect("有效 String.split 应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn string_split_validates_arguments() {
    let invalid_separator: Expression<'_> =
        parse(r#""x".split({})"#).expect("对象分隔符仍应成功解析");
    assert_eq!(
        evaluate(&invalid_separator).expect_err("对象不能转换为 split 分隔符"),
        EvalError::InvalidStringConversion(Span { start: 10, end: 12 })
    );

    let invalid_limit: Expression<'_> =
        parse(r#""x".split("", {})"#).expect("对象上限仍应成功解析");
    assert_eq!(
        evaluate(&invalid_limit).expect_err("对象不能转换为 split 上限"),
        EvalError::InvalidNumericConversion(Span { start: 14, end: 16 })
    );

    let excessive: Expression<'_> = parse(r#""x".split("", 1, 2)"#).expect("三个参数仍应成功解析");
    assert_eq!(
        evaluate(&excessive).expect_err("String.split 最多接收两个参数"),
        EvalError::InvalidArgumentCount(Span { start: 0, end: 19 })
    );
}

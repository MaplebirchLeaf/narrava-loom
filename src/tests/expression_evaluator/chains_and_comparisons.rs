use super::*;

#[test]
fn evaluates_array_object_and_string_index_reads() {
    let cases: [(&str, Value); 13] = [
        ("[10, 20][1]", Value::Number(20.0)),
        (r#"["a", "b"]["0"]"#, Value::string("a")),
        ("[10][9]", Value::Undefined),
        ("[10][-1]", Value::Undefined),
        ("[10][0.5]", Value::Undefined),
        ("[10, 20][\"length\"]", Value::Number(2.0)),
        (r#"{ name: "Narrava" }["name"]"#, Value::string("Narrava")),
        (r#"{ "1": "one" }[1]"#, Value::string("one")),
        (r#"{ name: "Narrava" }["missing"]"#, Value::Undefined),
        (
            r#""😀A"[0]"#,
            Value::String(TextValue::from_units(vec![0xD83D])),
        ),
        (
            r#""😀A"[1]"#,
            Value::String(TextValue::from_units(vec![0xDE00])),
        ),
        (r#""😀A"[2]"#, Value::string("A")),
        (r#""😀A"[3]"#, Value::Undefined),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("普通索引读取应成功解析");
        let value: Value = evaluate(&expression).expect("有效普通索引读取应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn index_read_validates_target_and_key_conversion() {
    let invalid_target: Expression<'_> = parse("1[0]").expect("数值索引结构应成功解析");
    assert_eq!(
        evaluate(&invalid_target).expect_err("数值不是可索引容器"),
        EvalError::InvalidIndexTarget(Span { start: 0, end: 1 })
    );

    let invalid_key: Expression<'_> = parse("[][{}]").expect("对象键索引应成功解析");
    assert_eq!(
        evaluate(&invalid_key).expect_err("对象不能转换为索引键"),
        EvalError::InvalidStringConversion(Span { start: 3, end: 5 })
    );
}

#[test]
fn optional_member_and_index_short_circuit_nullish_targets() {
    let cases: [&str; 4] = [
        "null?.missing",
        "undefined?.missing",
        "null?.[{}]",
        "undefined?.[0]",
    ];

    for source in cases {
        let expression: Expression<'_> = parse(source).expect("可选链应成功解析");
        let value: Value = evaluate(&expression).expect("空值目标应短路为 undefined");

        assert_eq!(value, Value::Undefined, "表达式：{source}");
    }
}

#[test]
fn optional_member_and_index_read_non_nullish_targets_normally() {
    let cases: [(&str, Value); 3] = [
        (r#"({ name: "Narrava" })?.name"#, Value::string("Narrava")),
        (r#"["zero"]?.[0]"#, Value::string("zero")),
        (
            r#""😀"?.[1]"#,
            Value::String(TextValue::from_units(vec![0xde00])),
        ),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("非空可选链应成功解析");
        let value: Value = evaluate(&expression).expect("非空目标应执行普通读取");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn optional_short_circuit_propagates_through_member_and_index_chain() {
    let cases: [&str; 4] = [
        "null?.profile.name",
        "undefined?.items[{}]",
        "null?.[0].length",
        "undefined?.[{}][0]",
    ];

    for source in cases {
        let expression: Expression<'_> = parse(source).expect("连续可选链应成功解析");
        let value: Value = evaluate(&expression).expect("连续链应保持短路状态");

        assert_eq!(value, Value::Undefined, "表达式：{source}");
    }
}

#[test]
fn optional_short_circuit_does_not_hide_real_undefined_or_cross_group() {
    let real_undefined: Expression<'_> =
        parse("({ profile: undefined })?.profile.name").expect("非空目标链应成功解析");
    assert_eq!(
        evaluate(&real_undefined).expect_err("真实 undefined 不能被视作短路"),
        EvalError::UnknownMember(Span { start: 34, end: 38 })
    );

    let grouped: Expression<'_> = parse("(null?.profile).name").expect("带括号的可选链应成功解析");
    assert_eq!(
        evaluate(&grouped).expect_err("括号必须结束可选链传播"),
        EvalError::UnknownMember(Span { start: 16, end: 20 })
    );
}

#[test]
fn optional_call_short_circuits_or_calls_non_nullish_callable() {
    let short_circuit: Expression<'_> = parse("null?.(@missing)").expect("空值可选调用应成功解析");
    assert_eq!(
        evaluate(&short_circuit).expect("空值可选调用不得求值参数"),
        Value::Undefined
    );

    let callable: Expression<'_> =
        parse(r#"["Narrava"].includes?.("Narrava")"#).expect("原生方法可选调用应成功解析");
    assert_eq!(
        evaluate(&callable).expect("非空 callable 应执行调用"),
        Value::Boolean(true)
    );
}

#[test]
fn call_and_following_members_preserve_optional_chain_short_circuit() {
    let cases: [&str; 3] = [
        "null?.missing()",
        "undefined?.missing(@unused).name",
        "null?.()?.name",
    ];

    for source in cases {
        let expression: Expression<'_> = parse(source).expect("带调用的连续链应成功解析");
        let value: Value = evaluate(&expression).expect("调用链应保持短路状态");

        assert_eq!(value, Value::Undefined, "表达式：{source}");
    }
}

#[test]
fn optional_call_does_not_hide_non_callable_values_or_cross_group() {
    let non_callable: Expression<'_> = parse("1?.()").expect("数值可选调用应成功解析");
    assert_eq!(
        evaluate(&non_callable).expect_err("非空数值仍不可调用"),
        EvalError::NotCallable(Span { start: 0, end: 1 })
    );

    let grouped: Expression<'_> = parse("(null?.missing)()").expect("分组后的普通调用应成功解析");
    assert_eq!(
        evaluate(&grouped).expect_err("括号必须结束调用链短路"),
        EvalError::NotCallable(Span { start: 0, end: 15 })
    );
}

#[test]
fn includes_validates_call_arguments() {
    let missing: Expression<'_> = parse("[].includes()").expect("空参数调用应成功解析");
    assert_eq!(
        evaluate(&missing).expect_err("includes 必须接收一个参数"),
        EvalError::InvalidArgumentCount(Span { start: 0, end: 13 })
    );

    let wrong_type: Expression<'_> =
        parse(r#""Narrava".includes(1)"#).expect("不同参数类型仍应成功解析");
    assert_eq!(
        evaluate(&wrong_type).expect_err("String.includes 只接受字符串"),
        EvalError::InvalidStringConversion(Span { start: 19, end: 20 })
    );
}

#[test]
fn rejects_calling_a_value_that_is_not_callable() {
    let expression: Expression<'_> = parse("1()").expect("普通调用结构应成功解析");

    assert_eq!(
        evaluate(&expression).expect_err("数字不能作为函数调用"),
        EvalError::NotCallable(Span { start: 0, end: 1 })
    );
}

#[test]
fn relational_comparison_handles_utf16_nan_and_invalid_objects() {
    let utf16: Expression<'_> =
        parse(r#""\uD83D\uDE00" < "\uE000""#).expect("Unicode 比较应成功解析");
    let nan: Expression<'_> = parse(r#""Narrava" < 1"#).expect("NaN 比较应成功解析");
    let object: Expression<'_> = parse("1 < {}").expect("对象比较应成功解析");

    assert_eq!(
        evaluate(&utf16).expect("字符串应按 UTF-16 码元比较"),
        Value::Boolean(true)
    );
    assert_eq!(
        evaluate(&nan).expect("NaN 关系比较应返回 false"),
        Value::Boolean(false)
    );
    assert_eq!(
        evaluate(&object).expect_err("对象不能隐式参与关系比较"),
        EvalError::InvalidNumericConversion(Span { start: 4, end: 6 })
    );
}

#[test]
fn evaluates_three_way_comparison() {
    let cases: [(&str, f64); 6] = [
        ("1 <=> 2", -1.0),
        ("2 <=> 2", 0.0),
        ("3 <=> 2", 1.0),
        (r#""10" <=> "2""#, -1.0),
        (r#""2" <=> 10"#, -1.0),
        (r#""\uD83D\uDE00" <=> "\uE000""#, -1.0),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("三向比较应成功解析");
        let value: Value = evaluate(&expression).expect("可排序值应成功完成三向比较");

        assert_eq!(value, Value::Number(expected), "表达式：{source}");
    }
}

#[test]
fn three_way_comparison_rejects_nan_and_objects() {
    let nan: Expression<'_> = parse(r#""Narrava" <=> 1"#).expect("三向比较应成功解析");
    let object: Expression<'_> = parse("1 <=> {}").expect("三向比较应成功解析");

    assert_eq!(
        evaluate(&nan).expect_err("NaN 不存在可靠顺序"),
        EvalError::UnorderedComparison(Span { start: 0, end: 9 })
    );
    assert_eq!(
        evaluate(&object).expect_err("对象不能隐式参与三向比较"),
        EvalError::InvalidNumericConversion(Span { start: 6, end: 8 })
    );
}

#[test]
fn evaluates_strict_equality_and_aliases_for_scalars() {
    let cases: [(&str, bool); 8] = [
        ("1 === 1", true),
        ("1 is 1", true),
        (r#"1 === "1""#, false),
        ("null === undefined", false),
        ("1 !== 2", true),
        ("1 isnot 2", true),
        (r#""Narrava" === "Narrava""#, true),
        (r#"+"bad" === +"bad""#, false),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("严格相等表达式应成功解析");
        let value: Value = evaluate(&expression).expect("标量严格相等应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn evaluates_loose_equality_and_aliases_for_scalars() {
    let cases: [(&str, bool); 7] = [
        (r#"1 == "1""#, true),
        (r#"1 equ "1""#, true),
        ("0 == false", true),
        (r#""0" == false"#, true),
        ("null == undefined", true),
        (r#"1 != "2""#, true),
        (r#""bad" == 0"#, false),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("非严格相等表达式应成功解析");
        let value: Value = evaluate(&expression).expect("标量非严格相等应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn array_equality_uses_reference_identity() {
    let array: Value = Value::array(vec![Value::Number(1.0)]);
    let context: WritableContext = WritableContext {
        global: None,
        variables: vec![
            (
                VariableScope::Variables,
                String::from("left"),
                array.clone(),
            ),
            (VariableScope::Variables, String::from("right"), array),
        ],
    };
    let same: Expression<'_> = parse("$left === $right").expect("Array 身份比较应成功解析");
    let distinct: Expression<'_> = parse("[] !== []").expect("独立 Array 比较应成功解析");

    assert_eq!(
        evaluate_with(&same, &context).expect("Array 别名应可比较"),
        Value::Boolean(true)
    );
    assert_eq!(
        evaluate(&distinct).expect("独立 Array 应可比较"),
        Value::Boolean(true)
    );
}

#[test]
fn object_equality_uses_reference_identity() {
    let object: Value = Value::object(vec![(String::from("value"), Value::Number(1.0))]);
    let context: WritableContext = WritableContext {
        global: None,
        variables: vec![
            (
                VariableScope::Variables,
                String::from("left"),
                object.clone(),
            ),
            (VariableScope::Variables, String::from("right"), object),
        ],
    };
    let same: Expression<'_> = parse("$left === $right").expect("Object 身份比较应成功解析");
    let distinct: Expression<'_> = parse("{} !== {}").expect("独立 Object 比较应成功解析");

    assert_eq!(
        evaluate_with(&same, &context).expect("Object 别名应可比较"),
        Value::Boolean(true)
    );
    assert_eq!(
        evaluate(&distinct).expect("独立 Object 应可比较"),
        Value::Boolean(true)
    );
}

#[test]
fn native_callable_equality_uses_registered_function_or_method_identity() {
    let cases: [(&str, bool); 5] = [
        ("defined === defined", true),
        ("defined == defined", true),
        ("defined !== random", true),
        ("[].includes === [1].includes", true),
        ("[].includes !== [].slice", true),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Callable 身份比较应成功解析");
        let value: Value = evaluate(&expression).expect("原生 Callable 应使用登记身份比较");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn evaluates_logical_and_nullish_short_circuit_values() {
    let cases: [(&str, Value); 10] = [
        ("0 && $score", Value::Number(0.0)),
        ("0 and $score", Value::Number(0.0)),
        ("true && 2", Value::Number(2.0)),
        ("1 || $score", Value::Number(1.0)),
        ("1 or $score", Value::Number(1.0)),
        (r#"false || "fallback""#, Value::string("fallback")),
        ("null ?? 4", Value::Number(4.0)),
        ("undefined ?? 5", Value::Number(5.0)),
        ("false ?? $score", Value::Boolean(false)),
        (r#""" ?? $score"#, Value::string("")),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("短路表达式应成功解析");
        let value: Value = evaluate(&expression).expect("未选分支不应被求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn selected_short_circuit_missing_variable_returns_undefined() {
    let cases: [&str; 3] = ["true && $score", "false || $score", "null ?? $score"];

    for source in cases {
        let expression: Expression<'_> = parse(source).expect("短路表达式应成功解析");

        assert_eq!(evaluate(&expression), Ok(Value::Undefined));
    }
}

#[test]
fn evaluates_only_selected_conditional_branch() {
    let cases: [(&str, Value); 6] = [
        ("true ? 1 : $score", Value::Number(1.0)),
        ("false ? $score : 2", Value::Number(2.0)),
        (r#"0 ? "yes" : "no""#, Value::string("no")),
        (r#"[] ? "yes" : "no""#, Value::string("yes")),
        ("null ? 1 : 2", Value::Number(2.0)),
        (
            "1 ? [1, 2] : {}",
            Value::array(vec![Value::Number(1.0), Value::Number(2.0)]),
        ),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("条件表达式应成功解析");
        let value: Value = evaluate(&expression).expect("未选择分支不应被求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn selected_conditional_missing_variable_returns_undefined() {
    let cases: [&str; 2] = ["true ? $score : 0", "false ? 0 : $score"];

    for source in cases {
        let expression: Expression<'_> = parse(source).expect("条件表达式应成功解析");

        assert_eq!(evaluate(&expression), Ok(Value::Undefined));
    }
}

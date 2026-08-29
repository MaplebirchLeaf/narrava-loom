use super::*;

#[test]
fn clone_deep_copies_value_graph_without_sharing_array_or_object_identity() {
    let nested = Value::array(vec![Value::Number(1.0)]);
    let source = Value::object(vec![(String::from("nested"), nested.clone())]);
    let context = SingleGlobalContext {
        name: String::from("source"),
        value: source.clone(),
    };
    let expression = parse("clone(source)").expect("clone 应成功解析");
    let cloned = evaluate_with(&expression, &context).expect("clone 应深拷贝值图");
    let identity = parse("clone(source) !== source && clone(source).nested !== source.nested")
        .expect("引用身份检查应成功解析");

    assert_eq!(
        evaluate_with(&identity, &context).expect("应可比较 clone 引用身份"),
        Value::Boolean(true)
    );
    let Value::Object(cloned_object) = cloned else {
        panic!("clone 应保留 Object 类型")
    };
    let Value::Array(cloned_nested) = cloned_object.snapshot()[0].1.clone() else {
        panic!("clone 应保留嵌套 Array")
    };
    cloned_nested.with_mut(|values| values.push(Value::Number(2.0)));
    let Value::Array(original_nested) = nested else {
        unreachable!("fixture 必须是 Array")
    };
    assert_eq!(original_nested.snapshot(), vec![Value::Number(1.0)]);
}

#[test]
fn clone_accepts_exactly_one_argument() {
    for source in ["clone()", "clone(1, 2)"] {
        let expression = parse(source).expect("clone 参数数量应成功解析");
        assert_eq!(
            evaluate(&expression).expect_err("clone 必须只接收一个参数"),
            EvalError::InvalidArgumentCount(expression.span)
        );
    }
}

#[test]
fn evaluates_defined_builtin_function() {
    let cases: [(&str, bool); 6] = [
        ("defined(undefined)", false),
        ("defined(null)", true),
        ("defined(false)", true),
        ("defined(0)", true),
        (r#"defined("")"#, true),
        ("defined([])", true),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("defined 调用应成功解析");
        let value: Value = evaluate(&expression).expect("defined 应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn defined_builtin_has_callable_identity_and_validates_arity() {
    let callable_cases: [(&str, Value); 2] = [
        (r#"typeof defined"#, Value::string("function")),
        ("defined instanceof Function", Value::Boolean(true)),
    ];

    for (source, expected) in callable_cases {
        let expression: Expression<'_> = parse(source).expect("defined callable 应成功解析");
        let value: Value = evaluate(&expression).expect("defined callable 身份应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }

    for source in ["defined()", "defined(1, 2)"] {
        let expression: Expression<'_> = parse(source).expect("defined 参数数量应成功解析");
        assert_eq!(
            evaluate(&expression).expect_err("defined 必须只接收一个参数"),
            EvalError::InvalidArgumentCount(expression.span),
            "表达式：{source}"
        );
    }
}

#[test]
fn evaluates_empty_builtin_function() {
    let cases: [(&str, bool); 12] = [
        ("empty(undefined)", true),
        ("empty(null)", true),
        (r#"empty("")"#, true),
        ("empty([])", true),
        ("empty({})", true),
        (r#"empty("Narrava")"#, false),
        ("empty([0])", false),
        ("empty({ value: undefined })", false),
        ("empty(0)", false),
        ("empty(false)", false),
        ("empty(defined)", false),
        ("empty(true)", false),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("empty 调用应成功解析");
        let value: Value = evaluate(&expression).expect("empty 应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn empty_builtin_has_callable_identity_and_validates_arity() {
    let type_expression: Expression<'_> = parse("typeof empty").expect("empty 类型应成功解析");
    assert_eq!(
        evaluate(&type_expression).expect("empty 应具有 callable 身份"),
        Value::string("function")
    );

    for source in ["empty()", "empty(1, 2)"] {
        let expression: Expression<'_> = parse(source).expect("empty 参数数量应成功解析");
        assert_eq!(
            evaluate(&expression).expect_err("empty 必须只接收一个参数"),
            EvalError::InvalidArgumentCount(expression.span),
            "表达式：{source}"
        );
    }
}

#[test]
fn evaluates_object_collection_builtin_functions_in_property_order() {
    let cases: [(&str, Value); 3] = [
        (
            "keys({ second: 2, first: 1 })",
            Value::array(vec![Value::string("second"), Value::string("first")]),
        ),
        (
            "values({ second: 2, first: 1 })",
            Value::array(vec![Value::Number(2.0), Value::Number(1.0)]),
        ),
        (
            "entries({ second: 2, first: 1 })",
            Value::array(vec![
                Value::array(vec![Value::string("second"), Value::Number(2.0)]),
                Value::array(vec![Value::string("first"), Value::Number(1.0)]),
            ]),
        ),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Object 集合函数应成功解析");
        let value: Value = evaluate(&expression).expect("Object 集合函数应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn evaluates_array_collection_builtin_functions() {
    let cases: [(&str, Value); 3] = [
        (
            r#"keys(["a", "b"])"#,
            Value::array(vec![Value::string("0"), Value::string("1")]),
        ),
        (
            r#"values(["a", "b"])"#,
            Value::array(vec![Value::string("a"), Value::string("b")]),
        ),
        (
            r#"entries(["a", "b"])"#,
            Value::array(vec![
                Value::array(vec![Value::string("0"), Value::string("a")]),
                Value::array(vec![Value::string("1"), Value::string("b")]),
            ]),
        ),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Array 集合函数应成功解析");
        let value: Value = evaluate(&expression).expect("Array 集合函数应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn collection_builtin_functions_validate_target_and_arity() {
    for source in ["keys()", "values([], [])", "entries()"] {
        let expression: Expression<'_> = parse(source).expect("集合函数参数数量应成功解析");
        assert_eq!(
            evaluate(&expression).expect_err("集合函数必须只接收一个参数"),
            EvalError::InvalidArgumentCount(expression.span),
            "表达式：{source}"
        );
    }

    let invalid: Expression<'_> = parse(r#"keys("text")"#).expect("非法集合目标应成功解析");
    assert_eq!(
        evaluate(&invalid).expect_err("字符串不应被隐式装箱为集合"),
        EvalError::InvalidCollectionTarget(Span { start: 5, end: 11 })
    );
}

#[test]
fn evaluates_object_has_own_static_function() {
    let cases: [(&str, bool); 5] = [
        (r#"Object.hasOwn({ name: "Narrava" }, "name")"#, true),
        (r#"Object.hasOwn({ name: "Narrava" }, "missing")"#, false),
        (r#"Object.hasOwn({ "1": true }, 1)"#, true),
        (r#"Object.hasOwn({}, "toString")"#, false),
        (r#"Object["hasOwn"]({ own: 1 }, "own")"#, true),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Object.hasOwn 应成功解析");
        let value: Value = evaluate(&expression).expect("Object.hasOwn 应成功求值");

        assert_eq!(value, Value::Boolean(expected), "表达式：{source}");
    }
}

#[test]
fn object_namespace_and_has_own_have_controlled_identity() {
    let cases: [(&str, Value); 5] = [
        ("typeof Object", Value::string("object")),
        ("typeof Object.assign", Value::string("function")),
        ("typeof Object.hasOwn", Value::string("function")),
        ("Object.assign instanceof Function", Value::Boolean(true)),
        ("Object.hasOwn instanceof Function", Value::Boolean(true)),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Object 身份应成功解析");
        assert_eq!(
            evaluate(&expression).expect("Object 身份应成功求值"),
            expected,
            "表达式：{source}"
        );
    }
}

#[test]
fn object_has_own_validates_target_key_and_arity() {
    for source in ["Object.hasOwn({})", "Object.hasOwn({}, \"key\", 1)"] {
        let expression: Expression<'_> = parse(source).expect("hasOwn 参数数量应成功解析");
        assert_eq!(
            evaluate(&expression).expect_err("hasOwn 必须接收两个参数"),
            EvalError::InvalidArgumentCount(expression.span),
            "表达式：{source}"
        );
    }

    let invalid_target: Expression<'_> =
        parse(r#"Object.hasOwn([], "0")"#).expect("非法 Object 目标应成功解析");
    assert_eq!(
        evaluate(&invalid_target).expect_err("Array 不能冒充 Object"),
        EvalError::InvalidObjectTarget(Span { start: 14, end: 16 })
    );

    let invalid_key: Expression<'_> = parse("Object.hasOwn({}, {})").expect("非法属性键应成功解析");
    assert_eq!(
        evaluate(&invalid_key).expect_err("Object 键不得执行宿主转换"),
        EvalError::InvalidStringConversion(Span { start: 18, end: 20 })
    );
}

#[test]
fn object_assign_mutates_target_in_source_order_and_returns_same_reference() {
    let target: Value = Value::object(vec![(String::from("a"), Value::Number(1.0))]);
    let mut context: WritableContext = WritableContext {
        global: None,
        variables: vec![(VariableScope::Variables, String::from("target"), target)],
    };
    let assign: Expression<'_> =
        parse("Object.assign($target, { b: 2, a: 3 }, { c: 4 }) === $target")
            .expect("Object.assign 应成功解析");
    let entries: Expression<'_> = parse("entries($target)").expect("Object.assign 结果应成功解析");

    assert_eq!(
        evaluate_with_mut(&assign, &mut context).expect("Object.assign 应成功修改目标"),
        Value::Boolean(true)
    );
    assert_eq!(
        evaluate_with_mut(&entries, &mut context).expect("Object.assign 结果应可读取"),
        Value::array(vec![
            Value::array(vec![Value::string("a"), Value::Number(3.0)]),
            Value::array(vec![Value::string("b"), Value::Number(2.0)]),
            Value::array(vec![Value::string("c"), Value::Number(4.0)]),
        ])
    );
}

#[test]
fn object_assign_requires_explicit_writable_context() {
    let target: Value = Value::object(vec![(String::from("a"), Value::Number(1.0))]);
    let context: SingleGlobalContext = SingleGlobalContext {
        name: String::from("target"),
        value: target.clone(),
    };
    let expression: Expression<'_> =
        parse("Object.assign(target, { a: 2 })").expect("只读 Object.assign 应成功解析");

    assert_eq!(
        evaluate_with(&expression, &context).expect_err("Object.assign 必须要求可写入口"),
        EvalError::MissingWriteContext(expression.span)
    );
    assert_eq!(
        target,
        Value::object(vec![(String::from("a"), Value::Number(1.0))])
    );
}

#[test]
fn object_assign_validates_arity_target_sources_and_authorization() {
    let missing: Expression<'_> = parse("Object.assign()").expect("空 assign 应成功解析");
    assert_eq!(
        evaluate(&missing).expect_err("Object.assign 至少需要目标"),
        EvalError::InvalidArgumentCount(missing.span)
    );

    let invalid_target: Expression<'_> =
        parse("Object.assign([], {})").expect("非法 assign 目标应成功解析");
    assert_eq!(
        evaluate(&invalid_target).expect_err("Array 不能作为 Object.assign 目标"),
        EvalError::InvalidObjectTarget(Span { start: 14, end: 16 })
    );

    let invalid_source: Expression<'_> =
        parse("Object.assign({}, [])").expect("非法 assign 来源应成功解析");
    assert_eq!(
        evaluate(&invalid_source).expect_err("Array 不能作为 Object.assign 来源"),
        EvalError::InvalidObjectTarget(Span { start: 18, end: 20 })
    );

    let target: Value = Value::object(vec![(String::from("a"), Value::Number(1.0))]);
    let mut context: RejectingGlobalContext = RejectingGlobalContext {
        name: String::from("target"),
        value: target.clone(),
    };
    let rejected: Expression<'_> =
        parse("Object.assign(target, { a: 2 })").expect("拒绝 assign 应成功解析");
    assert_eq!(
        evaluate_with_mut(&rejected, &mut context).expect_err("Context 可以拒绝引用修改"),
        EvalError::ContextWriteRejected(rejected.span)
    );
    assert_eq!(
        target,
        Value::object(vec![(String::from("a"), Value::Number(1.0))])
    );
}

#[test]
fn evaluates_single_argument_numeric_builtin_functions() {
    let cases: [(&str, f64); 12] = [
        ("abs(-2.5)", 2.5),
        ("abs(2.5)", 2.5),
        ("floor(2.9)", 2.0),
        ("floor(-2.1)", -3.0),
        ("ceil(2.1)", 3.0),
        ("ceil(-2.9)", -2.0),
        ("round(1.4)", 1.0),
        ("round(1.5)", 2.0),
        ("round(-1.4)", -1.0),
        ("round(-1.5)", -1.0),
        ("round(2.5)", 3.0),
        ("round(-2.5)", -2.0),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("数值函数应成功解析");
        let value: Value = evaluate(&expression).expect("数值函数应成功求值");

        assert_eq!(value, Value::Number(expected), "表达式：{source}");
    }
}

#[test]
fn web_round_preserves_negative_zero() {
    let expression: Expression<'_> = parse("round(-0.5)").expect("负零舍入应成功解析");
    let value: Value = evaluate(&expression).expect("负零舍入应成功求值");
    let Value::Number(number) = value else {
        panic!("round 必须返回 Number")
    };

    assert_eq!(number, 0.0);
    assert!(number.is_sign_negative());
}

#[test]
fn numeric_builtin_functions_validate_number_and_arity() {
    for source in ["abs()", "floor(1, 2)", "ceil()", "round(1, 2)"] {
        let expression: Expression<'_> = parse(source).expect("数值函数参数数量应成功解析");
        assert_eq!(
            evaluate(&expression).expect_err("一参数数值函数必须只接收一个参数"),
            EvalError::InvalidArgumentCount(expression.span),
            "表达式：{source}"
        );
    }

    let invalid: Expression<'_> = parse(r#"abs("2")"#).expect("字符串数值参数应成功解析");
    assert_eq!(
        evaluate(&invalid).expect_err("数值函数不得隐式转换字符串"),
        EvalError::InvalidNumericArgument(Span { start: 4, end: 7 })
    );
}

#[test]
fn evaluates_min_max_and_clamp_builtin_functions() {
    let cases: [(&str, f64); 8] = [
        ("min(3, -1, 2)", -1.0),
        ("min(4)", 4.0),
        ("max(3, -1, 2)", 3.0),
        ("max(-4)", -4.0),
        ("clamp(5, 0, 10)", 5.0),
        ("clamp(-1, 0, 10)", 0.0),
        ("clamp(11, 0, 10)", 10.0),
        ("clamp(0, 0, 0)", 0.0),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("范围数值函数应成功解析");
        let value: Value = evaluate(&expression).expect("范围数值函数应成功求值");

        assert_eq!(value, Value::Number(expected), "表达式：{source}");
    }
}

#[test]
fn min_max_preserve_nan_and_signed_zero() {
    for source in ["min(1, 0 / 0)", "max(1, 0 / 0)"] {
        let expression: Expression<'_> = parse(source).expect("NaN 聚合应成功解析");
        let Value::Number(number) = evaluate(&expression).expect("NaN 应传播") else {
            panic!("min/max 必须返回 Number")
        };
        assert!(number.is_nan(), "表达式：{source}");
    }

    let minimum: Expression<'_> = parse("min(0, -0)").expect("负零 min 应成功解析");
    let Value::Number(minimum) = evaluate(&minimum).expect("min 应保留负零") else {
        panic!("min 必须返回 Number")
    };
    assert!(minimum.is_sign_negative());

    let maximum: Expression<'_> = parse("max(-0, 0)").expect("正零 max 应成功解析");
    let Value::Number(maximum) = evaluate(&maximum).expect("max 应保留正零") else {
        panic!("max 必须返回 Number")
    };
    assert!(maximum.is_sign_positive());
}

#[test]
fn min_max_and_clamp_validate_arguments_and_bounds() {
    for source in ["min()", "max()", "clamp(1, 0)", "clamp(1, 0, 2, 3)"] {
        let expression: Expression<'_> = parse(source).expect("范围函数参数数量应成功解析");
        assert_eq!(
            evaluate(&expression).expect_err("范围函数参数数量必须受签名约束"),
            EvalError::InvalidArgumentCount(expression.span),
            "表达式：{source}"
        );
    }

    let invalid_number: Expression<'_> = parse(r#"min(1, "2")"#).expect("非法 min 参数应成功解析");
    assert_eq!(
        evaluate(&invalid_number).expect_err("min 不得转换字符串"),
        EvalError::InvalidNumericArgument(Span { start: 7, end: 10 })
    );

    let invalid_bounds: Expression<'_> = parse("clamp(5, 10, 0)").expect("倒置范围应成功解析");
    assert_eq!(
        evaluate(&invalid_bounds).expect_err("clamp 不得自动交换上下界"),
        EvalError::InvalidRange(Span { start: 9, end: 14 })
    );
}

#[test]
fn evaluates_number_string_and_boolean_builtin_conversions() {
    let cases: [(&str, Value); 16] = [
        ("number(null)", Value::Number(0.0)),
        ("number(false)", Value::Number(0.0)),
        ("number(true)", Value::Number(1.0)),
        (r#"number("  ")"#, Value::Number(0.0)),
        (r#"number("0x10")"#, Value::Number(16.0)),
        ("string(undefined)", Value::string("undefined")),
        ("string(null)", Value::string("null")),
        ("string(false)", Value::string("false")),
        ("string(-0)", Value::string("0")),
        ("string(1 / 0)", Value::string("Infinity")),
        ("boolean(undefined)", Value::Boolean(false)),
        ("boolean(null)", Value::Boolean(false)),
        ("boolean(0)", Value::Boolean(false)),
        (r#"boolean("")"#, Value::Boolean(false)),
        ("boolean([])", Value::Boolean(true)),
        ("boolean({})", Value::Boolean(true)),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("转换函数应成功解析");
        let value: Value = evaluate(&expression).expect("转换函数应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn number_conversion_preserves_nan_results() {
    for source in ["number(undefined)", r#"number("Narrava")"#] {
        let expression: Expression<'_> = parse(source).expect("NaN 转换应成功解析");
        let Value::Number(number) = evaluate(&expression).expect("NaN 转换应成功求值")
        else {
            panic!("number 必须返回 Number")
        };

        assert!(number.is_nan(), "表达式：{source}");
    }
}

#[test]
fn conversion_builtin_functions_validate_arity_and_supported_values() {
    for source in ["number()", "string(1, 2)", "boolean()"] {
        let expression: Expression<'_> = parse(source).expect("转换函数参数数量应成功解析");
        assert_eq!(
            evaluate(&expression).expect_err("转换函数必须只接收一个参数"),
            EvalError::InvalidArgumentCount(expression.span),
            "表达式：{source}"
        );
    }

    let number_array: Expression<'_> = parse("number([])").expect("数组转数值应成功解析");
    assert_eq!(
        evaluate(&number_array).expect_err("数组不得隐式转换为数值"),
        EvalError::InvalidNumericConversion(Span { start: 7, end: 9 })
    );

    let string_object: Expression<'_> = parse("string({})").expect("对象转字符串应成功解析");
    assert_eq!(
        evaluate(&string_object).expect_err("对象不得隐式转换为字符串"),
        EvalError::InvalidStringConversion(Span { start: 7, end: 9 })
    );
}

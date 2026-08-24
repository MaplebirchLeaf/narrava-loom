use super::*;

#[test]
fn converts_evaluator_errors_to_stable_diagnostics() {
    let span: Span = Span { start: 2, end: 8 };
    let conversion: Diagnostic = EvalError::InvalidNumericConversion(span).diagnostic();
    let global: Diagnostic = EvalError::UnknownGlobal(span).diagnostic();
    let write: Diagnostic = EvalError::MissingWriteContext(span).diagnostic();

    assert_eq!(conversion.code, "expression.invalid_numeric_conversion");
    assert_eq!(conversion.severity, DiagnosticSeverity::Error);
    assert_eq!(conversion.message, "值无法转换为 Number");
    assert_eq!(conversion.location, None);
    assert_eq!(global.code, "expression.unknown_global");
    assert_eq!(write.code, "expression.missing_write_context");
    assert_eq!(EvalError::NotCallable(span).span(), span);
}

#[test]
fn evaluates_scalar_literals_and_groups() {
    let cases: [(&str, Value); 6] = [
        ("undefined", Value::Undefined),
        ("null", Value::Null),
        ("true", Value::Boolean(true)),
        ("false", Value::Boolean(false)),
        ("12.5", Value::Number(12.5)),
        ("('Narrava')", Value::string("Narrava")),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("基础表达式应成功解析");
        let value: Value = evaluate(&expression).expect("基础表达式应成功求值");
        assert_eq!(value, expected);
    }
}

#[test]
fn evaluates_global_names_through_read_only_context() {
    let context: SingleGlobalContext = SingleGlobalContext {
        name: String::from("score"),
        value: Value::Number(40.0),
    };
    let expression: Expression<'_> =
        parse("{ result: score + 2 }.result").expect("上下文全局名称应成功解析");

    assert_eq!(
        evaluate_with(&expression, &context).expect("嵌套表达式应读取同一上下文"),
        Value::Number(42.0)
    );
}

#[test]
fn builtin_names_take_priority_and_unknown_globals_are_explicit() {
    let context: SingleGlobalContext = SingleGlobalContext {
        name: String::from("defined"),
        value: Value::Number(1.0),
    };
    let builtin: Expression<'_> = parse("typeof defined").expect("保留函数名应成功解析");
    assert_eq!(
        evaluate_with(&builtin, &context).expect("内置函数应优先于外部全局"),
        Value::string("function")
    );

    let missing: Expression<'_> = parse("missing").expect("未知全局应成功解析");
    assert_eq!(
        evaluate_with(&missing, &context).expect_err("未知全局必须明确报错"),
        EvalError::UnknownGlobal(Span { start: 0, end: 7 })
    );
}

#[test]
fn evaluates_setup_and_scoped_variables_through_context() {
    let context: ScopedContext = ScopedContext {
        setup: Value::object(vec![(String::from("bonus"), Value::Number(4.0))]),
        variables: Value::Number(30.0),
        temporary: Value::Number(7.0),
        local: Value::Number(1.0),
    };
    let expression: Expression<'_> =
        parse("$score + _turn + @index + setup.bonus").expect("State 读取应成功解析");

    assert_eq!(
        evaluate_with(&expression, &context).expect("四类只读值应成功求值"),
        Value::Number(42.0)
    );
}

#[test]
fn missing_scoped_variables_are_undefined_but_setup_is_required() {
    let context: ScopedContext = ScopedContext {
        setup: Value::object(Vec::new()),
        variables: Value::Number(1.0),
        temporary: Value::Number(2.0),
        local: Value::Number(3.0),
    };
    let missing: Expression<'_> =
        parse("defined($missing) || defined(_missing) || defined(@missing)")
            .expect("缺失变量判断应成功解析");
    assert_eq!(
        evaluate_with(&missing, &context).expect("缺失变量应作为 undefined 读取"),
        Value::Boolean(false)
    );

    let setup: Expression<'_> = parse("setup").expect("setup 根应成功解析");
    assert_eq!(
        evaluate(&setup).expect_err("空 Context 不提供 setup"),
        EvalError::MissingSetup(Span { start: 0, end: 5 })
    );
}

#[test]
fn evaluates_random_and_either_with_injected_source() {
    let context: SingleGlobalContext = SingleGlobalContext {
        name: String::from("unused"),
        value: Value::Undefined,
    };
    let mut random: FixedRandomSource = FixedRandomSource {
        values: vec![0.25, 0.0, 0.5, 0.999].into_iter(),
    };
    let cases: [(&str, Value); 4] = [
        ("random()", Value::Number(0.25)),
        (r#"either("a", "b", "c")"#, Value::string("a")),
        (r#"either("a", "b", "c")"#, Value::string("b")),
        (r#"either("a", "b", "c")"#, Value::string("c")),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("随机函数应成功解析");
        let value: Value = evaluate_with_random(&expression, &context, &mut random)
            .expect("注入随机源后应成功求值");

        assert_eq!(value, expected, "表达式：{source}");
    }
}

#[test]
fn random_functions_validate_source_and_arity() {
    let expression: Expression<'_> = parse("random()").expect("random 应成功解析");
    assert_eq!(
        evaluate(&expression).expect_err("无随机源时不得使用 random"),
        EvalError::MissingRandomSource(expression.span)
    );

    let context: SingleGlobalContext = SingleGlobalContext {
        name: String::from("unused"),
        value: Value::Undefined,
    };
    for invalid_value in [f64::NAN, -0.1, 1.0] {
        let mut random: FixedRandomSource = FixedRandomSource {
            values: vec![invalid_value].into_iter(),
        };
        assert_eq!(
            evaluate_with_random(&expression, &context, &mut random)
                .expect_err("随机单位必须位于 [0, 1)"),
            EvalError::InvalidRandomValue(expression.span)
        );
    }

    for source in ["random(1)", "either()"] {
        let expression: Expression<'_> = parse(source).expect("随机函数参数数量应成功解析");
        assert_eq!(
            evaluate(&expression).expect_err("随机函数参数数量必须受签名约束"),
            EvalError::InvalidArgumentCount(expression.span),
            "表达式：{source}"
        );
    }
}

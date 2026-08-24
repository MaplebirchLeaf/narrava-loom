use super::*;

#[test]
fn evaluates_array_and_string_length_members() {
    let cases: [(&str, f64); 5] = [
        ("[].length", 0.0),
        ("[1, 2, 3].length", 3.0),
        (r#""Narrava".length"#, 7.0),
        (r#""😀".length"#, 2.0),
        (r#""".length"#, 0.0),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("length 属性应成功解析");
        let value: Value = evaluate(&expression).expect("length 属性应成功求值");

        assert_eq!(value, Value::Number(expected), "表达式：{source}");
    }
}

#[test]
fn array_push_appends_values_returns_length_and_updates_aliases() {
    let items: Value = Value::array(vec![Value::Number(1.0)]);
    let mut context: WritableContext = WritableContext {
        global: None,
        variables: vec![
            (
                VariableScope::Variables,
                String::from("items"),
                items.clone(),
            ),
            (VariableScope::Variables, String::from("alias"), items),
        ],
    };
    let push: Expression<'_> =
        parse("$items.push(2, { ready: true })").expect("Array.push 应成功解析");
    let alias: Expression<'_> = parse("$alias").expect("Array 别名应成功解析");
    let empty_push: Expression<'_> = parse("[].push()").expect("空 push 应成功解析");

    assert_eq!(
        evaluate_with_mut(&push, &mut context).expect("Array.push 应成功追加"),
        Value::Number(3.0)
    );
    assert_eq!(
        evaluate_with_mut(&alias, &mut context).expect("Array 别名应观察到 push"),
        Value::array(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::object(vec![(String::from("ready"), Value::Boolean(true))]),
        ])
    );
    assert_eq!(
        evaluate_with_mut(&empty_push, &mut context).expect("零参数 push 应返回当前长度"),
        Value::Number(0.0)
    );
}

#[test]
fn array_push_requires_reference_write_authorization() {
    let items: Value = Value::array(vec![Value::Number(1.0)]);
    let read_only: SingleGlobalContext = SingleGlobalContext {
        name: String::from("items"),
        value: items.clone(),
    };
    let expression: Expression<'_> = parse("items.push(2)").expect("只读 push 应成功解析");
    assert_eq!(
        evaluate_with(&expression, &read_only).expect_err("Array.push 必须要求可写入口"),
        EvalError::MissingWriteContext(expression.span)
    );
    assert_eq!(items, Value::array(vec![Value::Number(1.0)]));

    let mut rejected: RejectingGlobalContext = RejectingGlobalContext {
        name: String::from("items"),
        value: items.clone(),
    };
    assert_eq!(
        evaluate_with_mut(&expression, &mut rejected).expect_err("Context 可以拒绝 Array.push"),
        EvalError::ContextWriteRejected(expression.span)
    );
    assert_eq!(items, Value::array(vec![Value::Number(1.0)]));
}

#[test]
fn array_pop_removes_and_returns_last_value_for_all_aliases() {
    let last: Value = Value::object(vec![(String::from("ready"), Value::Boolean(true))]);
    let items: Value = Value::array(vec![Value::Number(1.0), last.clone()]);
    let mut context: WritableContext = WritableContext {
        global: None,
        variables: vec![
            (
                VariableScope::Variables,
                String::from("items"),
                items.clone(),
            ),
            (VariableScope::Variables, String::from("alias"), items),
        ],
    };
    let pop: Expression<'_> = parse("$items.pop()").expect("Array.pop 应成功解析");
    let alias: Expression<'_> = parse("$alias").expect("Array 别名应成功解析");

    assert_eq!(
        evaluate_with_mut(&pop, &mut context).expect("Array.pop 应返回末项"),
        last
    );
    assert_eq!(
        evaluate_with_mut(&alias, &mut context).expect("Array 别名应观察到 pop"),
        Value::array(vec![Value::Number(1.0)])
    );
    assert_eq!(
        evaluate_with_mut(&pop, &mut context).expect("Array.pop 应返回剩余末项"),
        Value::Number(1.0)
    );
    assert_eq!(
        evaluate_with_mut(&pop, &mut context).expect("空 Array.pop 应成功"),
        Value::Undefined
    );
}

#[test]
fn array_pop_validates_arity_and_reference_write_authorization() {
    let extra: Expression<'_> = parse("[1].pop(2)").expect("额外参数 pop 应成功解析");
    assert_eq!(
        evaluate(&extra).expect_err("Array.pop 不接受参数"),
        EvalError::InvalidArgumentCount(extra.span)
    );

    let items: Value = Value::array(vec![Value::Number(1.0)]);
    let read_only: SingleGlobalContext = SingleGlobalContext {
        name: String::from("items"),
        value: items.clone(),
    };
    let expression: Expression<'_> = parse("items.pop()").expect("只读 pop 应成功解析");
    assert_eq!(
        evaluate_with(&expression, &read_only).expect_err("Array.pop 必须要求可写入口"),
        EvalError::MissingWriteContext(expression.span)
    );

    let mut rejected: RejectingGlobalContext = RejectingGlobalContext {
        name: String::from("items"),
        value: items.clone(),
    };
    assert_eq!(
        evaluate_with_mut(&expression, &mut rejected).expect_err("Context 可以拒绝 Array.pop"),
        EvalError::ContextWriteRejected(expression.span)
    );
    assert_eq!(items, Value::array(vec![Value::Number(1.0)]));
}

#[test]
fn array_shift_removes_and_returns_first_value_for_all_aliases() {
    let first: Value = Value::object(vec![(String::from("ready"), Value::Boolean(true))]);
    let items: Value = Value::array(vec![first.clone(), Value::Number(2.0)]);
    let mut context: WritableContext = WritableContext {
        global: None,
        variables: vec![
            (
                VariableScope::Variables,
                String::from("items"),
                items.clone(),
            ),
            (VariableScope::Variables, String::from("alias"), items),
        ],
    };
    let shift: Expression<'_> = parse("$items.shift()").expect("Array.shift 应成功解析");
    let alias: Expression<'_> = parse("$alias").expect("Array 别名应成功解析");

    assert_eq!(
        evaluate_with_mut(&shift, &mut context).expect("Array.shift 应返回首项"),
        first
    );
    assert_eq!(
        evaluate_with_mut(&alias, &mut context).expect("Array 别名应观察到 shift"),
        Value::array(vec![Value::Number(2.0)])
    );
    assert_eq!(
        evaluate_with_mut(&shift, &mut context).expect("Array.shift 应返回剩余首项"),
        Value::Number(2.0)
    );
    assert_eq!(
        evaluate_with_mut(&shift, &mut context).expect("空 Array.shift 应成功"),
        Value::Undefined
    );
}

#[test]
fn array_shift_validates_arity_and_reference_write_authorization() {
    let extra: Expression<'_> = parse("[1].shift(2)").expect("额外参数 shift 应成功解析");
    assert_eq!(
        evaluate(&extra).expect_err("Array.shift 不接受参数"),
        EvalError::InvalidArgumentCount(extra.span)
    );

    let items: Value = Value::array(vec![Value::Number(1.0)]);
    let read_only: SingleGlobalContext = SingleGlobalContext {
        name: String::from("items"),
        value: items.clone(),
    };
    let expression: Expression<'_> = parse("items.shift()").expect("只读 shift 应成功解析");
    assert_eq!(
        evaluate_with(&expression, &read_only).expect_err("Array.shift 必须要求可写入口"),
        EvalError::MissingWriteContext(expression.span)
    );

    let mut rejected: RejectingGlobalContext = RejectingGlobalContext {
        name: String::from("items"),
        value: items.clone(),
    };
    assert_eq!(
        evaluate_with_mut(&expression, &mut rejected).expect_err("Context 可以拒绝 Array.shift"),
        EvalError::ContextWriteRejected(expression.span)
    );
    assert_eq!(items, Value::array(vec![Value::Number(1.0)]));
}

#[test]
fn array_unshift_prepends_values_in_argument_order_and_returns_length() {
    let items: Value = Value::array(vec![Value::Number(3.0)]);
    let mut context: WritableContext = WritableContext {
        global: None,
        variables: vec![
            (
                VariableScope::Variables,
                String::from("items"),
                items.clone(),
            ),
            (VariableScope::Variables, String::from("alias"), items),
        ],
    };
    let unshift: Expression<'_> = parse("$items.unshift(1, 2)").expect("Array.unshift 应成功解析");
    let alias: Expression<'_> = parse("$alias").expect("Array 别名应成功解析");
    let empty_unshift: Expression<'_> = parse("[].unshift()").expect("空 unshift 应成功解析");

    assert_eq!(
        evaluate_with_mut(&unshift, &mut context).expect("Array.unshift 应成功插入"),
        Value::Number(3.0)
    );
    assert_eq!(
        evaluate_with_mut(&alias, &mut context).expect("Array 别名应观察到 unshift"),
        Value::array(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0),
        ])
    );
    assert_eq!(
        evaluate_with_mut(&empty_unshift, &mut context).expect("零参数 unshift 应返回当前长度"),
        Value::Number(0.0)
    );
}

#[test]
fn array_unshift_requires_reference_write_authorization() {
    let items: Value = Value::array(vec![Value::Number(2.0)]);
    let read_only: SingleGlobalContext = SingleGlobalContext {
        name: String::from("items"),
        value: items.clone(),
    };
    let expression: Expression<'_> = parse("items.unshift(1)").expect("只读 unshift 应成功解析");
    assert_eq!(
        evaluate_with(&expression, &read_only).expect_err("Array.unshift 必须要求可写入口"),
        EvalError::MissingWriteContext(expression.span)
    );

    let mut rejected: RejectingGlobalContext = RejectingGlobalContext {
        name: String::from("items"),
        value: items.clone(),
    };
    assert_eq!(
        evaluate_with_mut(&expression, &mut rejected).expect_err("Context 可以拒绝 Array.unshift"),
        EvalError::ContextWriteRejected(expression.span)
    );
    assert_eq!(items, Value::array(vec![Value::Number(2.0)]));
}

#[test]
fn array_splice_removes_inserts_and_returns_removed_values() {
    let referenced: Value = Value::object(vec![(String::from("id"), Value::Number(3.0))]);
    let items: Value = Value::array(vec![
        Value::Number(1.0),
        Value::Number(2.0),
        referenced.clone(),
        Value::Number(4.0),
    ]);
    let mut context: WritableContext = WritableContext {
        global: None,
        variables: vec![(VariableScope::Variables, String::from("items"), items)],
    };
    let replace: Expression<'_> =
        parse(r#"$items.splice(1, 2, "a", "b")"#).expect("Array.splice 替换应成功解析");
    let remove_tail: Expression<'_> =
        parse("$items.splice(-1)").expect("Array.splice 负起点应成功解析");
    let insert: Expression<'_> =
        parse("$items.splice(1, 0, 9)").expect("Array.splice 插入应成功解析");
    let read: Expression<'_> = parse("$items").expect("Array.splice 结果应成功解析");

    assert_eq!(
        evaluate_with_mut(&replace, &mut context).expect("Array.splice 应返回被删除项"),
        Value::array(vec![Value::Number(2.0), referenced])
    );
    assert_eq!(
        evaluate_with_mut(&read, &mut context).expect("Array.splice 替换结果应可读取"),
        Value::array(vec![
            Value::Number(1.0),
            Value::string("a"),
            Value::string("b"),
            Value::Number(4.0),
        ])
    );
    assert_eq!(
        evaluate_with_mut(&remove_tail, &mut context).expect("省略删除数应删除到末尾"),
        Value::array(vec![Value::Number(4.0)])
    );
    assert_eq!(
        evaluate_with_mut(&insert, &mut context).expect("零删除数应只插入"),
        Value::array(Vec::new())
    );
    assert_eq!(
        evaluate_with_mut(&read, &mut context).expect("Array.splice 最终结果应可读取"),
        Value::array(vec![
            Value::Number(1.0),
            Value::Number(9.0),
            Value::string("a"),
            Value::string("b"),
        ])
    );
}

#[test]
fn array_splice_handles_omitted_arguments_and_web_numeric_bounds() {
    let mut context: WritableContext = WritableContext {
        global: None,
        variables: Vec::new(),
    };
    let cases: [(&str, Value); 4] = [
        ("[1, 2].splice()", Value::array(Vec::new())),
        (
            r#"[1, 2, 3].splice("1", "Infinity")"#,
            Value::array(vec![Value::Number(2.0), Value::Number(3.0)]),
        ),
        (
            "[1, 2, 3].splice(-99, 1)",
            Value::array(vec![Value::Number(1.0)]),
        ),
        ("[1, 2, 3].splice(99, 1)", Value::array(Vec::new())),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Array.splice 边界应成功解析");
        assert_eq!(
            evaluate_with_mut(&expression, &mut context).expect("Array.splice 边界应成功求值"),
            expected,
            "表达式：{source}"
        );
    }
}

#[test]
fn array_splice_validates_numeric_arguments_and_reference_write_authorization() {
    let invalid_start: Expression<'_> =
        parse("[1].splice({})").expect("非法 splice 起点应成功解析");
    assert_eq!(
        evaluate(&invalid_start).expect_err("Object 不能转换为 splice 起点"),
        EvalError::InvalidNumericConversion(Span { start: 11, end: 13 })
    );

    let invalid_count: Expression<'_> =
        parse("[1].splice(0, {})").expect("非法 splice 删除数应成功解析");
    assert_eq!(
        evaluate(&invalid_count).expect_err("Object 不能转换为 splice 删除数"),
        EvalError::InvalidNumericConversion(Span { start: 14, end: 16 })
    );

    let items: Value = Value::array(vec![Value::Number(1.0)]);
    let read_only: SingleGlobalContext = SingleGlobalContext {
        name: String::from("items"),
        value: items.clone(),
    };
    let expression: Expression<'_> = parse("items.splice(0, 1)").expect("只读 splice 应成功解析");
    assert_eq!(
        evaluate_with(&expression, &read_only).expect_err("Array.splice 必须要求可写入口"),
        EvalError::MissingWriteContext(expression.span)
    );

    let mut rejected: RejectingGlobalContext = RejectingGlobalContext {
        name: String::from("items"),
        value: items.clone(),
    };
    assert_eq!(
        evaluate_with_mut(&expression, &mut rejected).expect_err("Context 可以拒绝 Array.splice"),
        EvalError::ContextWriteRejected(expression.span)
    );
    assert_eq!(items, Value::array(vec![Value::Number(1.0)]));
}

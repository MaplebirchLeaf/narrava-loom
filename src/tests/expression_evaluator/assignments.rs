use super::*;

#[test]
fn direct_assignment_writes_global_and_scoped_variables() {
    let mut context: WritableContext = WritableContext {
        global: Some((String::from("score"), Value::Number(40.0))),
        variables: vec![
            (
                VariableScope::Variables,
                String::from("score"),
                Value::Number(1.0),
            ),
            (
                VariableScope::Temporary,
                String::from("turn"),
                Value::Number(2.0),
            ),
        ],
    };
    let cases: [(&str, Value); 4] = [
        ("score = score + 2", Value::Number(42.0)),
        ("$score = 10", Value::Number(10.0)),
        ("_turn = $score + 1", Value::Number(11.0)),
        ("@index = _turn + 1", Value::Number(12.0)),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("直接赋值应成功解析");
        assert_eq!(
            evaluate_with_mut(&expression, &mut context).expect("直接赋值应成功写入"),
            expected,
            "表达式：{source}"
        );
    }

    assert_eq!(context.global("score"), Some(&Value::Number(42.0)));
    assert_eq!(
        context.variable(VariableScope::Local, "index"),
        Some(&Value::Number(12.0))
    );
}

#[test]
fn direct_assignment_requires_write_context_and_rejects_reserved_names() {
    let assignment: Expression<'_> = parse("score = 1").expect("全局赋值应成功解析");
    assert_eq!(
        evaluate(&assignment).expect_err("只读入口不得执行赋值"),
        EvalError::MissingWriteContext(Span { start: 0, end: 5 })
    );

    let mut context: WritableContext = WritableContext {
        global: None,
        variables: Vec::new(),
    };
    let reserved: [(&str, Span); 2] = [
        ("defined = 1", Span { start: 0, end: 7 }),
        ("Object = 1", Span { start: 0, end: 6 }),
    ];
    for (source, target_span) in reserved {
        let expression: Expression<'_> = parse(source).expect("保留名称赋值应成功解析");
        assert_eq!(
            evaluate_with_mut(&expression, &mut context).expect_err("引擎保留名称不得被覆盖"),
            EvalError::ReservedGlobal(target_span),
            "表达式：{source}"
        );
    }
}

#[test]
fn compound_assignment_reuses_binary_operator_semantics() {
    let cases: [(Value, &str, Value); 15] = [
        (Value::Number(2.0), "$value += 3", Value::Number(5.0)),
        (
            Value::string("Nar"),
            r#"$value += "rava""#,
            Value::string("Narrava"),
        ),
        (Value::Number(5.0), "$value -= 3", Value::Number(2.0)),
        (Value::Number(4.0), "$value *= 3", Value::Number(12.0)),
        (Value::Number(8.0), "$value /= 2", Value::Number(4.0)),
        (Value::Number(7.0), "$value //= 2", Value::Number(3.0)),
        (Value::Number(7.0), "$value %= 4", Value::Number(3.0)),
        (Value::Number(2.0), "$value **= 3", Value::Number(8.0)),
        (Value::Number(3.0), "$value <<= 2", Value::Number(12.0)),
        (Value::Number(8.0), "$value >>= 2", Value::Number(2.0)),
        (
            Value::Number(-1.0),
            "$value >>>= 1",
            Value::Number(2_147_483_647.0),
        ),
        (Value::Number(6.0), "$value &= 3", Value::Number(2.0)),
        (Value::Number(6.0), "$value ^= 3", Value::Number(5.0)),
        (Value::Number(6.0), "$value |= 3", Value::Number(7.0)),
        (Value::Number(1.0), "$value += true", Value::Number(2.0)),
    ];

    for (initial, source, expected) in cases {
        let mut context: WritableContext = WritableContext {
            global: None,
            variables: vec![(VariableScope::Variables, String::from("value"), initial)],
        };
        let expression: Expression<'_> = parse(source).expect("复合赋值应成功解析");
        let result: Value =
            evaluate_with_mut(&expression, &mut context).expect("复合赋值应成功写回");

        assert_eq!(result, expected, "表达式：{source}");
        assert_eq!(
            context.variable(VariableScope::Variables, "value"),
            Some(&expected),
            "表达式：{source}"
        );
    }
}

#[test]
fn logical_compound_assignment_short_circuits_read_and_write() {
    let cases: [(Value, &str, Value); 6] = [
        (
            Value::Boolean(false),
            "$value &&= missing",
            Value::Boolean(false),
        ),
        (
            Value::Boolean(true),
            "$value ||= missing",
            Value::Boolean(true),
        ),
        (Value::Number(1.0), "$value ??= missing", Value::Number(1.0)),
        (Value::Boolean(true), "$value &&= 5", Value::Number(5.0)),
        (Value::Boolean(false), "$value ||= 6", Value::Number(6.0)),
        (Value::Null, "$value ??= 7", Value::Number(7.0)),
    ];

    for (initial, source, expected) in cases {
        let mut context: WritableContext = WritableContext {
            global: None,
            variables: vec![(VariableScope::Variables, String::from("value"), initial)],
        };
        let expression: Expression<'_> = parse(source).expect("逻辑复合赋值应成功解析");
        let result: Value =
            evaluate_with_mut(&expression, &mut context).expect("短路复合赋值应遵守选择规则");

        assert_eq!(result, expected, "表达式：{source}");
        assert_eq!(
            context.variable(VariableScope::Variables, "value"),
            Some(&expected),
            "表达式：{source}"
        );
    }
}

#[test]
fn prefix_and_postfix_updates_write_once_and_return_expected_value() {
    let cases: [(Value, &str, Value, Value); 6] = [
        (
            Value::Number(4.0),
            "++$value",
            Value::Number(5.0),
            Value::Number(5.0),
        ),
        (
            Value::Number(4.0),
            "$value++",
            Value::Number(4.0),
            Value::Number(5.0),
        ),
        (
            Value::Number(4.0),
            "--$value",
            Value::Number(3.0),
            Value::Number(3.0),
        ),
        (
            Value::Number(4.0),
            "$value--",
            Value::Number(4.0),
            Value::Number(3.0),
        ),
        (
            Value::string("4"),
            "++$value",
            Value::Number(5.0),
            Value::Number(5.0),
        ),
        (
            Value::Boolean(true),
            "$value++",
            Value::Number(1.0),
            Value::Number(2.0),
        ),
    ];

    for (initial, source, returned, stored) in cases {
        let mut context: WritableContext = WritableContext {
            global: None,
            variables: vec![(VariableScope::Variables, String::from("value"), initial)],
        };
        let expression: Expression<'_> = parse(source).expect("更新表达式应成功解析");
        let result: Value =
            evaluate_with_mut(&expression, &mut context).expect("更新表达式应成功写回");

        assert_eq!(result, returned, "表达式：{source}");
        assert_eq!(
            context.variable(VariableScope::Variables, "value"),
            Some(&stored),
            "表达式：{source}"
        );
    }
}

#[test]
fn updates_support_globals_and_require_numeric_writable_targets() {
    let mut context: WritableContext = WritableContext {
        global: Some((String::from("score"), Value::Number(1.0))),
        variables: Vec::new(),
    };
    let global: Expression<'_> = parse("score++").expect("全局更新应成功解析");
    assert_eq!(
        evaluate_with_mut(&global, &mut context).expect("全局更新应成功写回"),
        Value::Number(1.0)
    );
    assert_eq!(context.global("score"), Some(&Value::Number(2.0)));

    let read_only: Expression<'_> = parse("$score++").expect("只读更新应成功解析");
    assert_eq!(
        evaluate(&read_only).expect_err("只读入口不得执行更新"),
        EvalError::MissingWriteContext(Span { start: 0, end: 6 })
    );

    let invalid: Expression<'_> = parse("++$object").expect("对象更新应成功解析");
    context.variables.push((
        VariableScope::Variables,
        String::from("object"),
        Value::object(Vec::new()),
    ));
    assert_eq!(
        evaluate_with_mut(&invalid, &mut context).expect_err("Object 不得数值化更新"),
        EvalError::InvalidNumericConversion(Span { start: 2, end: 9 })
    );
}

#[test]
fn object_member_targets_write_back_through_global_or_variable_root() {
    let global_player: Value = Value::object(vec![
        (String::from("name"), Value::string("Narrava")),
        (
            String::from("stats"),
            Value::object(vec![(String::from("score"), Value::Number(10.0))]),
        ),
    ]);
    let variable_player: Value = Value::object(vec![
        (String::from("name"), Value::string("Narrava")),
        (
            String::from("stats"),
            Value::object(vec![(String::from("score"), Value::Number(10.0))]),
        ),
    ]);
    let mut context: WritableContext = WritableContext {
        global: Some((String::from("player"), global_player)),
        variables: vec![(
            VariableScope::Variables,
            String::from("player"),
            variable_player,
        )],
    };
    let cases: [(&str, Value); 5] = [
        ("player.stats.score += 2", Value::Number(12.0)),
        ("$player.stats.score++", Value::Number(10.0)),
        ("++$player.stats.score", Value::Number(12.0)),
        ("$player.level = 3", Value::Number(3.0)),
        (r#"player.name = "Loom""#, Value::string("Loom")),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("Object 成员写入应成功解析");
        assert_eq!(
            evaluate_with_mut(&expression, &mut context).expect("Object 路径应成功写回"),
            expected,
            "表达式：{source}"
        );
    }

    let global_score: Expression<'_> = parse("player.stats.score").expect("全局嵌套成员应成功解析");
    assert_eq!(
        evaluate_with_mut(&global_score, &mut context).expect("全局根应保留写回结果"),
        Value::Number(12.0)
    );
    let variable_level: Expression<'_> = parse("$player.level").expect("变量新增成员应成功解析");
    assert_eq!(
        evaluate_with_mut(&variable_level, &mut context).expect("新增属性应保留"),
        Value::Number(3.0)
    );
}

#[test]
fn object_member_target_requires_writable_root_and_existing_intermediate_object() {
    let mut context: WritableContext = WritableContext {
        global: None,
        variables: vec![(
            VariableScope::Variables,
            String::from("player"),
            Value::object(Vec::new()),
        )],
    };
    let missing_intermediate: Expression<'_> =
        parse("$player.stats.score = 1").expect("缺失中间属性应成功解析");
    assert_eq!(
        evaluate_with_mut(&missing_intermediate, &mut context)
            .expect_err("中间 Object 属性必须已存在"),
        EvalError::UnknownMember(Span { start: 8, end: 13 })
    );

    let temporary: Expression<'_> =
        parse("({ score: 1 }).score = 2").expect("临时对象成员写入应成功解析");
    assert_eq!(
        evaluate_with_mut(&temporary, &mut context).expect_err("没有 Context 根的临时对象不可写回"),
        EvalError::UnsupportedExpression(Span { start: 1, end: 13 })
    );
}

#[test]
fn object_and_array_index_targets_share_root_path_writeback() {
    let data: Value = Value::object(vec![
        (String::from("score"), Value::Number(1.0)),
        (
            String::from("items"),
            Value::array(vec![Value::Number(2.0), Value::Number(4.0)]),
        ),
    ]);
    let mut context: WritableContext = WritableContext {
        global: None,
        variables: vec![
            (VariableScope::Variables, String::from("data"), data),
            (
                VariableScope::Local,
                String::from("key"),
                Value::string("score"),
            ),
            (
                VariableScope::Local,
                String::from("index"),
                Value::Number(0.0),
            ),
        ],
    };
    let cases: [(&str, Value); 4] = [
        ("$data[@key] += 2", Value::Number(3.0)),
        ("$data.items[1] += 5", Value::Number(9.0)),
        ("$data.items[@index++]++", Value::Number(2.0)),
        ("$data.items[2] = 12", Value::Number(12.0)),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("索引写入应成功解析");
        assert_eq!(
            evaluate_with_mut(&expression, &mut context).expect("索引路径应成功写回"),
            expected,
            "表达式：{source}"
        );
    }

    assert_eq!(
        context.variable(VariableScope::Local, "index"),
        Some(&Value::Number(1.0)),
        "动态索引表达式必须只求值一次"
    );
    let expected: Value = Value::object(vec![
        (String::from("score"), Value::Number(3.0)),
        (
            String::from("items"),
            Value::array(vec![
                Value::Number(3.0),
                Value::Number(9.0),
                Value::Number(12.0),
            ]),
        ),
    ]);
    assert_eq!(
        context.variable(VariableScope::Variables, "data"),
        Some(&expected)
    );
}

#[test]
fn deletes_root_bindings_and_object_members_without_writing_undefined() {
    let data: Value = Value::object(vec![
        (String::from("name"), Value::string("Narrava")),
        (
            String::from("nested"),
            Value::object(vec![(String::from("score"), Value::Number(3.0))]),
        ),
    ]);
    let mut context: WritableContext = WritableContext {
        global: Some((String::from("temporaryApi"), Value::Boolean(true))),
        variables: vec![(VariableScope::Variables, String::from("data"), data)],
    };
    let global: Expression<'_> = parse("temporaryApi").expect("global 删除目标应可解析");
    let member: Expression<'_> = parse("$data.nested.score").expect("Object 删除目标应可解析");

    let global_value: Option<Value> =
        delete_with_mut(&global, &mut context).expect("global 根绑定应可删除");
    let member_value: Option<Value> =
        delete_with_mut(&member, &mut context).expect("Object 成员应可删除");

    assert_eq!(global_value, Some(Value::Boolean(true)));
    assert_eq!(member_value, Some(Value::Number(3.0)));
    assert_eq!(context.global("temporaryApi"), None);
    let read: Expression<'_> = parse("$data.nested.score").expect("成员读取应可解析");
    assert_eq!(
        evaluate_with_mut(&read, &mut context).expect_err("属性应已真正删除"),
        EvalError::UnknownMember(Span { start: 13, end: 18 })
    );
}

#[test]
fn rejects_array_element_deletion_to_preserve_dense_arrays() {
    let mut context: WritableContext = WritableContext {
        global: None,
        variables: vec![(
            VariableScope::Variables,
            String::from("items"),
            Value::array(vec![Value::Number(1.0), Value::Number(2.0)]),
        )],
    };
    let target: Expression<'_> = parse("$items[0]").expect("Array 删除目标应可解析");

    let error: EvalError =
        delete_with_mut(&target, &mut context).expect_err("Array 删除不能制造空洞");

    assert_eq!(
        error,
        EvalError::InvalidDeleteTarget(Span { start: 7, end: 8 })
    );
    assert_eq!(
        context.variable(VariableScope::Variables, "items"),
        Some(&Value::array(vec![Value::Number(1.0), Value::Number(2.0)]))
    );
}

#[test]
fn rejected_member_deletion_does_not_mutate_shared_object() {
    let object: Value = Value::object(vec![(String::from("score"), Value::Number(3.0))]);
    let alias: Value = object.clone();
    let mut context: RejectingGlobalContext = RejectingGlobalContext {
        name: String::from("data"),
        value: object,
    };
    let target: Expression<'_> = parse("data.score").expect("Object 删除目标应可解析");

    let error: EvalError =
        delete_with_mut(&target, &mut context).expect_err("Context 应拒绝根提交");

    assert_eq!(
        error,
        EvalError::ContextWriteRejected(Span { start: 0, end: 4 })
    );
    assert_eq!(
        alias,
        Value::object(vec![(String::from("score"), Value::Number(3.0))])
    );
}

#[test]
fn array_index_target_rejects_noncanonical_or_sparse_positions() {
    let mut context: WritableContext = WritableContext {
        global: None,
        variables: vec![(
            VariableScope::Variables,
            String::from("items"),
            Value::array(vec![Value::Number(1.0)]),
        )],
    };
    let cases: [(&str, Span); 3] = [
        ("$items[-1] = 2", Span { start: 7, end: 9 }),
        (r#"$items["01"] = 2"#, Span { start: 7, end: 11 }),
        ("$items[3] = 2", Span { start: 7, end: 8 }),
    ];

    for (source, index_span) in cases {
        let expression: Expression<'_> = parse(source).expect("非法 Array 索引应成功解析");
        assert_eq!(
            evaluate_with_mut(&expression, &mut context)
                .expect_err("Array 不得执行非规范或跨空洞写入"),
            EvalError::InvalidArrayIndex(index_span),
            "表达式：{source}"
        );
    }
}

#[test]
fn cloned_array_values_share_index_mutation() {
    let array: Value = Value::array(vec![Value::Number(1.0)]);
    let mut context: WritableContext = WritableContext {
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
    let write: Expression<'_> = parse("$left[0] = 2").expect("Array 写入应成功解析");
    let read: Expression<'_> = parse("$right[0]").expect("Array 别名读取应成功解析");

    evaluate_with_mut(&write, &mut context).expect("Array 别名写入应成功");

    assert_eq!(
        evaluate_with_mut(&read, &mut context).expect("Array 别名应观察到修改"),
        Value::Number(2.0)
    );
}

#[test]
fn cloned_object_values_share_member_mutation() {
    let object: Value = Value::object(vec![(String::from("score"), Value::Number(1.0))]);
    let mut context: WritableContext = WritableContext {
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
    let write: Expression<'_> = parse("$left.score = 2").expect("Object 写入应成功解析");
    let read: Expression<'_> = parse("$right.score").expect("Object 别名读取应成功解析");

    evaluate_with_mut(&write, &mut context).expect("Object 别名写入应成功");

    assert_eq!(
        evaluate_with_mut(&read, &mut context).expect("Object 别名应观察到修改"),
        Value::Number(2.0)
    );
}

#[test]
fn read_only_context_does_not_mutate_shared_object_before_error() {
    let object: Value = Value::object(vec![(String::from("score"), Value::Number(1.0))]);
    let context: SingleGlobalContext = SingleGlobalContext {
        name: String::from("player"),
        value: object.clone(),
    };
    let expression: Expression<'_> = parse("player.score = 2").expect("只读 Object 写入应成功解析");

    assert_eq!(
        evaluate_with(&expression, &context).expect_err("只读 Context 必须拒绝 Object 写入"),
        EvalError::MissingWriteContext(Span { start: 0, end: 6 })
    );
    assert_eq!(
        object,
        Value::object(vec![(String::from("score"), Value::Number(1.0))])
    );
}

#[test]
fn rejecting_context_does_not_mutate_shared_array_before_error() {
    let array: Value = Value::array(vec![Value::Number(1.0)]);
    let mut context: RejectingGlobalContext = RejectingGlobalContext {
        name: String::from("items"),
        value: array.clone(),
    };
    let expression: Expression<'_> = parse("items[0] = 2").expect("拒绝 Array 写入应成功解析");

    assert_eq!(
        evaluate_with_mut(&expression, &mut context)
            .expect_err("Context 拒绝后不得保留 Array 修改"),
        EvalError::ContextWriteRejected(Span { start: 0, end: 5 })
    );
    assert_eq!(array, Value::array(vec![Value::Number(1.0)]));
}

#[test]
fn setup_member_and_index_paths_write_back_through_dedicated_root() {
    let mut context: WritableSetupContext = WritableSetupContext {
        setup: Value::object(vec![
            (
                String::from("audio"),
                Value::object(vec![(String::from("volume"), Value::Number(5.0))]),
            ),
            (
                String::from("flags"),
                Value::object(vec![(String::from("ready"), Value::Boolean(false))]),
            ),
            (
                String::from("items"),
                Value::array(vec![Value::Number(1.0)]),
            ),
        ]),
    };
    let cases: [(&str, Value); 4] = [
        ("setup.audio.volume += 2", Value::Number(7.0)),
        (r#"setup["flags"].ready = true"#, Value::Boolean(true)),
        ("setup.items[0]++", Value::Number(1.0)),
        ("setup.items[1] = 3", Value::Number(3.0)),
    ];

    for (source, expected) in cases {
        let expression: Expression<'_> = parse(source).expect("setup 路径写入应成功解析");
        assert_eq!(
            evaluate_with_mut(&expression, &mut context).expect("setup 应通过专用根写回"),
            expected,
            "表达式：{source}"
        );
    }

    let volume: Expression<'_> = parse("setup.audio.volume").expect("setup 写回结果应成功解析");
    assert_eq!(
        evaluate_with_mut(&volume, &mut context).expect("setup 写回结果应可读取"),
        Value::Number(7.0)
    );
}

#[test]
fn setup_path_requires_writable_context() {
    let context: ScopedContext = ScopedContext {
        setup: Value::object(vec![(String::from("value"), Value::Number(1.0))]),
        variables: Value::Undefined,
        temporary: Value::Undefined,
        local: Value::Undefined,
    };
    let expression: Expression<'_> = parse("setup.value = 2").expect("setup 写入应成功解析");
    assert_eq!(
        evaluate_with(&expression, &context).expect_err("只读 Context 不得写 setup"),
        EvalError::MissingWriteContext(Span { start: 0, end: 5 })
    );
}

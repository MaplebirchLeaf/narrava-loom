use super::*;

#[test]
fn stores_macro_structure_and_execution_metadata() {
    let mut definitions: MacroDefinitions<MacroDefinition<&str>> = MacroDefinitions::new();
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Container,
        MacroArgumentKind::ArgumentList,
        MacroExecutionKind::Async,
        "handler",
    );

    let _previous: Option<MacroDefinition<&str>> = definitions.add("fetch", definition);
    let stored: &MacroDefinition<&str> = definitions.get("fetch").expect("定义应已注册");

    assert_eq!(stored.body_kind, MacroBodyKind::Container);
    assert_eq!(stored.argument_kind, MacroArgumentKind::ArgumentList);
    assert_eq!(stored.execution_kind, MacroExecutionKind::Async);
    assert_eq!(stored.handler, "handler");
}

#[test]
fn stores_raw_argument_format_for_special_macro_syntax() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Container,
        MacroArgumentKind::Raw,
        MacroExecutionKind::Sync,
        "link-handler",
    );

    assert_eq!(definition.argument_kind, MacroArgumentKind::Raw);
}

#[test]
fn stores_mixed_argument_list_format() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Container,
        MacroArgumentKind::ArgumentList,
        MacroExecutionKind::Sync,
        "button-handler",
    );

    assert_eq!(definition.argument_kind, MacroArgumentKind::ArgumentList);
}

#[test]
fn parses_shared_interaction_target_syntax() {
    let target =
        parse_interaction_target("[[$LocationName|$Location]]").expect("交互目标参数应可解析");

    assert_eq!(target.label, "$LocationName");
    assert_eq!(target.target, "$Location");
}

#[test]
fn rejects_incomplete_interaction_target_syntax() {
    let missing_separator: InteractionTargetError =
        parse_interaction_target("[[Map]]").expect_err("交互目标必须包含分隔符");
    let empty_target: InteractionTargetError =
        parse_interaction_target("[[Map| ]]").expect_err("交互目标不能为空");

    assert_eq!(missing_separator, InteractionTargetError::MissingSeparator);
    assert_eq!(empty_target, InteractionTargetError::EmptyTarget);
    assert_eq!(
        empty_target.diagnostic().code,
        "macro.invalid_interaction_target"
    );
}

#[test]
fn parses_expressions_and_interaction_targets_in_source_order() {
    let arguments: Vec<MacroArgument<'_>> =
        parse_argument_list("\"Author\" [[确认前往|Map]] ($count + 1)")
            .expect("混合参数列表应可解析");

    assert_eq!(arguments.len(), 3);
    assert!(matches!(
        &arguments[0],
        MacroArgument::Expression { offset: 0, .. }
    ));
    assert_eq!(
        arguments[1],
        MacroArgument::InteractionTarget {
            target: crate::macro_runtime::InteractionTarget {
                label: "确认前往",
                target: "Map",
            },
            label_offset: 11,
            target_offset: 24,
        }
    );
    assert!(matches!(
        &arguments[2],
        MacroArgument::Expression { offset: 30, .. }
    ));
}

#[test]
fn keeps_spaces_inside_an_interaction_target() {
    let arguments: Vec<MacroArgument<'_>> =
        parse_argument_list("[[确认 前往|Map Room]]").expect("交互目标内部可以包含空格");

    assert_eq!(
        arguments,
        vec![MacroArgument::InteractionTarget {
            target: crate::macro_runtime::InteractionTarget {
                label: "确认 前往",
                target: "Map Room",
            },
            label_offset: 2,
            target_offset: 16,
        }]
    );
}

#[test]
fn reports_the_argument_offset_for_expression_errors() {
    let error: MacroArgumentListError =
        parse_argument_list("[[前往|Map]] ($count +)").expect_err("错误表达式应被拒绝");

    assert!(matches!(
        error,
        MacroArgumentListError::Expression { offset: 15, .. }
    ));
}

#[test]
fn maps_argument_parse_diagnostic_to_twee_source() {
    let error: MacroArgumentListError = parse_argument_list("1 $").expect_err("空变量名称应被拒绝");
    let issue: MacroArgumentIssue = error.issue(3);
    let locator: DiagnosticLocator<'_> = DiagnosticLocator::new("story/main.twee", "X\n<<x 1 $>>");
    let diagnostic: Diagnostic = issue
        .locate(&locator, 6)
        .expect("参数片段位置应可映射回 Twee");

    assert_eq!(diagnostic.code, "expression.invalid_variable");
    assert_eq!(diagnostic.location.expect("应包含位置").start, 8);
}

#[test]
fn keeps_nested_evaluation_span_in_argument_issue() {
    let error: MacroArgumentValueError<EvalError> =
        MacroArgumentValueError::InteractionEvaluation {
            error: EvalError::UnknownGlobal(Span { start: 1, end: 4 }),
            offset: 10,
        };
    let issue: MacroArgumentIssue = error.issue(20);

    assert_eq!(issue.diagnostic.code, "expression.unknown_global");
    assert_eq!(issue.span, Span { start: 11, end: 14 });
}

#[test]
fn prepares_mixed_arguments_as_ordered_runtime_values() {
    let arguments: Vec<MacroArgument<'_>> =
        parse_argument_list("\"Author\" [[确认前往|Map]] (1 + 1)").expect("参数列表应可解析");
    let base: EmptyEvaluationContext = EmptyEvaluationContext;

    let values: Vec<Value> =
        prepare_argument_values(&arguments, |expression| evaluate_with(expression, &base))
            .expect("参数应可准备为运行时值");

    assert_eq!(
        values,
        vec![
            Value::string("Author"),
            Value::object(vec![
                (String::from("label"), Value::string("确认前往")),
                (String::from("target"), Value::string("Map")),
            ]),
            Value::Number(2.0),
        ]
    );
}

#[test]
fn enters_a_call_with_prepared_args_for_handler_access() {
    let arguments: Vec<MacroArgument<'_>> =
        parse_argument_list("\"Author\" [[确认前往|Map]]").expect("参数列表应可解析");
    let base: EmptyEvaluationContext = EmptyEvaluationContext;
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();

    enter_argument_call(&mut locals, &arguments, |expression| {
        evaluate_with(expression, &base)
    })
    .expect("准备完成的参数应建立调用帧");

    let context: MacroEvaluationContext<'_> = MacroEvaluationContext::new(&base, &locals);
    let label = parse("@args[1].label").expect("Interaction Target 字段应可解析");
    assert_eq!(
        evaluate_with(&label, &context).expect("Handler 应可读取 Interaction Target"),
        Value::string("确认前往")
    );
}

#[test]
fn does_not_enter_a_partial_call_when_argument_evaluation_fails() {
    let arguments: Vec<MacroArgument<'_>> =
        parse_argument_list("(1 + 1)").expect("参数列表应可解析");
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(vec![Value::string("outer")]);

    let error: MacroArgumentValueError<&str> =
        enter_argument_call(&mut locals, &arguments, |_expression| Err("failed"))
            .expect_err("求值失败不得建立新调用帧");

    assert_eq!(
        error,
        MacroArgumentValueError::Expression {
            error: "failed",
            offset: 0,
        }
    );
    assert_eq!(locals.args(), Some(&[Value::string("outer")][..]));
}

#[test]
fn keeps_expression_offset_when_argument_evaluation_fails() {
    let arguments: Vec<MacroArgument<'_>> =
        parse_argument_list("[[前往|Map]] (1 + 1)").expect("参数列表应可解析");

    let error: MacroArgumentValueError<&str> =
        prepare_argument_values(&arguments, |_expression| Err("failed"))
            .expect_err("Expression 求值错误应被保留");

    assert_eq!(
        error,
        MacroArgumentValueError::Expression {
            error: "failed",
            offset: 15,
        }
    );
}

#[test]
fn evaluates_variable_and_interpolated_interaction_fields() {
    let arguments: Vec<MacroArgument<'_>> =
        parse_argument_list("[[$LocationName|区域 ${$Location}]]")
            .expect("动态 Interaction Target 应可解析");
    let base: InteractionEvaluationContext = InteractionEvaluationContext {
        location_name: Value::string("世界地图"),
        location: Value::string("Map"),
    };

    let values: Vec<Value> =
        prepare_argument_values(&arguments, |expression| evaluate_with(expression, &base))
            .expect("变量与插值应在调用前求值");

    assert_eq!(
        values,
        vec![Value::object(vec![
            (String::from("label"), Value::string("世界地图")),
            (String::from("target"), Value::string("区域 Map")),
        ])]
    );
}

#[test]
fn preserves_literal_spacing_around_interaction_interpolation() {
    let spaced: Vec<MacroArgument<'_>> =
        parse_argument_list("[[前往 ${$LocationName}|Map]]").expect("带空格目标应可解析");
    let compact: Vec<MacroArgument<'_>> =
        parse_argument_list("[[前往${$LocationName}|Map]]").expect("紧邻目标应可解析");
    let base: InteractionEvaluationContext = InteractionEvaluationContext {
        location_name: Value::string("世界地图"),
        location: Value::string("Map"),
    };

    let spaced_values: Vec<Value> =
        prepare_argument_values(&spaced, |expression| evaluate_with(expression, &base))
            .expect("带空格插值应可求值");
    let compact_values: Vec<Value> =
        prepare_argument_values(&compact, |expression| evaluate_with(expression, &base))
            .expect("紧邻插值应可求值");

    assert_eq!(
        spaced_values,
        vec![Value::object(vec![
            (String::from("label"), Value::string("前往 世界地图")),
            (String::from("target"), Value::string("Map")),
        ])]
    );
    assert_eq!(
        compact_values,
        vec![Value::object(vec![
            (String::from("label"), Value::string("前往世界地图")),
            (String::from("target"), Value::string("Map")),
        ])]
    );
}

#[test]
fn keeps_nested_braces_inside_interaction_interpolation() {
    let arguments: Vec<MacroArgument<'_>> = parse_argument_list(
        "[[前往 ${({ value: \"Map}\" }).value}|${({ target: $Location }).target}]]",
    )
    .expect("带嵌套花括号的 Interaction Target 应可解析");
    let base: InteractionEvaluationContext = InteractionEvaluationContext {
        location_name: Value::string("世界地图"),
        location: Value::string("Map"),
    };

    let values: Vec<Value> =
        prepare_argument_values(&arguments, |expression| evaluate_with(expression, &base))
            .expect("嵌套对象与字符串内花括号不应提前结束插值");

    assert_eq!(
        values,
        vec![Value::object(vec![
            (String::from("label"), Value::string("前往 Map}")),
            (String::from("target"), Value::string("Map")),
        ])]
    );
}

#[test]
fn rejects_unclosed_or_non_scalar_interaction_content() {
    let unclosed: Vec<MacroArgument<'_>> =
        parse_argument_list("[[前往 ${$Location|Map]]").expect("外层 Interaction Target 应可解析");
    let object: Vec<MacroArgument<'_>> =
        parse_argument_list("[[$LocationName|Map]]").expect("变量目标应可解析");
    let scalar_context: InteractionEvaluationContext = InteractionEvaluationContext {
        location_name: Value::string("Map"),
        location: Value::string("Map"),
    };
    let object_context: InteractionEvaluationContext = InteractionEvaluationContext {
        location_name: Value::object(Vec::new()),
        location: Value::string("Map"),
    };

    let unclosed_error: MacroArgumentValueError<EvalError> =
        prepare_argument_values(&unclosed, |expression| {
            evaluate_with(expression, &scalar_context)
        })
        .expect_err("未闭合插值应被拒绝");
    let object_error: MacroArgumentValueError<EvalError> =
        prepare_argument_values(&object, |expression| {
            evaluate_with(expression, &object_context)
        })
        .expect_err("集合不得隐式转换为 Interaction 文本");

    assert_eq!(
        unclosed_error,
        MacroArgumentValueError::UnclosedInteraction { offset: 9 }
    );
    assert_eq!(
        object_error,
        MacroArgumentValueError::InvalidInteractionText { offset: 2 }
    );
}

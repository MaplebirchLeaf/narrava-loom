//! HIR 到最小 MIR 顺序指令的测试。

use crate::{
    expression::parse,
    hir::{
        HirBodyKind, HirBodyNode, HirCapture, HirFor, HirForKind, HirForTarget, HirIf, HirIfBranch,
        HirMacro, HirMacroArguments, HirPassage, HirPrint, HirStory, HirSwitch, HirSwitchCase,
        HirWhile,
    },
    mir::{
        MirBody, MirCollectionIterationKind, MirExecutionPosition, MirInstruction,
        MirInstructionPointer, MirMacroBody, MirStory, lower_body,
    },
    runtime::{RuntimeExecutionIdentity, RuntimeExecutionLocation},
    source::Source,
    twee::{MacroSyntaxKind, Span},
};
use std::path::Path;

fn node(kind: HirBodyKind<'_>) -> HirBodyNode<'_> {
    HirBodyNode {
        kind,
        span: Span {
            start: 0,
            end: 1,
            line: 1,
            column: 1,
        },
    }
}

#[test]
fn lowers_linear_text_and_print_to_explicit_halt() {
    let body: Vec<HirBodyNode<'_>> = vec![
        node(HirBodyKind::Text("森林")),
        node(HirBodyKind::Print(HirPrint::Literal("入口"))),
    ];

    let mir: MirBody<'_, '_> = lower_body(&body).expect("顺序节点应进入 MIR");
    let instructions: &[MirInstruction<'_, '_>] = mir.instructions();

    assert!(matches!(
        instructions[0],
        MirInstruction::Text { text: "森林", .. }
    ));
    assert!(matches!(
        instructions[1],
        MirInstruction::PrintLiteral { text: "入口", .. }
    ));
    assert!(matches!(instructions[2], MirInstruction::Halt));
}

#[test]
fn mir_story_owns_the_i18n_catalog_from_the_same_hir() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let hir: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![
                node(HirBodyKind::Text("欢迎，")),
                node(HirBodyKind::Print(HirPrint::Expression(
                    parse("$name").expect("变量表达式应有效"),
                ))),
                node(HirBodyKind::Text("。")),
            ],
        }],
    };

    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("Story 应进入 MIR");

    assert_eq!(mir.i18n().messages().len(), 1);
    assert_eq!(mir.i18n().messages()[0].id().as_str(), "p5:Start:body.0");
    assert_eq!(mir.i18n().messages()[0].text(), "欢迎，{$name}。");
    let MirInstruction::Text {
        i18n: Some(identity),
        ..
    } = &mir
        .passage("Start")
        .expect("Start MIR 应存在")
        .instructions()[0]
    else {
        panic!("可翻译 Text 应携带 I18n 身份");
    };
    assert_eq!(identity.id().as_str(), "p5:Start:body.0");
    assert_eq!(identity.placeholder(), None);
    let MirInstruction::PrintExpression {
        i18n: Some(identity),
        ..
    } = &mir
        .passage("Start")
        .expect("Start MIR 应存在")
        .instructions()[1]
    else {
        panic!("可翻译 Print 应携带 I18n placeholder");
    };
    assert_eq!(identity.id().as_str(), "p5:Start:body.0");
    assert_eq!(identity.placeholder(), Some("$name"));
}

#[test]
fn dynamic_container_body_does_not_shift_following_i18n_identity() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let span = |start: usize, end: usize| Span {
        start,
        end,
        line: 1,
        column: start + 1,
    };
    let hir: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![
                HirBodyNode {
                    kind: HirBodyKind::Macro(HirMacro {
                        name: "slot",
                        arguments: HirMacroArguments::Raw("\"status\""),
                        syntax_kind: MacroSyntaxKind::Container,
                        body: vec![HirBodyNode {
                            kind: HirBodyKind::Text("槽内文字"),
                            span: span(10, 14),
                        }],
                    }),
                    span: span(0, 15),
                },
                HirBodyNode {
                    kind: HirBodyKind::Text("槽后正文"),
                    span: span(16, 20),
                },
            ],
        }],
    };

    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("Story 应进入 MIR");
    let MirInstruction::Text {
        i18n: Some(identity),
        ..
    } = &mir.passage("Start").unwrap().instructions()[1]
    else {
        panic!("slot 后正文应携带自己的 I18n 身份");
    };

    assert_eq!(identity.id().as_str(), "p5:Start:body.1");
}

#[test]
fn lowers_capture_names_onto_nested_dynamic_macros() {
    let body: Vec<HirBodyNode<'_>> = vec![node(HirBodyKind::Capture(HirCapture {
        locals: vec!["name", "index"],
        body: vec![node(HirBodyKind::Macro(HirMacro {
            name: "link",
            arguments: HirMacroArguments::Raw("[[前往|Map]]"),
            syntax_kind: crate::twee::MacroSyntaxKind::Container,
            body: Vec::new(),
        }))],
    }))];

    let lowered: MirBody<'_, '_> = lower_body(&body).expect("capture 应进入 MIR 元数据");

    assert!(matches!(
        &lowered.instructions()[0],
        MirInstruction::InvokeMacro { call, captures, .. }
            if call.name == "link" && captures == &["name", "index"]
    ));
    assert!(matches!(lowered.instructions()[1], MirInstruction::Halt));
}

#[test]
fn lowers_delayed_macro_body_as_an_independent_mir_unit() {
    let body: Vec<HirBodyNode<'_>> = vec![
        node(HirBodyKind::Macro(HirMacro {
            name: "wait",
            arguments: HirMacroArguments::None,
            syntax_kind: MacroSyntaxKind::Inline,
            body: Vec::new(),
        })),
        node(HirBodyKind::Set(Box::new(
            parse("$ready = true").expect("set 应可解析"),
        ))),
        node(HirBodyKind::Text("完成")),
    ];

    let lowered: MirMacroBody<'_, '_> =
        MirMacroBody::lower(&body).expect("延迟正文应复用 MIR lowering");

    assert!(matches!(
        lowered.instructions(),
        [
            MirInstruction::InvokeMacro { call, .. },
            MirInstruction::EvaluateDiscard(_),
            MirInstruction::Text { text: "完成", .. },
            MirInstruction::Halt,
        ] if call.name == "wait"
    ));
}

#[test]
fn rejects_nodes_without_a_defined_mir_lowering() {
    let body: Vec<HirBodyNode<'_>> = vec![node(HirBodyKind::Break)];

    let error = lower_body(&body).expect_err("未定义的控制流不能被静默忽略");

    assert_eq!(error.kind, "break");
    assert_eq!(error.span, body[0].span);
}

#[test]
fn instruction_pointer_never_advances_past_the_program() {
    let start: MirInstructionPointer = MirInstructionPointer::start();

    assert_eq!(start.index(), 0);
    assert_eq!(start.next(2).expect("应进入第二条指令").index(), 1);
    assert!(start.next(1).is_none());
}

#[test]
fn lowers_if_to_typed_conditional_and_end_jumps() {
    let condition = parse("$ready").expect("测试条件应有效");
    let body: Vec<HirBodyNode<'_>> = vec![node(HirBodyKind::If(HirIf {
        branches: vec![HirIfBranch {
            condition,
            body: vec![node(HirBodyKind::Text("准备完成"))],
        }],
        fallback: Some(vec![node(HirBodyKind::Text("尚未准备"))]),
    }))];

    let mir: MirBody<'_, '_> = lower_body(&body).expect("if 应降低为显式跳转");
    let instructions: &[MirInstruction<'_, '_>] = mir.instructions();

    let MirInstruction::JumpIfFalse { target, .. } = instructions[0] else {
        panic!("首条指令应检查条件");
    };
    let MirInstruction::Jump { target: end } = instructions[2] else {
        panic!("真分支后应跳过 fallback");
    };
    assert_eq!(target.index(), 3);
    assert_eq!(end.index(), 5);
    assert!(matches!(
        instructions[1],
        MirInstruction::Text {
            text: "准备完成",
            ..
        }
    ));
    assert!(matches!(
        instructions[3],
        MirInstruction::Text {
            text: "尚未准备",
            ..
        }
    ));
    assert!(
        matches!(instructions[4], MirInstruction::Jump { .. }),
        "fallback 后应有跳转到结构末尾的分隔"
    );
    assert!(matches!(instructions[5], MirInstruction::Halt));
}

#[test]
fn lowers_switch_with_one_evaluation_slot_and_strict_case_jumps() {
    let value = parse("$location").expect("switch 主值应有效");
    let candidate = parse("\"Forest\"").expect("case 值应有效");
    let body: Vec<HirBodyNode<'_>> = vec![node(HirBodyKind::Switch(Box::new(HirSwitch {
        value,
        cases: vec![HirSwitchCase {
            value: candidate,
            body: vec![node(HirBodyKind::Text("进入森林"))],
        }],
        default: Some(vec![node(HirBodyKind::Text("停留原地"))]),
    })))];

    let mir: MirBody<'_, '_> = lower_body(&body).expect("switch 应进入 MIR");
    let instructions: &[MirInstruction<'_, '_>] = mir.instructions();
    let MirInstruction::Evaluate { destination, .. } = instructions[0] else {
        panic!("switch 主值应先求值一次");
    };
    let MirInstruction::JumpIfNotStrictEqual {
        left, target: next, ..
    } = instructions[1]
    else {
        panic!("case 应使用严格不相等跳转");
    };
    let MirInstruction::Jump { target: end } = instructions[3] else {
        panic!("匹配 case 后应越过 default");
    };

    assert_eq!(mir.value_slot_count(), 1);
    assert_eq!(destination, left);
    assert_eq!(destination.index(), 0);
    assert_eq!(next.index(), 4);
    assert_eq!(end.index(), 6);
    assert!(matches!(
        instructions[2],
        MirInstruction::Text {
            text: "进入森林",
            ..
        }
    ));
    assert!(matches!(
        instructions[4],
        MirInstruction::Text {
            text: "停留原地",
            ..
        }
    ));
    assert!(
        matches!(instructions[5], MirInstruction::Jump { .. }),
        "default 后应有跳转到结构末尾的分隔"
    );
    assert!(matches!(instructions[6], MirInstruction::Halt));
}

#[test]
fn lowers_while_break_and_continue_to_current_loop_targets() {
    let condition = parse("$active").expect("while 条件应有效");
    let body: Vec<HirBodyNode<'_>> = vec![node(HirBodyKind::While(Box::new(HirWhile {
        condition,
        body: vec![
            node(HirBodyKind::Text("循环")),
            node(HirBodyKind::Continue),
            node(HirBodyKind::Break),
        ],
    })))];

    let mir: MirBody<'_, '_> = lower_body(&body).expect("while 应进入 MIR");
    let instructions: &[MirInstruction<'_, '_>] = mir.instructions();
    let MirInstruction::JumpIfFalse { target: end, .. } = instructions[0] else {
        panic!("while 应先检查条件");
    };
    let MirInstruction::Jump {
        target: continue_target,
    } = instructions[2]
    else {
        panic!("continue 应降低为跳转");
    };
    let MirInstruction::Jump {
        target: break_target,
    } = instructions[3]
    else {
        panic!("break 应降低为跳转");
    };
    let MirInstruction::Jump {
        target: loop_target,
    } = instructions[4]
    else {
        panic!("循环正文结束后应重新检查条件");
    };

    assert_eq!(continue_target.index(), 0);
    assert_eq!(loop_target.index(), 0);
    assert_eq!(break_target, end);
    assert_eq!(end.index(), 5);
    assert!(matches!(instructions[5], MirInstruction::Halt));
}

#[test]
fn nested_loop_break_does_not_escape_the_outer_loop() {
    let outer_condition = parse("$outer").expect("外层条件应有效");
    let inner_condition = parse("$inner").expect("内层条件应有效");
    let body: Vec<HirBodyNode<'_>> = vec![node(HirBodyKind::While(Box::new(HirWhile {
        condition: outer_condition,
        body: vec![
            node(HirBodyKind::While(Box::new(HirWhile {
                condition: inner_condition,
                body: vec![node(HirBodyKind::Break)],
            }))),
            node(HirBodyKind::Continue),
        ],
    })))];

    let mir: MirBody<'_, '_> = lower_body(&body).expect("嵌套 while 应进入 MIR");
    let instructions: &[MirInstruction<'_, '_>] = mir.instructions();
    let MirInstruction::Jump {
        target: inner_break,
    } = instructions[2]
    else {
        panic!("内层 break 应降低为跳转");
    };
    let MirInstruction::Jump {
        target: outer_continue,
    } = instructions[4]
    else {
        panic!("外层 continue 应降低为跳转");
    };

    assert_eq!(inner_break.index(), 4);
    assert_eq!(outer_continue.index(), 0);
    assert!(matches!(instructions[6], MirInstruction::Halt));
}

#[test]
fn lowers_for_in_and_of_to_distinct_collection_iteration_kinds() {
    let key_target = parse("@key").expect("in 目标应有效");
    let value_target = parse("@value").expect("of 目标应有效");
    let object = parse("$object").expect("in 集合应有效");
    let array = parse("$array").expect("of 集合应有效");
    let span: Span = node(HirBodyKind::Text("")).span;
    let body: Vec<HirBodyNode<'_>> = vec![
        node(HirBodyKind::For(Box::new(HirFor {
            target: HirForTarget {
                value: key_target,
                span,
            },
            kind: HirForKind::In {
                collection: object,
                span,
            },
            body: Vec::new(),
        }))),
        node(HirBodyKind::For(Box::new(HirFor {
            target: HirForTarget {
                value: value_target,
                span,
            },
            kind: HirForKind::Of {
                collection: array,
                span,
            },
            body: Vec::new(),
        }))),
    ];

    let mir: MirBody<'_, '_> = lower_body(&body).expect("集合 for 应进入 MIR");
    let instructions: &[MirInstruction<'_, '_>] = mir.instructions();
    let MirInstruction::PrepareCollectionIteration {
        kind: first_kind,
        destination: first_slot,
        ..
    } = instructions[0]
    else {
        panic!("for in 应准备集合迭代");
    };
    let MirInstruction::PrepareCollectionIteration {
        kind: second_kind,
        destination: second_slot,
        ..
    } = instructions[3]
    else {
        panic!("for of 应准备集合迭代");
    };

    assert_eq!(first_kind, MirCollectionIterationKind::Keys);
    assert_eq!(second_kind, MirCollectionIterationKind::Values);
    assert_ne!(first_slot, second_slot);
    assert_eq!(mir.iterator_slot_count(), 2);
}

#[test]
fn lowers_for_range_with_stable_next_break_and_continue_targets() {
    let target = parse("@index").expect("range 目标应有效");
    let start = parse("1").expect("range 起点应有效");
    let end = parse("5").expect("range 终点应有效");
    let step = parse("2").expect("range 步长应有效");
    let span: Span = node(HirBodyKind::Text("")).span;
    let body: Vec<HirBodyNode<'_>> = vec![node(HirBodyKind::For(Box::new(HirFor {
        target: HirForTarget {
            value: target,
            span,
        },
        kind: HirForKind::Range {
            start,
            start_span: span,
            end,
            end_span: span,
            step: Some(step),
            step_span: Some(span),
        },
        body: vec![node(HirBodyKind::Continue), node(HirBodyKind::Break)],
    })))];

    let mir: MirBody<'_, '_> = lower_body(&body).expect("range for 应进入 MIR");
    let instructions: &[MirInstruction<'_, '_>] = mir.instructions();
    let MirInstruction::PrepareRangeIteration { destination, .. } = instructions[0] else {
        panic!("range 应只准备一次边界");
    };
    let MirInstruction::NextIteration {
        iterator,
        exhausted,
        ..
    } = instructions[1]
    else {
        panic!("range 应从迭代槽读取下一值");
    };
    let MirInstruction::Jump {
        target: continue_target,
    } = instructions[2]
    else {
        panic!("continue 应返回 NextIteration");
    };
    let MirInstruction::Jump {
        target: break_target,
    } = instructions[3]
    else {
        panic!("break 应跳到 for 结束");
    };

    assert_eq!(destination, iterator);
    assert_eq!(continue_target.index(), 1);
    assert_eq!(break_target, exhausted);
    assert_eq!(exhausted.index(), 5);
    assert_eq!(mir.iterator_slot_count(), 1);
}

#[test]
fn lowers_set_run_and_unset_without_duplicate_expression_instructions() {
    let assignment = parse("$count = 1").expect("set 赋值应有效");
    let call = parse("Math.abs(-1)").expect("run 表达式应有效");
    let target = parse("$count").expect("unset 目标应有效");
    let body: Vec<HirBodyNode<'_>> = vec![
        node(HirBodyKind::Set(Box::new(assignment))),
        node(HirBodyKind::Run(Box::new(call))),
        node(HirBodyKind::Unset(Box::new(target))),
    ];

    let mir: MirBody<'_, '_> = lower_body(&body).expect("State 动作应进入 MIR");
    let instructions: &[MirInstruction<'_, '_>] = mir.instructions();

    assert!(matches!(
        instructions[0],
        MirInstruction::EvaluateDiscard(_)
    ));
    assert!(matches!(
        instructions[1],
        MirInstruction::EvaluateDiscard(_)
    ));
    assert!(matches!(instructions[2], MirInstruction::Unset(_)));
    assert!(matches!(instructions[3], MirInstruction::Halt));
}

#[test]
fn keeps_include_and_goto_as_distinct_story_requests() {
    let included = parse("\"Details\"").expect("include 目标应有效");
    let destination = parse("$next").expect("goto 目标应有效");
    let body: Vec<HirBodyNode<'_>> = vec![
        node(HirBodyKind::Include(Box::new(included))),
        node(HirBodyKind::Goto(Box::new(destination))),
    ];

    let mir: MirBody<'_, '_> = lower_body(&body).expect("Story 动作应进入 MIR");
    let instructions: &[MirInstruction<'_, '_>] = mir.instructions();

    assert!(matches!(
        instructions[0],
        MirInstruction::RequestInclude { .. }
    ));
    assert!(matches!(instructions[1], MirInstruction::RequestGoto(_)));
    assert!(matches!(instructions[2], MirInstruction::Halt));
}

#[test]
fn lowers_dynamic_macro_without_binding_its_runtime_definition() {
    let body: Vec<HirBodyNode<'_>> = vec![node(HirBodyKind::Macro(HirMacro {
        name: "notice",
        arguments: HirMacroArguments::Raw("\"hello\""),
        syntax_kind: crate::twee::MacroSyntaxKind::Inline,
        body: Vec::new(),
    }))];

    let mir: MirBody<'_, '_> = lower_body(&body).expect("动态 Macro 应进入 MIR");
    let instructions: &[MirInstruction<'_, '_>] = mir.instructions();
    let MirInstruction::InvokeMacro { call, .. } = instructions[0] else {
        panic!("动态 Macro 应保留为运行时调用");
    };

    assert_eq!(call.name, "notice");
    assert_eq!(call.arguments, HirMacroArguments::Raw("\"hello\""));
}

#[test]
fn story_positions_combine_case_sensitive_passage_and_instruction_identity() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let hir: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![node(HirBodyKind::Text("大写"))],
            },
            HirPassage {
                source: &source.path,
                name: "start",
                tags: Vec::new(),
                body: vec![node(HirBodyKind::Text("小写"))],
            },
        ],
    };

    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("线性 Story 应进入 MIR");
    let upper = mir.passage("Start").expect("应保留大写 PassageName");
    let lower = mir.passage("start").expect("应保留小写 PassageName");
    let position: MirExecutionPosition = MirExecutionPosition::start(upper.id());
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(3, 5);
    let location: RuntimeExecutionLocation = RuntimeExecutionLocation::new(identity, position);

    assert_ne!(upper.id(), lower.id());
    assert_eq!(position.passage(), upper.id());
    assert_eq!(position.instruction().index(), 0);
    assert_eq!(location.identity(), identity);
    assert_eq!(location.position(), position);
    assert_eq!(
        position
            .next(upper.instructions().len())
            .expect("Text 后应进入 Halt")
            .instruction()
            .index(),
        1
    );
}

//! MIR 单步执行帧测试。

use std::{collections::BTreeMap, path::Path};

use crate::{
    GameIdentity,
    expression::{
        parse,
        value::{TextValue, Value},
    },
    hir::{
        HirBodyKind, HirBodyNode, HirCapture, HirFor, HirForKind, HirForTarget, HirMacro,
        HirMacroArguments, HirPassage, HirPrint, HirStory, HirSwitch, HirSwitchCase,
    },
    i18n::{
        I18nLanguageChain, I18nRuntimeLanguage, I18nTemplate, I18nTemplateMessage,
        I18nValidatedTemplate, NlangPackageEntry, NlangPackageInput, NlangValidatedPackage,
    },
    lir::LirProgram,
    mir::{MirMacroBody, MirStory},
    semantic::{SemanticNode, SemanticOutput},
    source::Source,
    state::State,
    twee::{MacroSyntaxKind, Span},
    vm::{MirExecutionError, MirExecutionFrame, MirStep},
};

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
fn macro_body_frame_pauses_at_macro_and_continues_from_the_next_instruction() {
    let body: Vec<HirBodyNode<'_>> = vec![
        node(HirBodyKind::Macro(HirMacro {
            name: "wait",
            arguments: HirMacroArguments::None,
            syntax_kind: MacroSyntaxKind::Inline,
            body: Vec::new(),
        })),
        node(HirBodyKind::Set(Box::new(
            parse("$ready = true").expect("set 应有效"),
        ))),
        node(HirBodyKind::Text("完成")),
    ];
    let mir: MirMacroBody<'_, '_> = MirMacroBody::lower(&body).expect("正文应进入 MIR");
    let macro_bytecode: crate::bytecode::BytecodeMacroBody =
        crate::bytecode::BytecodeMacroBody::compile(&mir);
    let mut frame: MirExecutionFrame = MirExecutionFrame::new_macro(&macro_bytecode);
    let mut state: State = State::new();

    assert_eq!(
        frame.step_macro(&macro_bytecode, &mut state),
        Ok(MirStep::MacroPending)
    );
    assert_eq!(
        frame
            .pending_macro_body(&macro_bytecode)
            .map(|call| call.name.as_str()),
        Some("wait")
    );
    frame
        .complete_macro_body(&macro_bytecode, SemanticOutput::default())
        .expect("完成异步 Macro 后应推进位置");
    assert_eq!(
        frame.step_macro(&macro_bytecode, &mut state),
        Ok(MirStep::Running)
    );
    assert_eq!(state.variables_get("ready"), Some(&Value::Boolean(true)));
    assert_eq!(
        frame.step_macro(&macro_bytecode, &mut state),
        Ok(MirStep::Running)
    );
    assert_eq!(
        frame.step_macro(&macro_bytecode, &mut state),
        Ok(MirStep::Halted)
    );
    assert!(matches!(
        frame.output().nodes(),
        [SemanticNode::Text(text)]
            if text.to_unicode_string().as_deref() == Some("完成")
    ));
}

#[test]
fn macro_body_frame_rejects_include_without_impersonating_a_story_passage() {
    let body: Vec<HirBodyNode<'_>> = vec![node(HirBodyKind::Include(Box::new(
        parse("\"Other\"").expect("include 目标应有效"),
    )))];
    let mir: MirMacroBody<'_, '_> = MirMacroBody::lower(&body).expect("正文应进入 MIR");
    let macro_bytecode: crate::bytecode::BytecodeMacroBody =
        crate::bytecode::BytecodeMacroBody::compile(&mir);
    let mut frame: MirExecutionFrame = MirExecutionFrame::new_macro(&macro_bytecode);
    let mut state: State = State::new();

    assert_eq!(
        frame.step_macro(&macro_bytecode, &mut state),
        Err(MirExecutionError::MacroBodyIncludeUnsupported)
    );
}

#[test]
fn frame_steps_text_and_print_until_halt_without_losing_position() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let expression = parse("$count").expect("print Expression 应有效");
    let hir: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![
                node(HirBodyKind::Text("数量：")),
                node(HirBodyKind::Print(HirPrint::Expression(expression))),
            ],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("测试 Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let encoded = crate::bytecode::BytecodeProgram::compile(&lir)
        .to_json()
        .expect("Bytecode 应可序列化");
    let bytecode =
        crate::bytecode::BytecodeProgram::from_json(&encoded).expect("VM 应接受反序列化 Bytecode");
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let _previous: Option<Value> = state.variables_set("count", Value::Number(2.0));
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::Running));
    assert_eq!(frame.location().instruction().index(), 2);
    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::Halted));

    assert_eq!(frame.output().len(), 1);
    assert!(matches!(
        &frame.output().nodes()[0],
        SemanticNode::Text(text) if text.to_unicode_string().as_deref() == Some("数量：2")
    ));
}

#[test]
fn frame_rejects_an_execution_chain_that_exhausts_its_instruction_budget() {
    let body: Vec<HirBodyNode<'_>> = vec![
        node(HirBodyKind::Set(Box::new(
            parse("$first = true").expect("首条 set 应有效"),
        ))),
        node(HirBodyKind::Set(Box::new(
            parse("$second = true").expect("第二条 set 应有效"),
        ))),
        node(HirBodyKind::Set(Box::new(
            parse("$third = true").expect("第三条 set 应有效"),
        ))),
    ];
    let mir: MirMacroBody<'_, '_> = MirMacroBody::lower(&body).expect("正文应进入 MIR");
    let bytecode = crate::bytecode::BytecodeMacroBody::compile(&mir);
    let mut frame: MirExecutionFrame =
        MirExecutionFrame::new_macro(&bytecode).with_instruction_limit(2);
    let mut state: State = State::new();

    assert_eq!(
        frame.step_macro(&bytecode, &mut state),
        Ok(MirStep::Running)
    );
    assert_eq!(
        frame.step_macro(&bytecode, &mut state),
        Ok(MirStep::Running)
    );
    assert_eq!(
        frame.step_macro(&bytecode, &mut state),
        Err(MirExecutionError::InstructionLimitExceeded { limit: 2 })
    );
    assert_eq!(state.variables_get("third"), None);
}

#[test]
fn frame_uses_validated_translation_to_reorder_and_translate_dynamic_values() {
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
                node(HirBodyKind::Text("Found ")),
                node(HirBodyKind::Print(HirPrint::Expression(
                    parse("$item").expect("物品表达式应有效"),
                ))),
                node(HirBodyKind::Text(" × ")),
                node(HirBodyKind::Print(HirPrint::Expression(
                    parse("$count").expect("数量表达式应有效"),
                ))),
            ],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("测试 Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let template: I18nTemplate = I18nTemplate::new(
        "zh-CN",
        BTreeMap::from([
            (
                String::from("items"),
                BTreeMap::from([(String::from("Iron Sword"), String::from("铁剑"))]),
            ),
            (
                String::from("counts"),
                BTreeMap::from([(String::from("2"), String::from("二"))]),
            ),
        ]),
        BTreeMap::from([(
            String::from("p5:Start:body.0"),
            I18nTemplateMessage::new(
                "Found {$item} × {$count}",
                "{$count} 个{$item}",
                BTreeMap::from([
                    (String::from("$count"), String::from("counts")),
                    (String::from("$item"), String::from("items")),
                ]),
            ),
        )]),
    );
    let translation: I18nValidatedTemplate = mir
        .i18n()
        .validate(template)
        .expect("目标语言模板应通过目录校验");
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let _previous: Option<Value> =
        state.variables_set("item", Value::String(TextValue::from("Iron Sword")));
    let _previous: Option<Value> = state.variables_set("count", Value::Number(2.0));
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    let language: I18nRuntimeLanguage = I18nRuntimeLanguage::Translation(translation);
    assert_eq!(
        frame.step_with_runtime_language(&bytecode, &language, &mut state),
        Ok(MirStep::Running)
    );
    assert_eq!(frame.location().instruction().index(), 4);
    assert!(matches!(
        frame.output().nodes(),
        [SemanticNode::Text(text)]
            if text.to_unicode_string().as_deref() == Some("2 个铁剑")
    ));
}

#[test]
fn translated_text_keeps_hard_break_as_protocol_structure() {
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
                node(HirBodyKind::Text("a")),
                node(HirBodyKind::HardBreak),
                node(HirBodyKind::Text("b")),
            ],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("HardBreak 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let template: I18nTemplate = I18nTemplate::new(
        "zh-CN",
        BTreeMap::new(),
        BTreeMap::from([
            (
                String::from("p5:Start:body.0"),
                I18nTemplateMessage::new("a", "甲", BTreeMap::new()),
            ),
            (
                String::from("p5:Start:body.2"),
                I18nTemplateMessage::new("b", "乙", BTreeMap::new()),
            ),
        ]),
    );
    let language = I18nRuntimeLanguage::Translation(mir.i18n().validate(template).unwrap());
    let passage = bytecode.passage("Start").unwrap();
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);
    let mut state: State = State::new();

    for _ in 0..3 {
        let _step = frame
            .step_with_runtime_language(&bytecode, &language, &mut state)
            .unwrap();
    }

    assert!(matches!(
        frame.output().nodes(),
        [SemanticNode::Text(first), SemanticNode::HardBreak, SemanticNode::Text(second)]
            if first.to_unicode_string().as_deref() == Some("甲")
                && second.to_unicode_string().as_deref() == Some("乙")
    ));
}

#[test]
fn frame_uses_the_language_chain_before_falling_back_to_default_text() {
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
            body: vec![node(HirBodyKind::Text("Default text"))],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("测试 Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let chain: I18nLanguageChain = I18nLanguageChain::validate(
        mir.i18n(),
        "en",
        vec![
            vm_language_package("zh-Hant", Some("zh"), "Default text", ""),
            vm_language_package("zh", None, "Default text", "后备文本"),
        ],
    )
    .expect("语言回退链应有效");
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    let language: I18nRuntimeLanguage = I18nRuntimeLanguage::Chain(chain);
    assert_eq!(
        frame.step_with_runtime_language(&bytecode, &language, &mut state),
        Ok(MirStep::Running)
    );
    assert!(matches!(
        frame.output().nodes(),
        [SemanticNode::Text(text)]
            if text.to_unicode_string().as_deref() == Some("后备文本")
    ));
}

#[test]
fn frame_rejects_a_language_chain_validated_by_another_mir_story() {
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
            body: vec![node(HirBodyKind::Text("Default text"))],
        }],
    };
    let first: MirStory<'_, '_> = MirStory::lower(&hir).expect("首个 Story 应进入 MIR");
    let rebuilt: MirStory<'_, '_> = MirStory::lower(&hir).expect("重建 Story 应进入 MIR");
    let rebuilt_lir: LirProgram<'_, '_, '_> =
        LirProgram::lower(&rebuilt).expect("重建 MIR 应进入 LIR");
    let rebuilt_bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&rebuilt_lir);
    let chain: I18nLanguageChain = I18nLanguageChain::validate(
        first.i18n(),
        "en",
        vec![vm_language_package(
            "zh-CN",
            None,
            "Default text",
            "目标文本",
        )],
    )
    .expect("语言链应绑定首个目录");
    let passage = rebuilt_bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    let language: I18nRuntimeLanguage = I18nRuntimeLanguage::Chain(chain);
    assert_eq!(
        frame.step_with_runtime_language(&rebuilt_bytecode, &language, &mut state),
        Err(MirExecutionError::DifferentI18nCatalog)
    );
    assert_eq!(frame.location().instruction().index(), 0);
    assert!(frame.output().is_empty());
}

#[test]
fn frame_rejects_translation_validated_by_another_mir_story() {
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
            body: vec![node(HirBodyKind::Text("Same text"))],
        }],
    };
    let first: MirStory<'_, '_> = MirStory::lower(&hir).expect("首个 Story 应进入 MIR");
    let rebuilt: MirStory<'_, '_> = MirStory::lower(&hir).expect("重建 Story 应进入 MIR");
    let rebuilt_lir: LirProgram<'_, '_, '_> =
        LirProgram::lower(&rebuilt).expect("重建 MIR 应进入 LIR");
    let rebuilt_bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&rebuilt_lir);
    let translation: I18nValidatedTemplate = first
        .i18n()
        .validate(I18nTemplate::new("en", BTreeMap::new(), BTreeMap::new()))
        .expect("译文应通过首个 MIR 目录校验");
    let passage = rebuilt_bytecode.passage("Start").expect("Start 应存在");
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);
    let mut state: State = State::new();

    let language: I18nRuntimeLanguage = I18nRuntimeLanguage::Translation(translation);
    assert_eq!(
        frame.step_with_runtime_language(&rebuilt_bytecode, &language, &mut state),
        Err(MirExecutionError::DifferentI18nCatalog)
    );
    assert_eq!(frame.location().instruction().index(), 0);
    assert!(frame.output().is_empty());
}

#[test]
fn frame_executes_state_changes_and_strict_switch_control_flow() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let assignment = parse("$place = \"Forest\"").expect("set 应有效");
    let selected = parse("$place").expect("switch 主值应有效");
    let candidate = parse("\"Forest\"").expect("case 应有效");
    let target = parse("$place").expect("unset 应有效");
    let hir: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![
                node(HirBodyKind::Set(Box::new(assignment))),
                node(HirBodyKind::Switch(Box::new(HirSwitch {
                    value: selected,
                    cases: vec![HirSwitchCase {
                        value: candidate,
                        body: vec![node(HirBodyKind::Text("匹配"))],
                    }],
                    default: Some(vec![node(HirBodyKind::Text("未匹配"))]),
                }))),
                node(HirBodyKind::Unset(Box::new(target))),
            ],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("测试 Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    for _step in 0..16 {
        if frame.step(&bytecode, &mut state).expect("MIR 单步应成功") == MirStep::Halted {
            break;
        }
    }

    assert!(state.variables_get("place").is_none());
    assert_eq!(frame.output().len(), 1);
    assert!(matches!(
        &frame.output().nodes()[0],
        SemanticNode::Text(text) if text.to_unicode_string().as_deref() == Some("匹配")
    ));
    assert_eq!(
        frame.step(&bytecode, &mut state),
        Ok(MirStep::Halted),
        "Halt 必须保持稳定"
    );
}

#[test]
fn frame_preserves_range_iterator_state_between_single_steps() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let span: Span = node(HirBodyKind::Text("")).span;
    let initialize = parse("$sum = 0").expect("初始化应有效");
    let target = parse("$index").expect("for 目标应有效");
    let start = parse("1").expect("range 起点应有效");
    let end = parse("3").expect("range 终点应有效");
    let accumulate = parse("$sum += $index").expect("累加应有效");
    let hir: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![
                node(HirBodyKind::Set(Box::new(initialize))),
                node(HirBodyKind::For(Box::new(HirFor {
                    target: HirForTarget {
                        value: target,
                        span,
                    },
                    kind: HirForKind::Range {
                        start,
                        start_span: span,
                        end,
                        end_span: span,
                        step: None,
                        step_span: None,
                    },
                    body: vec![node(HirBodyKind::Set(Box::new(accumulate)))],
                }))),
            ],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("range Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    for _step in 0..32 {
        if frame.step(&bytecode, &mut state).expect("range 单步应成功") == MirStep::Halted {
            break;
        }
    }

    assert_eq!(state.variables_get("sum"), Some(&Value::Number(6.0)));
    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::Halted));
}

#[test]
fn collection_iterator_uses_the_snapshot_created_by_prepare() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let span: Span = node(HirBodyKind::Text("")).span;
    let target = parse("$item").expect("for 目标应有效");
    let collection = parse("$items").expect("集合应有效");
    let remember = parse("$last = $item").expect("循环赋值应有效");
    let hir: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![node(HirBodyKind::For(Box::new(HirFor {
                target: HirForTarget {
                    value: target,
                    span,
                },
                kind: HirForKind::Of { collection, span },
                body: vec![node(HirBodyKind::Set(Box::new(remember)))],
            })))],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("集合 Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let _previous: Option<Value> = state.variables_set(
        "items",
        Value::array(vec![Value::Number(1.0), Value::Number(2.0)]),
    );
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::Running));
    let _replaced: Option<Value> =
        state.variables_set("items", Value::array(vec![Value::Number(9.0)]));
    for _step in 0..24 {
        if frame.step(&bytecode, &mut state).expect("集合单步应成功") == MirStep::Halted {
            break;
        }
    }

    assert_eq!(state.variables_get("last"), Some(&Value::Number(2.0)));
}

#[test]
fn include_pushes_a_passage_frame_and_returns_to_the_caller() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let included = parse("\"Details\"").expect("include 目标应有效");
    let hir: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![
                    node(HirBodyKind::Text("A")),
                    node(HirBodyKind::Include(Box::new(included))),
                    node(HirBodyKind::Text("C")),
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Details",
                tags: Vec::new(),
                body: vec![node(HirBodyKind::Text("B"))],
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("include Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    for _step in 0..16 {
        if frame
            .step(&bytecode, &mut state)
            .expect("include 单步应成功")
            == MirStep::Halted
        {
            break;
        }
    }

    let output: String = frame
        .output()
        .nodes()
        .iter()
        .filter_map(|node| match node {
            SemanticNode::Text(text) => text.to_unicode_string(),
            _ => None,
        })
        .collect();
    assert_eq!(output, "ABC");
}

#[test]
fn goto_stops_the_chain_with_a_stable_navigation_request() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let destination = parse("\"Next\"").expect("goto 目标应有效");
    let hir: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![
                    node(HirBodyKind::Text("before")),
                    node(HirBodyKind::Goto(Box::new(destination))),
                    node(HirBodyKind::Text("after")),
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Next",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("goto Story 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::Running));
    assert_eq!(
        frame.step(&bytecode, &mut state),
        Ok(MirStep::NavigationPending)
    );
    assert_eq!(frame.navigation(), Some("Next"));
    assert_eq!(
        frame.step(&bytecode, &mut state),
        Ok(MirStep::NavigationPending)
    );
    assert_eq!(frame.output().len(), 1);
}

#[test]
fn silently_discards_direct_and_included_text_but_keeps_state_changes() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let included = parse("\"Details\"").expect("include 目标应有效");
    let assignment = parse("$changed = true").expect("set 应有效");
    let hir: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![
                    node(HirBodyKind::Text("A")),
                    node(HirBodyKind::Silently(vec![
                        node(HirBodyKind::Text("隐藏")),
                        node(HirBodyKind::Set(Box::new(assignment))),
                        node(HirBodyKind::Include(Box::new(included))),
                    ])),
                    node(HirBodyKind::Text("B")),
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Details",
                tags: Vec::new(),
                body: vec![node(HirBodyKind::Text("同样隐藏"))],
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("silently 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    for _step in 0..24 {
        if frame
            .step(&bytecode, &mut state)
            .expect("silently 单步应成功")
            == MirStep::Halted
        {
            break;
        }
    }

    let output: String = frame
        .output()
        .nodes()
        .iter()
        .filter_map(|node| match node {
            SemanticNode::Text(text) => text.to_unicode_string(),
            _ => None,
        })
        .collect();
    assert_eq!(output, "AB");
    assert_eq!(state.variables_get("changed"), Some(&Value::Boolean(true)));
}

#[test]
fn exit_stops_only_the_current_included_passage() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let included = parse("\"Details\"").expect("include 目标应有效");
    let hir: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![
                    node(HirBodyKind::Text("A")),
                    node(HirBodyKind::Include(Box::new(included))),
                    node(HirBodyKind::Text("D")),
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Details",
                tags: Vec::new(),
                body: vec![
                    node(HirBodyKind::Text("B")),
                    node(HirBodyKind::Exit),
                    node(HirBodyKind::Text("C")),
                ],
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("exit 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    for _step in 0..20 {
        if frame.step(&bytecode, &mut state).expect("exit 单步应成功") == MirStep::Halted {
            break;
        }
    }

    let output: String = frame
        .output()
        .nodes()
        .iter()
        .filter_map(|node| match node {
            SemanticNode::Text(text) => text.to_unicode_string(),
            _ => None,
        })
        .collect();
    assert_eq!(output, "ABD");
}

#[test]
fn dynamic_macro_stays_pending_at_the_same_vm_location() {
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
            body: vec![node(HirBodyKind::Capture(HirCapture {
                locals: vec!["name"],
                body: vec![node(HirBodyKind::Macro(HirMacro {
                    name: "notice",
                    arguments: HirMacroArguments::Raw("\"hello\""),
                    syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                    body: Vec::new(),
                }))],
            }))],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("动态 Macro 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);
    let before = frame.location();

    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::MacroPending));
    assert_eq!(frame.location(), before);
    assert_eq!(
        frame
            .pending_macro(&bytecode)
            .expect("应能读取待处理 Macro")
            .name,
        "notice"
    );
    assert_eq!(frame.pending_macro_captures(&bytecode), Some(vec!["name"]));
    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::MacroPending));
    assert_eq!(frame.location(), before);

    frame
        .complete_macro(
            &bytecode,
            SemanticOutput::from_nodes(vec![SemanticNode::Text(TextValue::from("完成"))]),
        )
        .expect("Macro 完成输出应回到 VM");
    assert_eq!(frame.location().instruction().index(), 1);
    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::Halted));
    assert_eq!(frame.output().len(), 1);
}

#[test]
fn silently_suppresses_completed_macro_output() {
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
            body: vec![node(HirBodyKind::Silently(vec![node(HirBodyKind::Macro(
                HirMacro {
                    name: "notice",
                    arguments: HirMacroArguments::None,
                    syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                    body: Vec::new(),
                },
            ))]))],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("静默 Macro 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut state: State = State::new();
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);

    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::MacroPending));
    frame
        .complete_macro(
            &bytecode,
            SemanticOutput::from_nodes(vec![SemanticNode::Text(TextValue::from("隐藏"))]),
        )
        .expect("静默 Macro 仍应正常完成");

    assert!(frame.output().is_empty());
    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::Halted));
}

fn vm_language_package(
    locale: &str,
    fallback: Option<&str>,
    source: &str,
    text: &str,
) -> NlangValidatedPackage {
    let fallback_field: String = fallback
        .map(|value: &str| format!(", \"fallback\": {value:?}"))
        .unwrap_or_default();
    let manifest: String = format!(
        r#"{{
            "locale": {locale:?}{fallback_field},
            "version": "1.0.0",
            "game": {{ "id": "example.forest", "versions": "*" }}
        }}"#
    );
    let translation: I18nTemplate = I18nTemplate::new(
        locale,
        BTreeMap::new(),
        BTreeMap::from([(
            String::from("p5:Start:body.0"),
            I18nTemplateMessage::new(source, text, BTreeMap::new()),
        )]),
    );
    let translations: String = translation.to_nmsg();
    let game: GameIdentity = GameIdentity::new("example.forest", "0.1.0").expect("游戏身份应有效");
    NlangPackageInput::new(vec![
        NlangPackageEntry::new("manifest.json", manifest.into_bytes()),
        NlangPackageEntry::new("translations.nmsg", translations.into_bytes()),
        NlangPackageEntry::new("dictionary.json", br#"{}"#.to_vec()),
    ])
    .validate(locale, &game)
    .expect("VM 测试语言包应有效")
}

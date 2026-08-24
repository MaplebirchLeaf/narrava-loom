use std::path::Path;

use crate::{
    bytecode::{BYTECODE_MAGIC, BYTECODE_VERSION, BytecodeProgram, Opcode},
    hir::HirStory,
    lir::LirProgram,
    mir::MirStory,
    source::{Source, SourceList},
    twee,
};

#[test]
fn bytecode_encodes_the_complete_story_with_header_entries_and_constants() {
    let sources: SourceList =
        SourceList::discover("src/tests/fixtures/game").expect("测试 Source 应可读取");
    let ast: twee::Story<'_> = twee::Story::build(&sources.items).expect("测试 Twee 应可编译");
    let hir: HirStory<'_> = HirStory::lower(&ast).expect("AST 应进入 HIR");
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("HIR 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");

    let bytecode: BytecodeProgram = BytecodeProgram::compile(&lir);

    assert_eq!(bytecode.header().magic, BYTECODE_MAGIC);
    assert_eq!(bytecode.header().version, BYTECODE_VERSION);
    let start = bytecode.passage("Start").expect("Start 入口应存在");
    assert_eq!(start.name(), "Start");
    assert_eq!(
        start.instructions().last().map(|value| value.opcode()),
        Some(Opcode::Halt)
    );
    assert!(!bytecode.constants().strings().is_empty());
    assert!(!bytecode.constants().i18n().is_empty());
}

#[test]
fn bytecode_passage_lookup_remains_case_sensitive() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let sources: [Source; 1] = [source];
    let ast: twee::Story<'_> = twee::Story::build(&sources).expect("Twee 应可编译");
    let hir: HirStory<'_> = HirStory::lower(&ast).expect("AST 应进入 HIR");
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("HIR 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: BytecodeProgram = BytecodeProgram::compile(&lir);

    assert!(bytecode.passage("Start").is_some());
    assert!(bytecode.passage("start").is_none());
}

#[test]
fn bytecode_instructions_own_serializable_expression_operands() {
    let sources = SourceList::discover("examples").expect("完整示例应可读取");
    let ast = twee::Story::build(&sources.items).expect("Twee 应可编译");
    let hir = HirStory::lower(&ast).expect("AST 应进入 HIR");
    let mir = MirStory::lower(&hir).expect("HIR 应进入 MIR");
    let lir = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode = BytecodeProgram::compile(&lir);

    let operands: Vec<_> = bytecode
        .passages()
        .iter()
        .flat_map(|passage| passage.instructions())
        .flat_map(|instruction| instruction.expressions())
        .collect();
    assert!(!operands.is_empty());
    for operand in operands {
        let encoded = serde_json::to_string(operand).expect("表达式操作数应可序列化");
        let decoded: crate::expression::OwnedExpression =
            serde_json::from_str(&encoded).expect("表达式操作数应可反序列化");
        assert_eq!(decoded, *operand);
    }
}

#[test]
fn bytecode_instruction_metadata_is_owned_and_serializable() {
    let sources = SourceList::discover("examples").expect("完整示例应可读取");
    let ast = twee::Story::build(&sources.items).expect("Twee 应可编译");
    let hir = HirStory::lower(&ast).expect("AST 应进入 HIR");
    let mir = MirStory::lower(&hir).expect("HIR 应进入 MIR");
    let lir = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode = BytecodeProgram::compile(&lir);

    for instruction in bytecode
        .passages()
        .iter()
        .flat_map(|passage| passage.instructions())
    {
        let encoded = serde_json::to_string(instruction.operation()).expect("指令元数据应可序列化");
        let decoded: crate::bytecode::BytecodeOperation =
            serde_json::from_str(&encoded).expect("指令元数据应可反序列化");
        assert_eq!(&decoded, instruction.operation());
    }
}

#[test]
fn bytecode_owns_complete_serializable_macro_calls() {
    let sources = SourceList::discover("examples").expect("完整示例应可读取");
    let ast = twee::Story::build(&sources.items).expect("Twee 应可编译");
    let hir = HirStory::lower(&ast).expect("AST 应进入 HIR");
    let mir = MirStory::lower(&hir).expect("HIR 应进入 MIR");
    let lir = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode = BytecodeProgram::compile(&lir);

    let call = bytecode
        .passages()
        .iter()
        .flat_map(|passage| passage.instructions())
        .find_map(|instruction| instruction.macro_call())
        .expect("示例应包含动态 Macro");
    let encoded = serde_json::to_string(call).expect("Macro 调用应可序列化");
    let decoded: crate::hir::OwnedHirMacro =
        serde_json::from_str(&encoded).expect("Macro 调用应可反序列化");

    assert_eq!(decoded, *call);
    assert_eq!(decoded.as_hir(), call.as_hir());
}

#[test]
fn complete_bytecode_program_round_trips_after_compiler_inputs_are_dropped() {
    let encoded = {
        let sources = SourceList::discover("examples").expect("完整示例应可读取");
        let ast = twee::Story::build(&sources.items).expect("Twee 应可编译");
        let hir = HirStory::lower(&ast).expect("AST 应进入 HIR");
        let mir = MirStory::lower(&hir).expect("HIR 应进入 MIR");
        let lir = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
        BytecodeProgram::compile(&lir)
            .to_json()
            .expect("完整 Bytecode 应可序列化")
    };

    let decoded = BytecodeProgram::from_json(&encoded).expect("Bytecode 应可独立反序列化");

    assert!(decoded.passage("Hall").is_some());
    assert!(!decoded.i18n().messages().is_empty());
    assert!(
        decoded
            .passages()
            .iter()
            .flat_map(|passage| passage.instructions())
            .any(|instruction| instruction.macro_call().is_some())
    );
}

#[test]
fn bytecode_decoder_rejects_a_tampered_format_header() {
    let sources = SourceList::discover("src/tests/fixtures/game").expect("fixture 应可读取");
    let ast = twee::Story::build(&sources.items).expect("Twee 应可编译");
    let hir = HirStory::lower(&ast).expect("AST 应进入 HIR");
    let mir = MirStory::lower(&hir).expect("HIR 应进入 MIR");
    let lir = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let encoded = BytecodeProgram::compile(&lir)
        .to_json()
        .expect("Bytecode 应可序列化");
    let mut value: serde_json::Value =
        serde_json::from_slice(&encoded).expect("测试 Bytecode JSON 应有效");
    value["header"]["magic"] = serde_json::json!([0, 0, 0, 0]);
    let tampered = serde_json::to_vec(&value).expect("篡改后的测试 JSON 应可编码");

    assert_eq!(
        BytecodeProgram::from_json(&tampered).unwrap_err(),
        crate::bytecode::BytecodeDecodeError::InvalidMagic
    );
}

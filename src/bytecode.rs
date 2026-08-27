//! LIR 到 VM 之间的不可变内存 Bytecode。
//!
//! 编译时一次性建立拥有型指令、常量目录与 Passage 名称索引；反序列化时校验
//! 文件头、目录一致性与每条指令的布局。

use std::{
    collections::{BTreeMap, HashSet},
    error::Error,
    fmt,
};

use serde::{Deserialize, Serialize};

mod operation;

use operation::own_expressions;
pub use operation::{BytecodeI18nPart, BytecodeMacroArguments, BytecodeOperation};

use crate::{
    expression::OwnedExpression,
    hir::OwnedHirMacro,
    i18n::I18nCatalog,
    lir::LirProgram,
    mir::{MirInstruction, MirInstructionPointer, MirMacroBody, MirPassageId},
};

/// Bytecode JSON 文件头的固定魔数。
pub const BYTECODE_MAGIC: [u8; 4] = *b"NRVA";
/// 当前 Bytecode 编码版本；解码时与文件头比对，不匹配即拒绝。
pub const BYTECODE_VERSION: u16 = 1;

/// Bytecode 解码或布局校验失败的原因。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BytecodeDecodeError {
    /// JSON 无法解析为 Bytecode 结构。
    InvalidJson(String),
    /// 文件头魔数不是 `BYTECODE_MAGIC`。
    InvalidMagic,
    /// 文件头版本不是 `BYTECODE_VERSION`。
    UnsupportedVersion(u16),
    /// 结构完整但内部布局不一致（目录大小、槽位或跳转越界等）。
    InvalidLayout(String),
}

impl fmt::Display for BytecodeDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Bytecode 解码失败: {self:?}")
    }
}

impl Error for BytecodeDecodeError {}

/// 指令类别标签；与 [`BytecodeOperation`] 一一对应，解码时交叉验证冗余信息。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Opcode {
    Text,
    HardBreak,
    PrintExpression,
    PrintLiteral,
    EvaluateDiscard,
    Unset,
    RequestInclude,
    RequestGoto,
    InvokeMacro,
    ExitPassage,
    Evaluate,
    PrepareCollectionIteration,
    PrepareRangeIteration,
    NextIteration,
    JumpIfFalse,
    JumpIfNotStrictEqual,
    Jump,
    Halt,
}

/// Bytecode 文件头：固定魔数与编码版本，解码时据此拒绝无效输入。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BytecodeHeader {
    pub magic: [u8; 4],
    pub version: u16,
}

/// 当前内存编码的拥有型常量目录。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BytecodeConstants {
    strings: Vec<String>,
    expressions: Vec<OwnedExpression>,
    macros: Vec<OwnedHirMacro>,
    i18n: Vec<BytecodeI18nPart>,
}

impl BytecodeConstants {
    /// 全部文本指令去重后的字符串常量。
    pub fn strings(&self) -> &[String] {
        &self.strings
    }
    /// 指令按求值顺序引用的表达式常量。
    pub fn expressions(&self) -> &[OwnedExpression] {
        &self.expressions
    }
    /// InvokeMacro 使用的拥有型 Macro 调用常量。
    pub fn macros(&self) -> &[OwnedHirMacro] {
        &self.macros
    }
    /// 指令附着的翻译消息身份常量。
    pub fn i18n(&self) -> &[BytecodeI18nPart] {
        &self.i18n
    }

    fn collect(&mut self, instruction: &MirInstruction<'_, '_>) {
        match instruction {
            MirInstruction::Text { text, .. } | MirInstruction::PrintLiteral { text, .. } => {
                push_value(&mut self.strings, (*text).to_owned())
            }
            MirInstruction::HardBreak { .. } => {}
            MirInstruction::PrintExpression { expression, .. }
            | MirInstruction::EvaluateDiscard(expression)
            | MirInstruction::Unset(expression)
            | MirInstruction::RequestGoto(expression) => {
                push_value(&mut self.expressions, OwnedExpression::from(*expression))
            }
            MirInstruction::RequestInclude { target, .. }
            | MirInstruction::Evaluate {
                expression: target, ..
            }
            | MirInstruction::PrepareCollectionIteration {
                collection: target, ..
            }
            | MirInstruction::NextIteration { target, .. }
            | MirInstruction::JumpIfFalse {
                condition: target, ..
            }
            | MirInstruction::JumpIfNotStrictEqual { right: target, .. } => {
                push_value(&mut self.expressions, OwnedExpression::from(*target))
            }
            MirInstruction::PrepareRangeIteration {
                start, end, step, ..
            } => {
                push_value(&mut self.expressions, OwnedExpression::from(*start));
                push_value(&mut self.expressions, OwnedExpression::from(*end));
                if let Some(step) = step {
                    push_value(&mut self.expressions, OwnedExpression::from(*step));
                }
            }
            MirInstruction::InvokeMacro { call, .. } => {
                push_value(&mut self.macros, OwnedHirMacro::from(*call))
            }
            _ => {}
        }
        if let Some(i18n) = instruction.i18n() {
            push_value(&mut self.i18n, BytecodeI18nPart::from(i18n));
        }
    }
}

/// 一条编码后的指令：操作元数据、按求值顺序的表达式与可选的 Macro 调用。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BytecodeInstruction {
    opcode: Opcode,
    operation: BytecodeOperation,
    expressions: Vec<OwnedExpression>,
    macro_call: Option<OwnedHirMacro>,
}

impl BytecodeInstruction {
    /// 指令的类别标签。
    pub fn opcode(&self) -> Opcode {
        self.opcode
    }
    /// 指令的执行元数据（控制流、输出模式、槽位与 Span）。
    pub fn operation(&self) -> &BytecodeOperation {
        &self.operation
    }
    /// 指令使用的表达式按求值顺序保存，不再依赖 HIR 生命周期。
    pub fn expressions(&self) -> &[OwnedExpression] {
        &self.expressions
    }
    /// InvokeMacro 指令的完整拥有型调用；其他指令为 None。
    pub fn macro_call(&self) -> Option<&OwnedHirMacro> {
        self.macro_call.as_ref()
    }
}

/// 编码后的 Passage：稳定 ID、区分大小写的名称、执行帧槽位数量与指令序列。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BytecodePassage {
    id: MirPassageId,
    name: String,
    value_slot_count: usize,
    iterator_slot_count: usize,
    instructions: Vec<BytecodeInstruction>,
}

impl BytecodePassage {
    /// 本次编译内的稳定 Passage 身份。
    pub fn id(&self) -> MirPassageId {
        self.id
    }
    /// 区分大小写的 Passage 名称。
    pub fn name(&self) -> &str {
        &self.name
    }
    /// 全部指令序列。
    pub fn instructions(&self) -> &[BytecodeInstruction] {
        &self.instructions
    }
    /// 按指令位置查询单条指令。
    pub fn instruction(&self, index: MirInstructionPointer) -> Option<&BytecodeInstruction> {
        self.instructions.get(index.index())
    }
    /// 执行帧可容纳的临时值槽数量。
    pub fn value_slot_count(&self) -> usize {
        self.value_slot_count
    }
    /// 执行帧可容纳的可暂停迭代槽数量。
    pub fn iterator_slot_count(&self) -> usize {
        self.iterator_slot_count
    }
}

/// 完整可执行的 Bytecode 程序：文件头、Passage 目录、名称索引、常量与 I18n。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BytecodeProgram {
    header: BytecodeHeader,
    passages: Vec<BytecodePassage>,
    names: BTreeMap<String, MirPassageId>,
    constants: BytecodeConstants,
    i18n: I18nCatalog,
}

impl BytecodeProgram {
    /// 从已通过地址校验的 LIR 编译：收集常量目录、建立名称索引并编码全部指令。
    pub fn compile(lir: &LirProgram<'_, '_, '_>) -> Self {
        let mut constants: BytecodeConstants = BytecodeConstants::default();
        let mut names: BTreeMap<String, MirPassageId> = BTreeMap::new();
        let passages: Vec<BytecodePassage> = lir
            .passages()
            .iter()
            .map(|passage| {
                names.insert(passage.name().to_owned(), passage.id());
                let instructions = passage
                    .instructions()
                    .iter()
                    .map(|instruction| {
                        constants.collect(instruction);
                        encode(instruction)
                    })
                    .collect();
                BytecodePassage {
                    id: passage.id(),
                    name: passage.name().to_owned(),
                    value_slot_count: passage.value_slot_count(),
                    iterator_slot_count: passage.iterator_slot_count(),
                    instructions,
                }
            })
            .collect();
        Self {
            header: BytecodeHeader {
                magic: BYTECODE_MAGIC,
                version: BYTECODE_VERSION,
            },
            passages,
            names,
            constants,
            i18n: lir.i18n().clone(),
        }
    }

    /// 返回文件头（魔数与版本）。
    pub fn header(&self) -> BytecodeHeader {
        self.header
    }
    /// 返回指令共享的常量目录。
    pub fn constants(&self) -> &BytecodeConstants {
        &self.constants
    }
    /// 按区分大小写的名称查询 Passage。
    pub fn passage(&self, name: &str) -> Option<&BytecodePassage> {
        self.passage_by_id(*self.names.get(name)?)
    }
    /// 按本次编译内的稳定 ID 查询 Passage。
    pub fn passage_by_id(&self, id: MirPassageId) -> Option<&BytecodePassage> {
        self.passages.iter().find(|passage| passage.id() == id)
    }
    /// 按编译顺序返回全部 Passage。
    pub fn passages(&self) -> &[BytecodePassage] {
        &self.passages
    }
    /// 返回程序附带的默认语言文本目录。
    pub fn i18n(&self) -> &I18nCatalog {
        &self.i18n
    }

    /// 把整个程序序列化为 JSON 字节；不执行布局校验。
    pub fn to_json(&self) -> Result<Vec<u8>, BytecodeDecodeError> {
        serde_json::to_vec(self)
            .map_err(|error| BytecodeDecodeError::InvalidJson(error.to_string()))
    }

    /// 从 JSON 字节解码并执行完整布局校验；任何不一致返回 BytecodeDecodeError。
    pub fn from_json(input: &[u8]) -> Result<Self, BytecodeDecodeError> {
        let program: Self = serde_json::from_slice(input)
            .map_err(|error| BytecodeDecodeError::InvalidJson(error.to_string()))?;
        program.validate()?;
        Ok(program)
    }

    fn validate(&self) -> Result<(), BytecodeDecodeError> {
        if self.header.magic != BYTECODE_MAGIC {
            return Err(BytecodeDecodeError::InvalidMagic);
        }
        if self.header.version != BYTECODE_VERSION {
            return Err(BytecodeDecodeError::UnsupportedVersion(self.header.version));
        }
        if self.names.len() != self.passages.len() {
            return Err(BytecodeDecodeError::InvalidLayout(String::from(
                "Passage 名称目录大小不一致",
            )));
        }
        let mut ids = HashSet::with_capacity(self.passages.len());
        for passage in &self.passages {
            if !ids.insert(passage.id) {
                return Err(BytecodeDecodeError::InvalidLayout(String::from(
                    "Passage ID 重复",
                )));
            }
            if self.names.get(&passage.name) != Some(&passage.id) {
                return Err(BytecodeDecodeError::InvalidLayout(format!(
                    "Passage `{}` 的名称目录无效",
                    passage.name
                )));
            }
            for instruction in &passage.instructions {
                instruction.validate(
                    passage.instructions.len(),
                    passage.value_slot_count,
                    passage.iterator_slot_count,
                )?;
            }
        }
        Ok(())
    }
}

impl BytecodeInstruction {
    fn validate(
        &self,
        instruction_count: usize,
        value_slot_count: usize,
        iterator_slot_count: usize,
    ) -> Result<(), BytecodeDecodeError> {
        if self.opcode != self.operation.opcode() {
            return Err(BytecodeDecodeError::InvalidLayout(String::from(
                "Opcode 与指令元数据不一致",
            )));
        }
        let expected_expressions = self.operation.expression_count();
        if self.expressions.len() != expected_expressions {
            return Err(BytecodeDecodeError::InvalidLayout(String::from(
                "指令表达式数量不一致",
            )));
        }
        if self.macro_call.is_some()
            != matches!(self.operation, BytecodeOperation::InvokeMacro { .. })
        {
            return Err(BytecodeDecodeError::InvalidLayout(String::from(
                "Macro 调用数据与指令不一致",
            )));
        }
        if let (
            BytecodeOperation::InvokeMacro {
                name,
                arguments,
                syntax_kind,
                ..
            },
            Some(call),
        ) = (&self.operation, &self.macro_call)
            && (name != &call.name
                || *syntax_kind != call.syntax_kind
                || !arguments.matches_owned(&call.arguments))
        {
            return Err(BytecodeDecodeError::InvalidLayout(String::from(
                "Macro 调用元数据不一致",
            )));
        }
        if let Some(slot) = self.operation.value_slot()
            && slot.index() >= value_slot_count
        {
            return Err(BytecodeDecodeError::InvalidLayout(String::from(
                "值槽越过执行帧边界",
            )));
        }
        if let Some(slot) = self.operation.iterator_slot()
            && slot.index() >= iterator_slot_count
        {
            return Err(BytecodeDecodeError::InvalidLayout(String::from(
                "迭代槽越过执行帧边界",
            )));
        }
        if let Some(target) = self.operation.jump_target()
            && target.index() >= instruction_count
        {
            return Err(BytecodeDecodeError::InvalidLayout(String::from(
                "跳转目标越过指令边界",
            )));
        }
        Ok(())
    }
}

/// 容器 Macro 延迟正文的编码形式：与 Passage 共享指令格式，但持有独立常量目录。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BytecodeMacroBody {
    value_slot_count: usize,
    iterator_slot_count: usize,
    instructions: Vec<BytecodeInstruction>,
    constants: BytecodeConstants,
}

impl BytecodeMacroBody {
    /// 从 MIR Macro 正文编译为拥有型指令序列与常量目录。
    pub fn compile(source: &MirMacroBody<'_, '_>) -> Self {
        let mut constants: BytecodeConstants = BytecodeConstants::default();
        let instructions = source
            .instructions()
            .iter()
            .map(|instruction| {
                constants.collect(instruction);
                encode(instruction)
            })
            .collect();
        Self {
            value_slot_count: source.value_slot_count(),
            iterator_slot_count: source.iterator_slot_count(),
            instructions,
            constants,
        }
    }
    /// 全部指令序列。
    pub fn instructions(&self) -> &[BytecodeInstruction] {
        &self.instructions
    }
    /// 按指令位置查询单条指令。
    pub fn instruction(&self, index: MirInstructionPointer) -> Option<&BytecodeInstruction> {
        self.instructions.get(index.index())
    }
    /// 本正文专用的常量目录。
    pub fn constants(&self) -> &BytecodeConstants {
        &self.constants
    }
    /// 执行帧可容纳的临时值槽数量。
    pub fn value_slot_count(&self) -> usize {
        self.value_slot_count
    }
    /// 执行帧可容纳的可暂停迭代槽数量。
    pub fn iterator_slot_count(&self) -> usize {
        self.iterator_slot_count
    }
}

fn encode(source: &MirInstruction<'_, '_>) -> BytecodeInstruction {
    let opcode = match source {
        MirInstruction::Text { .. } => Opcode::Text,
        MirInstruction::HardBreak { .. } => Opcode::HardBreak,
        MirInstruction::PrintExpression { .. } => Opcode::PrintExpression,
        MirInstruction::PrintLiteral { .. } => Opcode::PrintLiteral,
        MirInstruction::EvaluateDiscard(_) => Opcode::EvaluateDiscard,
        MirInstruction::Unset(_) => Opcode::Unset,
        MirInstruction::RequestInclude { .. } => Opcode::RequestInclude,
        MirInstruction::RequestGoto(_) => Opcode::RequestGoto,
        MirInstruction::InvokeMacro { .. } => Opcode::InvokeMacro,
        MirInstruction::ExitPassage => Opcode::ExitPassage,
        MirInstruction::Evaluate { .. } => Opcode::Evaluate,
        MirInstruction::PrepareCollectionIteration { .. } => Opcode::PrepareCollectionIteration,
        MirInstruction::PrepareRangeIteration { .. } => Opcode::PrepareRangeIteration,
        MirInstruction::NextIteration { .. } => Opcode::NextIteration,
        MirInstruction::JumpIfFalse { .. } => Opcode::JumpIfFalse,
        MirInstruction::JumpIfNotStrictEqual { .. } => Opcode::JumpIfNotStrictEqual,
        MirInstruction::Jump { .. } => Opcode::Jump,
        MirInstruction::Halt => Opcode::Halt,
    };
    BytecodeInstruction {
        opcode,
        operation: BytecodeOperation::from_mir(source),
        expressions: own_expressions(source),
        macro_call: match source {
            MirInstruction::InvokeMacro { call, .. } => Some(OwnedHirMacro::from(*call)),
            _ => None,
        },
    }
}

fn push_value<T: PartialEq>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) {
        values.push(value);
    }
}

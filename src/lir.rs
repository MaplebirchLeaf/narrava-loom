//! MIR 到 VM 可执行程序之间的低层边界。
//!
//! MIR 表达叙事控制流；LIR 在运行前建立稳定 Passage 索引并验证所有指令地址。
//! VM 因而只接收已经通过结构校验的程序。

use std::collections::BTreeMap;

use crate::{
    i18n::I18nCatalog,
    mir::{MirInstruction, MirPassage, MirPassageId, MirStory},
};

/// MIR 无法成为可执行程序的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LirLowerErrorKind {
    DuplicatePassage,
    InvalidInstructionTarget,
}

/// LIR lowering 错误保留发生问题的 Passage 与指令位置。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LirLowerError {
    passage: String,
    instruction: Option<usize>,
    kind: LirLowerErrorKind,
}

impl LirLowerError {
    /// 发生错误的 Passage 名称。
    pub fn passage(&self) -> &str {
        &self.passage
    }

    /// 出错指令在 Passage 内的下标；结构级错误为 None。
    pub fn instruction(&self) -> Option<usize> {
        self.instruction
    }

    /// 错误的具体类别。
    pub fn kind(&self) -> LirLowerErrorKind {
        self.kind
    }
}

/// 一份经过地址校验并建立 Passage 索引的 VM 程序。
///
/// 指令仍借用同一次编译产生的 MIR，避免在编译链中复制 Expression 与 Macro AST。
#[derive(Debug)]
pub struct LirProgram<'mir, 'hir, 'source> {
    mir: &'mir MirStory<'hir, 'source>,
    passages: BTreeMap<&'source str, MirPassageId>,
}

impl<'mir, 'hir, 'source> LirProgram<'mir, 'hir, 'source> {
    /// 建立 Passage 名称索引并验证全部跳转目标；任一失败返回 LirLowerError。
    pub fn lower(mir: &'mir MirStory<'hir, 'source>) -> Result<Self, LirLowerError> {
        let mut passages: BTreeMap<&'source str, MirPassageId> = BTreeMap::new();
        for passage in mir.passages() {
            if passages.insert(passage.name(), passage.id()).is_some() {
                return Err(LirLowerError {
                    passage: passage.name().to_owned(),
                    instruction: None,
                    kind: LirLowerErrorKind::DuplicatePassage,
                });
            }
            validate_targets(passage)?;
        }
        Ok(Self { mir, passages })
    }

    /// PassageName 区分大小写；索引只在本次 LIR 编译结果中有效。
    pub fn passage(&self, name: &str) -> Option<&'mir MirPassage<'hir, 'source>> {
        self.passage_by_id(*self.passages.get(name)?)
    }

    /// 按本次编译内的稳定 ID 查询 Passage。
    pub fn passage_by_id(&self, id: MirPassageId) -> Option<&'mir MirPassage<'hir, 'source>> {
        self.mir.passage_by_id(id)
    }

    /// 按 HIR 顺序返回全部 Passage。
    pub fn passages(&self) -> &'mir [MirPassage<'hir, 'source>] {
        self.mir.passages()
    }

    /// 返回与底层 MIR 同源的默认语言文本目录。
    pub fn i18n(&self) -> &I18nCatalog {
        self.mir.i18n()
    }
}

fn validate_targets(passage: &MirPassage<'_, '_>) -> Result<(), LirLowerError> {
    let instruction_count: usize = passage.instructions().len();
    for (index, instruction) in passage.instructions().iter().enumerate() {
        let target: Option<usize> = match instruction {
            MirInstruction::NextIteration { exhausted, .. }
            | MirInstruction::JumpIfFalse {
                target: exhausted, ..
            }
            | MirInstruction::JumpIfNotStrictEqual {
                target: exhausted, ..
            }
            | MirInstruction::Jump { target: exhausted } => Some(exhausted.index()),
            _ => None,
        };
        if target.is_some_and(|target: usize| target >= instruction_count) {
            return Err(LirLowerError {
                passage: passage.name().to_owned(),
                instruction: Some(index),
                kind: LirLowerErrorKind::InvalidInstructionTarget,
            });
        }
    }
    Ok(())
}

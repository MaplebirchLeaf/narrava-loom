//! HIR 结构到显式控制流之间的中层 IR 基础。
//!
//! MIR 把叙事正文编译为可暂停的顺序指令序列：控制流全部显式为跳转与迭代指令，
//! 临时值保存在指令槽中。Widget 与 Return 等尚无指令语义的 HIR 节点在 lowering
//! 时返回 `MirLowerError`，不允许退回递归执行。

mod lowering;

use lowering::attach_i18n;
pub use lowering::lower_body;

use crate::{
    expression::Expression,
    hir::{HirBodyNode, HirMacro, HirStory},
    i18n::{I18nCatalog, I18nTextId},
    twee::Span,
};

/// MIR 中一条可由程序计数位置指向的顺序指令。
#[derive(Debug, PartialEq, Eq)]
pub enum MirInstruction<'hir, 'source> {
    Text {
        text: &'source str,
        output: MirOutputMode,
        span: Span,
        i18n: Option<MirI18nTextPart>,
    },
    PrintExpression {
        expression: &'hir Expression<'source>,
        output: MirOutputMode,
        span: Span,
        i18n: Option<MirI18nTextPart>,
    },
    PrintLiteral {
        text: &'source str,
        output: MirOutputMode,
        span: Span,
        i18n: Option<MirI18nTextPart>,
    },
    /// 求值并丢弃结果；set 的赋值副作用由 Expression AST 自身表达。
    EvaluateDiscard(&'hir Expression<'source>),
    /// 删除 HIR 已验证的可写目标。
    Unset(&'hir Expression<'source>),
    /// 请求执行目标 Passage 后返回当前执行链。
    RequestInclude {
        target: &'hir Expression<'source>,
        output: MirOutputMode,
    },
    /// 请求结束当前 Passage 并由 Engine 确认导航。
    RequestGoto(&'hir Expression<'source>),
    /// 运行时再通过 Macro Definitions 解析的动态调用。
    InvokeMacro {
        call: &'hir HirMacro<'source>,
        /// 词法外层 `capture` 明确要求延长生命周期的 Macro Local 名称。
        captures: Vec<&'source str>,
        output: MirOutputMode,
    },
    /// 结束当前 Passage／include 调用帧；Widget 后续拥有自己的同类边界。
    ExitPassage,
    /// 求值一次并保存到当前 Passage 的临时值槽。
    Evaluate {
        expression: &'hir Expression<'source>,
        destination: MirValueSlot,
    },
    /// 只求值一次集合，并建立键或值迭代状态。
    PrepareCollectionIteration {
        collection: &'hir Expression<'source>,
        kind: MirCollectionIterationKind,
        destination: MirIteratorSlot,
    },
    /// 只求值一次 range 边界，并建立数值迭代状态。
    PrepareRangeIteration {
        start: &'hir Expression<'source>,
        end: &'hir Expression<'source>,
        step: Option<&'hir Expression<'source>>,
        destination: MirIteratorSlot,
    },
    /// 取得下一迭代值并写入 HIR 已验证目标；耗尽时跳到循环结束。
    NextIteration {
        iterator: MirIteratorSlot,
        target: &'hir Expression<'source>,
        exhausted: MirInstructionPointer,
    },
    /// 条件为假时把程序计数位置改为 target。
    JumpIfFalse {
        condition: &'hir Expression<'source>,
        target: MirInstructionPointer,
    },
    /// 右值与临时槽不严格相等时跳转；用于只求值一次的 switch 主值。
    JumpIfNotStrictEqual {
        left: MirValueSlot,
        right: &'hir Expression<'source>,
        target: MirInstructionPointer,
    },
    /// 无条件把程序计数位置改为 target。
    Jump { target: MirInstructionPointer },
    /// 当前指令序列的明确结束位置。
    Halt,
}

/// 当前指令产生的 Presentation 是否进入执行链输出。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MirOutputMode {
    /// 指令结果进入执行链输出。
    Visible,
    /// 指令结果被静默丢弃（如 `silently` 内部）。
    Suppressed,
}

/// 一条 MIR 输出片段在默认语言消息中的身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirI18nTextPart {
    id: I18nTextId,
    placeholder: Option<String>,
}

impl MirI18nTextPart {
    /// 默认语言消息的稳定 ID。
    pub fn id(&self) -> &I18nTextId {
        &self.id
    }

    /// 动态表达式片段对应的占位符名称；静态文本为 None。
    pub fn placeholder(&self) -> Option<&str> {
        self.placeholder.as_deref()
    }
}

impl MirInstruction<'_, '_> {
    /// 返回输出指令（Text/PrintExpression/PrintLiteral）附着的翻译消息身份。
    pub fn i18n(&self) -> Option<&MirI18nTextPart> {
        match self {
            Self::Text { i18n, .. }
            | Self::PrintExpression { i18n, .. }
            | Self::PrintLiteral { i18n, .. } => i18n.as_ref(),
            _ => None,
        }
    }
}

/// 尚未拥有 MIR 语义的 HIR 节点。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MirLowerError {
    pub kind: &'static str,
    pub span: Span,
}

/// MIR 指令序列中的稳定位置；边界检查集中在构造下一位置时完成。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MirInstructionPointer(usize);

impl MirInstructionPointer {
    /// 指向序列第一条指令。
    pub fn start() -> Self {
        Self(0)
    }

    /// 以 usize 返回当前指令位置。
    pub fn index(self) -> usize {
        self.0
    }

    /// 返回下一条有效指令的位置；当前已是最后一条时返回 None。
    pub fn next(self, instruction_count: usize) -> Option<Self> {
        let next: usize = self.0.checked_add(1)?;
        (next < instruction_count).then_some(Self(next))
    }
}

/// 当前 Passage 执行帧中的临时值位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MirValueSlot(usize);

impl MirValueSlot {
    /// 以 usize 返回值槽编号。
    pub fn index(self) -> usize {
        self.0
    }
}

/// for in 与 for of 对集合选择的迭代视图。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MirCollectionIterationKind {
    /// 遍历集合的键。
    Keys,
    /// 遍历集合的值。
    Values,
}

/// 当前 Passage 执行帧中的可暂停迭代状态位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MirIteratorSlot(usize);

impl MirIteratorSlot {
    /// 以 usize 返回迭代槽编号。
    pub fn index(self) -> usize {
        self.0
    }
}

/// 一段 MIR 指令及其执行帧所需的临时值槽数量。
#[derive(Debug, PartialEq, Eq)]
pub struct MirBody<'hir, 'source> {
    instructions: Vec<MirInstruction<'hir, 'source>>,
    value_slot_count: usize,
    iterator_slot_count: usize,
}

/// 容器 Macro 延迟正文对应的独立 MIR 可执行单元。
///
/// 它与 Passage 使用同一套指令语义，但不属于 Story Passage。VM 为其分配独立
/// 执行帧，使异步 Macro 能保存并恢复正文中的确切指令位置。
#[derive(Debug, PartialEq, Eq)]
pub struct MirMacroBody<'hir, 'source> {
    body: MirBody<'hir, 'source>,
}

impl<'hir, 'source> MirMacroBody<'hir, 'source> {
    /// 把延迟正文降低为可暂停的指令序列。
    pub fn lower(body: &'hir [HirBodyNode<'source>]) -> Result<Self, MirLowerError> {
        Ok(Self {
            body: lower_body(body)?,
        })
    }

    pub fn instructions(&self) -> &[MirInstruction<'hir, 'source>] {
        self.body.instructions()
    }

    pub fn instruction(
        &self,
        pointer: MirInstructionPointer,
    ) -> Option<&MirInstruction<'hir, 'source>> {
        self.body.instructions.get(pointer.index())
    }

    pub fn value_slot_count(&self) -> usize {
        self.body.value_slot_count()
    }

    pub fn iterator_slot_count(&self) -> usize {
        self.body.iterator_slot_count()
    }
}

impl<'hir, 'source> MirBody<'hir, 'source> {
    /// 全部指令序列，末尾固定为 `Halt`。
    pub fn instructions(&self) -> &[MirInstruction<'hir, 'source>] {
        &self.instructions
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

/// 一份 MIR Story 内的 Passage 身份；只在该编译结果中有效。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MirPassageId(usize);

impl MirPassageId {
    /// 以 usize 返回 Passage 身份编号。
    pub fn index(self) -> usize {
        self.0
    }
}

/// 一条可暂停执行位置，由 Passage 和其内部指令位置共同确定。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MirExecutionPosition {
    passage: MirPassageId,
    instruction: MirInstructionPointer,
}

impl MirExecutionPosition {
    /// 从 Passage 的第一条指令开始。
    pub fn start(passage: MirPassageId) -> Self {
        Self {
            passage,
            instruction: MirInstructionPointer::start(),
        }
    }

    /// 独立 Macro 正文不属于 Story Passage，只复用指令位置部分。
    pub(crate) fn macro_body() -> Self {
        Self {
            passage: MirPassageId(usize::MAX),
            instruction: MirInstructionPointer::start(),
        }
    }

    /// 当前所在的 Passage。
    pub fn passage(self) -> MirPassageId {
        self.passage
    }

    /// 当前所在的指令位置。
    pub fn instruction(self) -> MirInstructionPointer {
        self.instruction
    }

    /// 在同一 Passage 内替换指令位置（供 VM 实现跳转）。
    pub(crate) fn with_instruction(self, instruction: MirInstructionPointer) -> Self {
        Self {
            passage: self.passage,
            instruction,
        }
    }

    /// 在同一 Passage 内进入下一条有效指令。
    pub fn next(self, instruction_count: usize) -> Option<Self> {
        Some(Self {
            passage: self.passage,
            instruction: self.instruction.next(instruction_count)?,
        })
    }
}

/// 一个 Passage 的顺序 MIR 指令。
#[derive(Debug, PartialEq, Eq)]
pub struct MirPassage<'hir, 'source> {
    id: MirPassageId,
    name: &'source str,
    body: MirBody<'hir, 'source>,
}

impl<'hir, 'source> MirPassage<'hir, 'source> {
    /// 本次编译内的稳定 Passage 身份。
    pub fn id(&self) -> MirPassageId {
        self.id
    }

    /// 区分大小写的 Passage 名称。
    pub fn name(&self) -> &'source str {
        self.name
    }

    /// 全部指令序列。
    pub fn instructions(&self) -> &[MirInstruction<'hir, 'source>] {
        self.body.instructions()
    }

    /// 按指令位置查询单条指令。
    pub fn instruction(
        &self,
        pointer: MirInstructionPointer,
    ) -> Option<&MirInstruction<'hir, 'source>> {
        self.body.instructions.get(pointer.index())
    }

    /// 执行帧可容纳的临时值槽数量。
    pub fn value_slot_count(&self) -> usize {
        self.body.value_slot_count()
    }

    /// 执行帧可容纳的可暂停迭代槽数量。
    pub fn iterator_slot_count(&self) -> usize {
        self.body.iterator_slot_count()
    }
}

/// 同一份 HIR 编译结果产生的 MIR Passage 集合。
#[derive(Debug, PartialEq, Eq)]
pub struct MirStory<'hir, 'source> {
    passages: Vec<MirPassage<'hir, 'source>>,
    i18n: I18nCatalog,
}

impl<'hir, 'source> MirStory<'hir, 'source> {
    /// 以 HIR Passage 顺序分配本次编译内的身份，并降低全部正文。
    pub fn lower(hir: &'hir HirStory<'source>) -> Result<Self, MirLowerError> {
        let i18n: I18nCatalog = I18nCatalog::from_hir(hir);
        let mut passages: Vec<MirPassage<'hir, 'source>> = Vec::with_capacity(hir.passages.len());
        for (index, passage) in hir.passages.iter().enumerate() {
            // `[widget]` Passage 只向 Macro Definitions 提供声明，不能把定义节点当正文执行。
            let mut body: MirBody<'hir, 'source> = if passage.has_tag("widget") {
                lower_body(&[])?
            } else {
                lower_body(&passage.body)?
            };
            attach_i18n(&mut body, passage.source.as_str(), passage.name, &i18n);
            passages.push(MirPassage {
                id: MirPassageId(index),
                name: passage.name,
                body,
            });
        }
        Ok(Self { passages, i18n })
    }

    /// PassageName 区分大小写，与 HIR 和 Story 导航规则一致。
    pub fn passage(&self, name: &str) -> Option<&MirPassage<'hir, 'source>> {
        self.passages.iter().find(|passage| passage.name == name)
    }

    /// 解析当前编译结果内的 PassageId；用于 VM 恢复调用栈。
    pub fn passage_by_id(&self, id: MirPassageId) -> Option<&MirPassage<'hir, 'source>> {
        self.passages.get(id.0)
    }

    /// 按 HIR 顺序返回全部 Passage。
    pub fn passages(&self) -> &[MirPassage<'hir, 'source>] {
        &self.passages
    }

    /// 与该 MIR 由同一份 HIR 生成的默认语言文本目录。
    pub fn i18n(&self) -> &I18nCatalog {
        &self.i18n
    }
}

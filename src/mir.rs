//! HIR 结构到显式控制流之间的中层 IR 基础。
//!
//! 当前只降低没有控制流歧义的顺序节点。其他 HIR 节点必须等对应跳转结构
//! 定义后再加入，不能被跳过或退回递归执行。

use crate::{
    expression::Expression,
    hir::{HirBodyKind, HirBodyNode, HirFor, HirForKind, HirMacro, HirPrint, HirStory},
    i18n::{I18nCatalog, I18nMessage, I18nTextId},
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
    Visible,
    Suppressed,
}

/// 一条 MIR 输出片段在默认语言消息中的身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirI18nTextPart {
    id: I18nTextId,
    placeholder: Option<String>,
}

impl MirI18nTextPart {
    pub fn id(&self) -> &I18nTextId {
        &self.id
    }

    pub fn placeholder(&self) -> Option<&str> {
        self.placeholder.as_deref()
    }
}

impl MirInstruction<'_, '_> {
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
    pub fn start() -> Self {
        Self(0)
    }

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
    pub fn index(self) -> usize {
        self.0
    }
}

/// for in 与 for of 对集合选择的迭代视图。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MirCollectionIterationKind {
    Keys,
    Values,
}

/// 当前 Passage 执行帧中的可暂停迭代状态位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MirIteratorSlot(usize);

impl MirIteratorSlot {
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
/// 它与 Passage 使用同一套指令语义，但尚不冒充 Story Passage。后续 VM 会为该
/// 单元分配独立执行身份，使异步 Macro 能保存并恢复正文中的确切指令位置。
#[derive(Debug, PartialEq, Eq)]
pub struct MirMacroBody<'hir, 'source> {
    body: MirBody<'hir, 'source>,
}

impl<'hir, 'source> MirMacroBody<'hir, 'source> {
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
    pub fn instructions(&self) -> &[MirInstruction<'hir, 'source>] {
        &self.instructions
    }

    pub fn value_slot_count(&self) -> usize {
        self.value_slot_count
    }

    pub fn iterator_slot_count(&self) -> usize {
        self.iterator_slot_count
    }
}

/// 一份 MIR Story 内的 Passage 身份；只在该编译结果中有效。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MirPassageId(usize);

impl MirPassageId {
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

    pub fn passage(self) -> MirPassageId {
        self.passage
    }

    pub fn instruction(self) -> MirInstructionPointer {
        self.instruction
    }

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
    pub fn id(&self) -> MirPassageId {
        self.id
    }

    pub fn name(&self) -> &'source str {
        self.name
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

    pub fn passages(&self) -> &[MirPassage<'hir, 'source>] {
        &self.passages
    }

    /// 与该 MIR 由同一份 HIR 生成的默认语言文本目录。
    pub fn i18n(&self) -> &I18nCatalog {
        &self.i18n
    }
}

/// 把一段纯顺序 HIR 降为带明确 Halt 的 MIR 指令序列。
pub fn lower_body<'hir, 'source>(
    body: &'hir [HirBodyNode<'source>],
) -> Result<MirBody<'hir, 'source>, MirLowerError> {
    let mut context: MirLoweringContext<'hir, 'source> = MirLoweringContext {
        instructions: Vec::with_capacity(body.len() + 1),
        value_slot_count: 0,
        iterator_slot_count: 0,
        loops: Vec::new(),
        silence_depth: 0,
        captures: Vec::new(),
    };
    lower_nodes(body, &mut context)?;
    context.instructions.push(MirInstruction::Halt);
    Ok(MirBody {
        instructions: context.instructions,
        value_slot_count: context.value_slot_count,
        iterator_slot_count: context.iterator_slot_count,
    })
}

fn attach_i18n(body: &mut MirBody<'_, '_>, source: &str, passage: &str, catalog: &I18nCatalog) {
    let messages: Vec<&I18nMessage> = catalog
        .messages()
        .iter()
        .filter(|message: &&I18nMessage| message.source() == source && message.passage() == passage)
        .collect();
    let groups: Vec<Vec<usize>> = visible_text_groups(&body.instructions);

    for (group, message) in groups.into_iter().zip(messages) {
        let mut placeholders = message.placeholders().iter();
        for index in group {
            let instruction: &mut MirInstruction<'_, '_> = &mut body.instructions[index];
            let (i18n, is_expression): (&mut Option<MirI18nTextPart>, bool) = match instruction {
                MirInstruction::Text { i18n, .. } | MirInstruction::PrintLiteral { i18n, .. } => {
                    (i18n, false)
                }
                MirInstruction::PrintExpression { i18n, .. } => (i18n, true),
                _ => unreachable!("I18n 文本组只能包含输出指令"),
            };
            let placeholder: Option<String> = is_expression.then(|| {
                placeholders
                    .next()
                    .expect("I18n 表达式片段必须保留 placeholder")
                    .name()
                    .to_owned()
            });
            *i18n = Some(MirI18nTextPart {
                id: message.id().clone(),
                placeholder,
            });
        }
    }
}

fn visible_text_groups(instructions: &[MirInstruction<'_, '_>]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut has_static_text: bool = false;

    for (index, instruction) in instructions.iter().enumerate() {
        let static_part: Option<bool> = match instruction {
            MirInstruction::Text {
                text,
                output: MirOutputMode::Visible,
                ..
            }
            | MirInstruction::PrintLiteral {
                text,
                output: MirOutputMode::Visible,
                ..
            } => Some(!text.trim().is_empty()),
            MirInstruction::PrintExpression {
                output: MirOutputMode::Visible,
                ..
            } => Some(false),
            _ => None,
        };
        if let Some(is_static) = static_part {
            current.push(index);
            has_static_text |= is_static;
        } else {
            push_visible_group(&mut groups, &mut current, &mut has_static_text);
        }
    }
    push_visible_group(&mut groups, &mut current, &mut has_static_text);
    groups
}

fn push_visible_group(
    groups: &mut Vec<Vec<usize>>,
    current: &mut Vec<usize>,
    has_static_text: &mut bool,
) {
    if *has_static_text {
        groups.push(std::mem::take(current));
    } else {
        current.clear();
    }
    *has_static_text = false;
}

struct MirLoweringContext<'hir, 'source> {
    instructions: Vec<MirInstruction<'hir, 'source>>,
    value_slot_count: usize,
    iterator_slot_count: usize,
    loops: Vec<MirLoopFrame>,
    silence_depth: usize,
    captures: Vec<&'source str>,
}

struct MirLoopFrame {
    continue_target: MirInstructionPointer,
    break_jumps: Vec<usize>,
}

impl MirLoweringContext<'_, '_> {
    fn allocate_value_slot(&mut self) -> MirValueSlot {
        let slot: MirValueSlot = MirValueSlot(self.value_slot_count);
        self.value_slot_count = self
            .value_slot_count
            .checked_add(1)
            .expect("MIR 临时值槽不可能超过地址空间");
        slot
    }

    fn allocate_iterator_slot(&mut self) -> MirIteratorSlot {
        let slot: MirIteratorSlot = MirIteratorSlot(self.iterator_slot_count);
        self.iterator_slot_count = self
            .iterator_slot_count
            .checked_add(1)
            .expect("MIR 迭代槽不可能超过地址空间");
        slot
    }

    fn output_mode(&self) -> MirOutputMode {
        if self.silence_depth == 0 {
            MirOutputMode::Visible
        } else {
            MirOutputMode::Suppressed
        }
    }
}

fn lower_nodes<'hir, 'source>(
    body: &'hir [HirBodyNode<'source>],
    context: &mut MirLoweringContext<'hir, 'source>,
) -> Result<(), MirLowerError> {
    for node in body {
        match &node.kind {
            HirBodyKind::Text(text) => context.instructions.push(MirInstruction::Text {
                text,
                output: context.output_mode(),
                span: node.span,
                i18n: None,
            }),
            HirBodyKind::Print(HirPrint::Expression(expression)) => {
                context.instructions.push(MirInstruction::PrintExpression {
                    expression,
                    output: context.output_mode(),
                    span: node.span,
                    i18n: None,
                });
            }
            HirBodyKind::Print(HirPrint::Literal(text)) => {
                context.instructions.push(MirInstruction::PrintLiteral {
                    text,
                    output: context.output_mode(),
                    span: node.span,
                    i18n: None,
                });
            }
            HirBodyKind::Silently(body) => lower_silently(body, context)?,
            HirBodyKind::Set(expression) | HirBodyKind::Run(expression) => context
                .instructions
                .push(MirInstruction::EvaluateDiscard(expression)),
            HirBodyKind::Unset(target) => {
                context.instructions.push(MirInstruction::Unset(target));
            }
            HirBodyKind::Include(target) => {
                context.instructions.push(MirInstruction::RequestInclude {
                    target,
                    output: context.output_mode(),
                });
            }
            HirBodyKind::Goto(target) => {
                context
                    .instructions
                    .push(MirInstruction::RequestGoto(target));
            }
            HirBodyKind::Exit => context.instructions.push(MirInstruction::ExitPassage),
            HirBodyKind::Macro(call) => context.instructions.push(MirInstruction::InvokeMacro {
                call,
                captures: context.captures.clone(),
                output: context.output_mode(),
            }),
            HirBodyKind::Capture(capture) => {
                let previous_len: usize = context.captures.len();
                context.captures.extend(capture.locals.iter().copied());
                lower_nodes(&capture.body, context)?;
                context.captures.truncate(previous_len);
            }
            HirBodyKind::If(conditional) => lower_if(conditional, context)?,
            HirBodyKind::Switch(switch) => lower_switch(switch, context)?,
            HirBodyKind::While(loop_node) => lower_while(loop_node, context)?,
            HirBodyKind::For(loop_node) => lower_for(loop_node, context)?,
            HirBodyKind::Break => lower_break(node.span, context)?,
            HirBodyKind::Continue => lower_continue(node.span, context)?,
            kind => {
                return Err(MirLowerError {
                    kind: hir_kind_name(kind),
                    span: node.span,
                });
            }
        }
    }
    Ok(())
}

fn lower_silently<'hir, 'source>(
    body: &'hir [HirBodyNode<'source>],
    context: &mut MirLoweringContext<'hir, 'source>,
) -> Result<(), MirLowerError> {
    context.silence_depth = context
        .silence_depth
        .checked_add(1)
        .expect("silently 嵌套深度不可能超过地址空间");
    let lowered: Result<(), MirLowerError> = lower_nodes(body, context);
    context.silence_depth -= 1;
    lowered
}

/// 每个真分支在结束时跳到整个 if 之后；假条件跳到下一分支或 fallback。
fn lower_if<'hir, 'source>(
    conditional: &'hir crate::hir::HirIf<'source>,
    context: &mut MirLoweringContext<'hir, 'source>,
) -> Result<(), MirLowerError> {
    let mut end_jumps: Vec<usize> = Vec::with_capacity(conditional.branches.len());
    for branch in &conditional.branches {
        let condition_index: usize = context.instructions.len();
        context.instructions.push(MirInstruction::JumpIfFalse {
            condition: &branch.condition,
            target: MirInstructionPointer::start(),
        });
        lower_nodes(&branch.body, context)?;
        push_end_jump(context, &mut end_jumps);

        let next_branch: MirInstructionPointer = MirInstructionPointer(context.instructions.len());
        let MirInstruction::JumpIfFalse { target, .. } = &mut context.instructions[condition_index]
        else {
            unreachable!("刚写入的条件跳转必须仍在原位置")
        };
        *target = next_branch;
    }

    if let Some(fallback) = &conditional.fallback {
        lower_nodes(fallback, context)?;
    }

    patch_end_jumps(context, end_jumps);
    Ok(())
}

/// switch 主值只进入一个临时槽；case 按源码顺序严格比较且不贯穿。
fn lower_switch<'hir, 'source>(
    switch: &'hir crate::hir::HirSwitch<'source>,
    context: &mut MirLoweringContext<'hir, 'source>,
) -> Result<(), MirLowerError> {
    let selected: MirValueSlot = context.allocate_value_slot();
    context.instructions.push(MirInstruction::Evaluate {
        expression: &switch.value,
        destination: selected,
    });

    let mut end_jumps: Vec<usize> = Vec::with_capacity(switch.cases.len());
    for case in &switch.cases {
        let comparison_index: usize = context.instructions.len();
        context
            .instructions
            .push(MirInstruction::JumpIfNotStrictEqual {
                left: selected,
                right: &case.value,
                target: MirInstructionPointer::start(),
            });
        lower_nodes(&case.body, context)?;
        push_end_jump(context, &mut end_jumps);

        let next_case: MirInstructionPointer = MirInstructionPointer(context.instructions.len());
        let MirInstruction::JumpIfNotStrictEqual { target, .. } =
            &mut context.instructions[comparison_index]
        else {
            unreachable!("刚写入的 case 跳转必须仍在原位置")
        };
        *target = next_case;
    }

    if let Some(default) = &switch.default {
        lower_nodes(default, context)?;
    }

    patch_end_jumps(context, end_jumps);
    Ok(())
}

/// while 的条件位置同时是 continue 目标；条件为假与 break 共用结束目标。
fn lower_while<'hir, 'source>(
    loop_node: &'hir crate::hir::HirWhile<'source>,
    context: &mut MirLoweringContext<'hir, 'source>,
) -> Result<(), MirLowerError> {
    let condition_target: MirInstructionPointer = MirInstructionPointer(context.instructions.len());
    let condition_index: usize = context.instructions.len();
    context.instructions.push(MirInstruction::JumpIfFalse {
        condition: &loop_node.condition,
        target: MirInstructionPointer::start(),
    });

    context.loops.push(MirLoopFrame {
        continue_target: condition_target,
        break_jumps: Vec::new(),
    });
    let lowered: Result<(), MirLowerError> = lower_nodes(&loop_node.body, context);
    let frame: MirLoopFrame = context
        .loops
        .pop()
        .expect("while lowering 必须保留当前循环帧");
    lowered?;

    context.instructions.push(MirInstruction::Jump {
        target: condition_target,
    });
    let end: MirInstructionPointer = MirInstructionPointer(context.instructions.len());
    let MirInstruction::JumpIfFalse { target, .. } = &mut context.instructions[condition_index]
    else {
        unreachable!("while 条件跳转必须仍在原位置")
    };
    *target = end;
    patch_jumps(context, frame.break_jumps, end);
    Ok(())
}

/// 三种 for 共用可暂停迭代槽和 NextIteration 循环边界。
fn lower_for<'hir, 'source>(
    loop_node: &'hir HirFor<'source>,
    context: &mut MirLoweringContext<'hir, 'source>,
) -> Result<(), MirLowerError> {
    let iterator: MirIteratorSlot = context.allocate_iterator_slot();
    match &loop_node.kind {
        HirForKind::In { collection, .. } => {
            context
                .instructions
                .push(MirInstruction::PrepareCollectionIteration {
                    collection,
                    kind: MirCollectionIterationKind::Keys,
                    destination: iterator,
                });
        }
        HirForKind::Of { collection, .. } => {
            context
                .instructions
                .push(MirInstruction::PrepareCollectionIteration {
                    collection,
                    kind: MirCollectionIterationKind::Values,
                    destination: iterator,
                });
        }
        HirForKind::Range {
            start, end, step, ..
        } => context
            .instructions
            .push(MirInstruction::PrepareRangeIteration {
                start,
                end,
                step: step.as_ref(),
                destination: iterator,
            }),
    }

    let next_target: MirInstructionPointer = MirInstructionPointer(context.instructions.len());
    let next_index: usize = context.instructions.len();
    context.instructions.push(MirInstruction::NextIteration {
        iterator,
        target: &loop_node.target.value,
        exhausted: MirInstructionPointer::start(),
    });

    context.loops.push(MirLoopFrame {
        continue_target: next_target,
        break_jumps: Vec::new(),
    });
    let lowered: Result<(), MirLowerError> = lower_nodes(&loop_node.body, context);
    let frame: MirLoopFrame = context
        .loops
        .pop()
        .expect("for lowering 必须保留当前循环帧");
    lowered?;

    context.instructions.push(MirInstruction::Jump {
        target: next_target,
    });
    let end: MirInstructionPointer = MirInstructionPointer(context.instructions.len());
    let MirInstruction::NextIteration { exhausted, .. } = &mut context.instructions[next_index]
    else {
        unreachable!("for NextIteration 必须仍在原位置")
    };
    *exhausted = end;
    patch_jumps(context, frame.break_jumps, end);
    Ok(())
}

fn lower_break(span: Span, context: &mut MirLoweringContext<'_, '_>) -> Result<(), MirLowerError> {
    let Some(frame) = context.loops.last_mut() else {
        return Err(MirLowerError {
            kind: "break",
            span,
        });
    };
    frame.break_jumps.push(context.instructions.len());
    context.instructions.push(MirInstruction::Jump {
        target: MirInstructionPointer::start(),
    });
    Ok(())
}

fn lower_continue(
    span: Span,
    context: &mut MirLoweringContext<'_, '_>,
) -> Result<(), MirLowerError> {
    let Some(frame) = context.loops.last() else {
        return Err(MirLowerError {
            kind: "continue",
            span,
        });
    };
    context.instructions.push(MirInstruction::Jump {
        target: frame.continue_target,
    });
    Ok(())
}

fn push_end_jump(context: &mut MirLoweringContext<'_, '_>, end_jumps: &mut Vec<usize>) {
    end_jumps.push(context.instructions.len());
    context.instructions.push(MirInstruction::Jump {
        target: MirInstructionPointer::start(),
    });
}

fn patch_end_jumps(context: &mut MirLoweringContext<'_, '_>, end_jumps: Vec<usize>) {
    let end: MirInstructionPointer = MirInstructionPointer(context.instructions.len());
    patch_jumps(context, end_jumps, end);
}

fn patch_jumps(
    context: &mut MirLoweringContext<'_, '_>,
    jumps: Vec<usize>,
    target_position: MirInstructionPointer,
) {
    for index in jumps {
        let MirInstruction::Jump { target } = &mut context.instructions[index] else {
            unreachable!("分支结束跳转必须仍在原位置")
        };
        *target = target_position;
    }
}

fn hir_kind_name(kind: &HirBodyKind<'_>) -> &'static str {
    match kind {
        HirBodyKind::Text(_) => "text",
        HirBodyKind::Print(_) => "print",
        HirBodyKind::Silently(_) => "silently",
        HirBodyKind::If(_) => "if",
        HirBodyKind::Switch(_) => "switch",
        HirBodyKind::For(_) => "for",
        HirBodyKind::While(_) => "while",
        HirBodyKind::Break => "break",
        HirBodyKind::Continue => "continue",
        HirBodyKind::Exit => "exit",
        HirBodyKind::Set(_) => "set",
        HirBodyKind::Unset(_) => "unset",
        HirBodyKind::Run(_) => "run",
        HirBodyKind::Include(_) => "include",
        HirBodyKind::Goto(_) => "goto",
        HirBodyKind::Widget(_) => "widget",
        HirBodyKind::Return(_) => "return",
        HirBodyKind::Capture(_) => "capture",
        HirBodyKind::Macro(_) => "macro",
    }
}

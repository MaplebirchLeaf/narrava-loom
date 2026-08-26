//! HIR 正文到显式 MIR 控制流的 lowering。
//!
//! 本模块拥有槽位分配、循环帧、跳转回填和 I18n 输出身份附着。父模块只保留
//! MIR 数据契约与 Story/Passage 容器，避免公开类型和编译算法继续堆在同一文件。

use super::*;
use crate::{
    hir::{HirBodyKind, HirFor, HirForKind, HirPrint},
    i18n::I18nMessage,
};

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

pub(super) fn attach_i18n(
    body: &mut MirBody<'_, '_>,
    source: &str,
    passage: &str,
    catalog: &I18nCatalog,
) {
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

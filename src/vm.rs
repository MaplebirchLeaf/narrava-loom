//! 已编码 Bytecode 程序的最小单步执行帧。

use std::collections::BTreeMap;

mod execution;

use crate::{
    bytecode::{
        BytecodeInstruction, BytecodeMacroBody, BytecodeOperation, BytecodePassage, BytecodeProgram,
    },
    expression::{
        Span,
        evaluator::{
            EvalError, WritableEvaluationContext, assign_value_with_mut, delete_with_mut,
            evaluate_with_mut, value_to_text, values_strict_equal,
        },
        value::{TextValue, Value},
    },
    i18n::I18nRuntimeLanguage,
    mir::{
        MirCollectionIterationKind, MirExecutionPosition, MirIteratorSlot, MirOutputMode,
        MirValueSlot,
    },
    presentation::{PresentationNode, PresentationOutput},
    runtime::{collection_iteration_values, finite_range_number},
};

/// 单步完成后执行帧是否仍可继续。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirStep {
    Running,
    Halted,
    NavigationPending,
    MacroPending,
}

/// LIR 单步执行不能继续的稳定原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirExecutionError {
    MissingPassage,
    DifferentI18nCatalog,
    InvalidInstructionPointer,
    MissingValueSlot(MirValueSlot),
    MissingIteratorSlot(MirIteratorSlot),
    InvalidText(Span),
    Evaluation(EvalError),
    ExpectedMacroPending,
    MacroBodyIncludeUnsupported,
}

/// 一条执行链的 Passage 调用栈、运行状态与累计语义输出。
///
/// 异步 Macro 和 Engine 事务检查点尚未接入。
#[derive(Debug, PartialEq)]
pub struct MirExecutionFrame {
    stack: Vec<MirPassageFrame>,
    output: PresentationOutput,
    navigation: Option<String>,
    includes_entered: usize,
}

#[derive(Debug, PartialEq)]
struct MirPassageFrame {
    location: MirExecutionPosition,
    values: Vec<Option<Value>>,
    iterators: Vec<Option<MirIteratorState>>,
    output_suppressed: bool,
}

impl MirPassageFrame {
    fn new(passage: &BytecodePassage, output_suppressed: bool) -> Self {
        let values: Vec<Option<Value>> = std::iter::repeat_with(|| None)
            .take(passage.value_slot_count())
            .collect();
        let iterators: Vec<Option<MirIteratorState>> = std::iter::repeat_with(|| None)
            .take(passage.iterator_slot_count())
            .collect();
        Self {
            location: MirExecutionPosition::start(passage.id()),
            values,
            iterators,
            output_suppressed,
        }
    }

    fn new_macro(body: &BytecodeMacroBody) -> Self {
        let values: Vec<Option<Value>> = std::iter::repeat_with(|| None)
            .take(body.value_slot_count())
            .collect();
        let iterators: Vec<Option<MirIteratorState>> = std::iter::repeat_with(|| None)
            .take(body.iterator_slot_count())
            .collect();
        Self {
            location: MirExecutionPosition::macro_body(),
            values,
            iterators,
            output_suppressed: false,
        }
    }
}

#[derive(Debug, PartialEq)]
enum MirIteratorState {
    Collection {
        values: Vec<Value>,
        next: usize,
    },
    Range {
        current: f64,
        end: f64,
        step: f64,
        step_span: Span,
        finished: bool,
    },
}

impl MirIteratorState {
    fn next(&mut self) -> Result<Option<Value>, EvalError> {
        match self {
            Self::Collection { values, next } => {
                let value: Option<Value> = values.get(*next).cloned();
                if value.is_some() {
                    *next += 1;
                }
                Ok(value)
            }
            Self::Range {
                current,
                end,
                step,
                step_span,
                finished,
            } => {
                if *finished || (*step > 0.0 && *current > *end) || (*step < 0.0 && *current < *end)
                {
                    *finished = true;
                    return Ok(None);
                }
                let value: f64 = *current;
                if value == *end {
                    *finished = true;
                } else {
                    let next: f64 = value + *step;
                    if next == value {
                        return Err(EvalError::InvalidRange(*step_span));
                    }
                    *current = next;
                }
                Ok(Some(Value::Number(value)))
            }
        }
    }
}

impl MirExecutionFrame {
    pub fn new(passage: &BytecodePassage) -> Self {
        Self {
            stack: vec![MirPassageFrame::new(passage, false)],
            output: PresentationOutput::default(),
            navigation: None,
            includes_entered: 0,
        }
    }

    /// 建立不进入 Story history 的容器 Macro 正文执行帧。
    pub fn new_macro(body: &BytecodeMacroBody) -> Self {
        Self {
            stack: vec![MirPassageFrame::new_macro(body)],
            output: PresentationOutput::default(),
            navigation: None,
            includes_entered: 0,
        }
    }

    pub fn pending_macro_body<'hir>(
        &self,
        body: &'hir BytecodeMacroBody,
    ) -> Option<&'hir crate::hir::OwnedHirMacro> {
        body.instruction(self.location().instruction())?
            .macro_call()
    }

    pub fn pending_macro_body_captures<'hir>(
        &self,
        body: &'hir BytecodeMacroBody,
    ) -> Option<Vec<&'hir str>> {
        match body.instruction(self.location().instruction())?.operation() {
            BytecodeOperation::InvokeMacro { captures, .. } => {
                Some(captures.iter().map(String::as_str).collect())
            }
            _ => None,
        }
    }

    pub fn complete_macro_body(
        &mut self,
        body: &BytecodeMacroBody,
        output: PresentationOutput,
    ) -> Result<(), MirExecutionError> {
        let mode: MirOutputMode = match body
            .instruction(self.location().instruction())
            .map(BytecodeInstruction::operation)
        {
            Some(BytecodeOperation::InvokeMacro { output, .. }) => *output,
            Some(_) => return Err(MirExecutionError::ExpectedMacroPending),
            None => return Err(MirExecutionError::InvalidInstructionPointer),
        };
        if self.should_emit(mode) {
            self.output.append(output);
        }
        self.advance(body.instructions().len())
    }

    pub fn step_macro(
        &mut self,
        body: &BytecodeMacroBody,
        context: &mut dyn WritableEvaluationContext,
    ) -> Result<MirStep, MirExecutionError> {
        if self.navigation.is_some() {
            return Ok(MirStep::NavigationPending);
        }
        let instruction: &BytecodeInstruction = body
            .instruction(self.location().instruction())
            .ok_or(MirExecutionError::InvalidInstructionPointer)?;
        self.execute_instruction(None, body.instructions().len(), instruction, context)
    }

    pub fn location(&self) -> MirExecutionPosition {
        self.stack
            .last()
            .expect("执行链必须保留一个活动 Passage 帧")
            .location
    }

    pub fn output(&self) -> &PresentationOutput {
        &self.output
    }

    pub fn navigation(&self) -> Option<&str> {
        self.navigation.as_deref()
    }

    /// 读取当前位置尚未由 Macro 控制器处理的动态调用。
    pub fn pending_macro<'hir>(
        &self,
        story: &'hir BytecodeProgram,
    ) -> Option<&'hir crate::hir::OwnedHirMacro> {
        let location: MirExecutionPosition = self.location();
        let passage: &BytecodePassage = story.passage_by_id(location.passage())?;
        passage.instruction(location.instruction())?.macro_call()
    }

    /// 读取待处理 Macro 在源码词法位置上明确声明的捕获名称。
    pub fn pending_macro_captures<'hir>(
        &self,
        story: &'hir BytecodeProgram,
    ) -> Option<Vec<&'hir str>> {
        let location: MirExecutionPosition = self.location();
        let passage: &BytecodePassage = story.passage_by_id(location.passage())?;
        match passage.instruction(location.instruction())?.operation() {
            BytecodeOperation::InvokeMacro { captures, .. } => {
                Some(captures.iter().map(String::as_str).collect())
            }
            _ => None,
        }
    }

    /// 合并 Macro 控制器已经完成生命周期处理的独立输出，并推进调用位置。
    pub fn complete_macro(
        &mut self,
        story: &BytecodeProgram,
        output: PresentationOutput,
    ) -> Result<(), MirExecutionError> {
        let location: MirExecutionPosition = self.location();
        let passage: &BytecodePassage = story
            .passage_by_id(location.passage())
            .ok_or(MirExecutionError::MissingPassage)?;
        let mode: MirOutputMode = match passage
            .instruction(location.instruction())
            .map(BytecodeInstruction::operation)
        {
            Some(BytecodeOperation::InvokeMacro { output, .. }) => *output,
            Some(_) => return Err(MirExecutionError::ExpectedMacroPending),
            None => return Err(MirExecutionError::InvalidInstructionPointer),
        };
        if self.should_emit(mode) {
            self.output.append(output);
        }
        self.advance(passage.instructions().len())
    }

    pub fn includes_entered(&self) -> usize {
        self.includes_entered
    }

    pub fn into_output(self) -> PresentationOutput {
        self.output
    }

    /// 精确执行当前位置的一条指令；Halt 会返回调用者或稳定结束根 Passage。
    pub fn step(
        &mut self,
        story: &BytecodeProgram,
        context: &mut dyn WritableEvaluationContext,
    ) -> Result<MirStep, MirExecutionError> {
        self.step_with_language(story, None, context)
    }

    /// 使用 Engine 事务持有的语言选择执行一步。
    pub fn step_with_runtime_language(
        &mut self,
        story: &BytecodeProgram,
        language: &I18nRuntimeLanguage,
        context: &mut dyn WritableEvaluationContext,
    ) -> Result<MirStep, MirExecutionError> {
        self.step_with_language(story, Some(language), context)
    }

    fn advance(&mut self, instruction_count: usize) -> Result<(), MirExecutionError> {
        let next: MirExecutionPosition = self
            .location()
            .next(instruction_count)
            .ok_or(MirExecutionError::InvalidInstructionPointer)?;
        self.current_mut().location = next;
        Ok(())
    }

    fn set_iterator(
        &mut self,
        slot: MirIteratorSlot,
        state: MirIteratorState,
    ) -> Result<(), MirExecutionError> {
        let destination: &mut Option<MirIteratorState> = self
            .current_mut()
            .iterators
            .get_mut(slot.index())
            .ok_or(MirExecutionError::MissingIteratorSlot(slot))?;
        *destination = Some(state);
        Ok(())
    }

    fn current(&self) -> &MirPassageFrame {
        self.stack
            .last()
            .expect("执行链必须保留一个活动 Passage 帧")
    }

    fn current_mut(&mut self) -> &mut MirPassageFrame {
        self.stack
            .last_mut()
            .expect("执行链必须保留一个活动 Passage 帧")
    }

    fn should_emit(&self, mode: MirOutputMode) -> bool {
        !self.current().output_suppressed && mode == MirOutputMode::Visible
    }

    fn finish_current(&mut self) -> MirStep {
        if self.stack.len() == 1 {
            MirStep::Halted
        } else {
            let _completed: MirPassageFrame = self
                .stack
                .pop()
                .expect("include 结束必须可以弹出当前 Passage 帧");
            MirStep::Running
        }
    }
}

fn evaluate_passage_name(
    expression: &crate::expression::Expression<'_>,
    context: &mut dyn WritableEvaluationContext,
) -> Result<String, MirExecutionError> {
    let value: Value =
        evaluate_with_mut(expression, context).map_err(MirExecutionError::Evaluation)?;
    let text: TextValue =
        value_to_text(&value).ok_or(MirExecutionError::InvalidText(expression.span))?;
    text.to_unicode_string()
        .ok_or(MirExecutionError::InvalidText(expression.span))
}

//! 已编码 Bytecode 程序的最小单步执行帧。

use std::collections::BTreeMap;

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

    fn step_with_language(
        &mut self,
        story: &BytecodeProgram,
        language: Option<&I18nRuntimeLanguage>,
        context: &mut dyn WritableEvaluationContext,
    ) -> Result<MirStep, MirExecutionError> {
        if language.is_some_and(|language: &I18nRuntimeLanguage| !language.is_for(story.i18n())) {
            return Err(MirExecutionError::DifferentI18nCatalog);
        }
        if self.navigation.is_some() {
            return Ok(MirStep::NavigationPending);
        }
        let location: MirExecutionPosition = self.location();
        let passage: &BytecodePassage = story
            .passage_by_id(location.passage())
            .ok_or(MirExecutionError::MissingPassage)?;
        let instruction: &BytecodeInstruction = passage
            .instruction(location.instruction())
            .ok_or(MirExecutionError::InvalidInstructionPointer)?;

        if instruction.operation().i18n().is_some() {
            return self.execute_i18n_message(story, passage, language, context);
        }

        self.execute_instruction(
            Some(story),
            passage.instructions().len(),
            instruction,
            context,
        )
    }

    /// 同一消息的连续 MIR 片段必须作为一个翻译单元求值和输出。
    fn execute_i18n_message(
        &mut self,
        story: &BytecodeProgram,
        passage: &BytecodePassage,
        language: Option<&I18nRuntimeLanguage>,
        context: &mut dyn WritableEvaluationContext,
    ) -> Result<MirStep, MirExecutionError> {
        let start: usize = self.location().instruction().index();
        let first = passage
            .instructions()
            .get(start)
            .and_then(|instruction| instruction.operation().i18n())
            .expect("I18n 执行入口必须位于带身份的文本片段");
        let id: String = first.id().to_owned();
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        let mut part_count: usize = 0;

        for encoded in &passage.instructions()[start..] {
            let operation = encoded.operation();
            let Some(identity) = operation.i18n() else {
                break;
            };
            if identity.id() != id {
                break;
            }
            if let BytecodeOperation::PrintExpression { .. } = operation {
                let expression = encoded
                    .expressions()
                    .first()
                    .expect("PrintExpression Bytecode 必须拥有表达式")
                    .as_expression();
                let value: Value = evaluate_with_mut(&expression, context)
                    .map_err(MirExecutionError::Evaluation)?;
                let text: TextValue =
                    value_to_text(&value).ok_or(MirExecutionError::InvalidText(expression.span))?;
                let text: String = text
                    .to_unicode_string()
                    .ok_or(MirExecutionError::InvalidText(expression.span))?;
                let placeholder: &str = identity
                    .placeholder()
                    .expect("I18n PrintExpression 必须携带 placeholder");
                values.insert(placeholder.to_owned(), text);
            }
            part_count += 1;
        }

        let resolved = match language {
            Some(language) => language.resolve(story.i18n(), &id, &values),
            None => story.i18n().resolve_default(&id, &values),
        }
        .expect("MIR I18n 身份和 placeholder 必须来自同一目录");
        if self.should_emit(MirOutputMode::Visible) {
            self.output
                .push(PresentationNode::Text(TextValue::from(resolved.text())));
        }
        for _part in 0..part_count {
            self.advance(passage.instructions().len())?;
        }
        Ok(MirStep::Running)
    }

    fn execute_instruction(
        &mut self,
        story: Option<&BytecodeProgram>,
        instruction_count: usize,
        encoded: &BytecodeInstruction,
        context: &mut dyn WritableEvaluationContext,
    ) -> Result<MirStep, MirExecutionError> {
        let expressions: Vec<crate::expression::Expression<'_>> = encoded
            .expressions()
            .iter()
            .map(crate::expression::OwnedExpression::as_expression)
            .collect();
        match encoded.operation() {
            BytecodeOperation::Text { text, output, .. } => {
                if self.should_emit(*output) && !text.trim().is_empty() {
                    self.output
                        .push(PresentationNode::Text(TextValue::from(text.as_str())));
                }
                self.advance(instruction_count)?;
            }
            BytecodeOperation::PrintLiteral { text, output, .. } => {
                if self.should_emit(*output) {
                    self.output
                        .push(PresentationNode::Text(TextValue::from(text.as_str())));
                }
                self.advance(instruction_count)?;
            }
            BytecodeOperation::PrintExpression { output, .. } => {
                let expression = &expressions[0];
                let value: Value = evaluate_with_mut(expression, context)
                    .map_err(MirExecutionError::Evaluation)?;
                let text: TextValue =
                    value_to_text(&value).ok_or(MirExecutionError::InvalidText(expression.span))?;
                if self.should_emit(*output) {
                    self.output.push(PresentationNode::Text(text));
                }
                self.advance(instruction_count)?;
            }
            BytecodeOperation::EvaluateDiscard => {
                let expression = &expressions[0];
                let _value: Value = evaluate_with_mut(expression, context)
                    .map_err(MirExecutionError::Evaluation)?;
                self.advance(instruction_count)?;
            }
            BytecodeOperation::Unset => {
                let target = &expressions[0];
                let _deleted: Option<Value> =
                    delete_with_mut(target, context).map_err(MirExecutionError::Evaluation)?;
                self.advance(instruction_count)?;
            }
            BytecodeOperation::Evaluate { destination } => {
                let expression = &expressions[0];
                let value: Value = evaluate_with_mut(expression, context)
                    .map_err(MirExecutionError::Evaluation)?;
                let slot: &mut Option<Value> = self
                    .current_mut()
                    .values
                    .get_mut(destination.index())
                    .ok_or(MirExecutionError::MissingValueSlot(*destination))?;
                *slot = Some(value);
                self.advance(instruction_count)?;
            }
            BytecodeOperation::JumpIfFalse { target } => {
                let condition = &expressions[0];
                let value: Value =
                    evaluate_with_mut(condition, context).map_err(MirExecutionError::Evaluation)?;
                if value.is_truthy() {
                    self.advance(instruction_count)?;
                } else {
                    self.current_mut().location = self.location().with_instruction(*target);
                }
            }
            BytecodeOperation::JumpIfNotStrictEqual { left, target } => {
                let right = &expressions[0];
                let selected: &Value = self
                    .current()
                    .values
                    .get(left.index())
                    .and_then(Option::as_ref)
                    .ok_or(MirExecutionError::MissingValueSlot(*left))?;
                let candidate: Value =
                    evaluate_with_mut(right, context).map_err(MirExecutionError::Evaluation)?;
                if values_strict_equal(selected, &candidate) {
                    self.advance(instruction_count)?;
                } else {
                    self.current_mut().location = self.location().with_instruction(*target);
                }
            }
            BytecodeOperation::Jump { target } => {
                self.current_mut().location = self.location().with_instruction(*target);
            }
            BytecodeOperation::PrepareCollectionIteration { kind, destination } => {
                let collection = &expressions[0];
                let collection_value: Value = evaluate_with_mut(collection, context)
                    .map_err(MirExecutionError::Evaluation)?;
                let keys: bool = matches!(kind, MirCollectionIterationKind::Keys);
                let values: Vec<Value> =
                    collection_iteration_values(collection_value, keys, collection.span)
                        .map_err(MirExecutionError::Evaluation)?;
                self.set_iterator(
                    *destination,
                    MirIteratorState::Collection { values, next: 0 },
                )?;
                self.advance(instruction_count)?;
            }
            BytecodeOperation::PrepareRangeIteration {
                has_step,
                destination,
            } => {
                let start = &expressions[0];
                let end = &expressions[1];
                let start_number: f64 = finite_range_number(
                    evaluate_with_mut(start, context).map_err(MirExecutionError::Evaluation)?,
                    start.span,
                )
                .map_err(MirExecutionError::Evaluation)?;
                let end_number: f64 = finite_range_number(
                    evaluate_with_mut(end, context).map_err(MirExecutionError::Evaluation)?,
                    end.span,
                )
                .map_err(MirExecutionError::Evaluation)?;
                let step_number: f64 = if *has_step {
                    let expression = &expressions[2];
                    finite_range_number(
                        evaluate_with_mut(expression, context)
                            .map_err(MirExecutionError::Evaluation)?,
                        expression.span,
                    )
                    .map_err(MirExecutionError::Evaluation)?
                } else if start_number <= end_number {
                    1.0
                } else {
                    -1.0
                };
                let step_span: Span = if *has_step {
                    expressions[2].span
                } else {
                    start.span
                };
                if step_number == 0.0
                    || (start_number < end_number && step_number < 0.0)
                    || (start_number > end_number && step_number > 0.0)
                {
                    return Err(MirExecutionError::Evaluation(EvalError::InvalidRange(
                        step_span,
                    )));
                }
                self.set_iterator(
                    *destination,
                    MirIteratorState::Range {
                        current: start_number,
                        end: end_number,
                        step: step_number,
                        step_span,
                        finished: false,
                    },
                )?;
                self.advance(instruction_count)?;
            }
            BytecodeOperation::NextIteration {
                iterator,
                exhausted,
            } => {
                let target = &expressions[0];
                let state: &mut MirIteratorState = self
                    .current_mut()
                    .iterators
                    .get_mut(iterator.index())
                    .and_then(Option::as_mut)
                    .ok_or(MirExecutionError::MissingIteratorSlot(*iterator))?;
                match state.next().map_err(MirExecutionError::Evaluation)? {
                    Some(value) => {
                        assign_value_with_mut(target, value, context)
                            .map_err(MirExecutionError::Evaluation)?;
                        self.advance(instruction_count)?;
                    }
                    None => {
                        self.current_mut().location = self.location().with_instruction(*exhausted);
                    }
                }
            }
            BytecodeOperation::RequestInclude { output } => {
                let target = &expressions[0];
                let story: &BytecodeProgram =
                    story.ok_or(MirExecutionError::MacroBodyIncludeUnsupported)?;
                let name: String = evaluate_passage_name(target, context)?;
                let included: &BytecodePassage = story
                    .passage(&name)
                    .ok_or(MirExecutionError::MissingPassage)?;
                let output_suppressed: bool =
                    self.current().output_suppressed || *output == MirOutputMode::Suppressed;
                self.advance(instruction_count)?;
                self.stack
                    .push(MirPassageFrame::new(included, output_suppressed));
                self.includes_entered = self
                    .includes_entered
                    .checked_add(1)
                    .expect("单条执行链的 include 数量不可能超过地址空间");
            }
            BytecodeOperation::RequestGoto => {
                let target = &expressions[0];
                self.navigation = Some(evaluate_passage_name(target, context)?);
                return Ok(MirStep::NavigationPending);
            }
            BytecodeOperation::InvokeMacro { .. } => return Ok(MirStep::MacroPending),
            BytecodeOperation::ExitPassage | BytecodeOperation::Halt => {
                return Ok(self.finish_current());
            }
        }
        Ok(MirStep::Running)
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

//! Bytecode 单步执行、表达式求值与 I18n 输出解析。
//!
//! Frame 生命周期、include 栈和暂停恢复保留在父模块；本模块只解释当前指令并
//! 产生下一步控制信号。这样执行语义不会与 continuation 所有权混在同一大型 impl。

use super::*;

impl MirExecutionFrame {
    /// 以可选运行语言执行一条指令；语言不属于当前程序目录时返回错误。
    pub(super) fn step_with_language(
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
        self.consume_instructions(1)?;
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
        let mut dictionary_values: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
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
                let dictionary_eligible: bool = matches!(&value, Value::String(_));
                let text: TextValue =
                    value_to_text(&value).ok_or(MirExecutionError::InvalidText(expression.span))?;
                let text: String = text
                    .to_unicode_string()
                    .ok_or(MirExecutionError::InvalidText(expression.span))?;
                let placeholder: &str = identity
                    .placeholder()
                    .expect("I18n PrintExpression 必须携带 placeholder");
                if dictionary_eligible {
                    dictionary_values.insert(placeholder.to_owned());
                }
                values.insert(placeholder.to_owned(), text);
            }
            part_count += 1;
        }
        // 入口已经消费第一条；同一 I18n 消息折叠执行的其余指令也必须计入预算。
        self.consume_instructions(part_count.saturating_sub(1))?;

        let resolved = match language {
            Some(language) => language.resolve(story.i18n(), &id, &values, &dictionary_values),
            None => story.i18n().resolve_default(&id, &values),
        }
        .expect("MIR I18n 身份和 placeholder 必须来自同一目录");
        if self.should_emit(MirOutputMode::Visible) {
            self.output
                .push(SemanticNode::Text(TextValue::from(resolved.text())));
        }
        for _part in 0..part_count {
            self.advance(passage.instructions().len())?;
        }
        Ok(MirStep::Running)
    }

    /// 解释单条 Bytecode 指令并推进帧；`story` 为 None 时禁止 include 请求。
    pub(super) fn execute_instruction(
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
                        .push(SemanticNode::Text(fold_source_line_whitespace(text)));
                }
                self.advance(instruction_count)?;
            }
            BytecodeOperation::HardBreak { output } => {
                if self.should_emit(*output) {
                    self.output.push(SemanticNode::HardBreak);
                }
                self.advance(instruction_count)?;
            }
            BytecodeOperation::PrintLiteral { text, output, .. } => {
                if self.should_emit(*output) {
                    self.output
                        .push(SemanticNode::Text(TextValue::from(text.as_str())));
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
                    self.output.push(SemanticNode::Text(text));
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
}

/// Twee 自然换行是源码排版空白；显式 HardBreak 已在 HIR 中成为独立节点。
fn fold_source_line_whitespace(text: &str) -> TextValue {
    let mut output: String = String::with_capacity(text.len());
    let mut pending_space: bool = false;
    for character in text.chars() {
        if matches!(character, '\r' | '\n') {
            while output.ends_with([' ', '\t']) {
                output.pop();
            }
            pending_space = !output.is_empty();
        } else if pending_space && matches!(character, ' ' | '\t') {
            continue;
        } else {
            if pending_space {
                output.push(' ');
                pending_space = false;
            }
            output.push(character);
        }
    }
    TextValue::from(output)
}

//! Passage include、结构化控制流、Widget 与动态 Macro 的执行细节。
//!
//! 所有分支最终回到 `execute_body` 或统一 Macro 分派，避免容器节点另建一套输出、
//! 局部作用域或控制信号规则。

use super::*;

impl<'runtime, 'hir, 'source, Story, Native>
    RuntimeExecutionContext<'runtime, 'hir, 'source, Story, Native>
where
    Story: RuntimeStoryAccess<'hir, 'source> + ?Sized,
{
    /// 执行已解析的动态片段并产生有序输出（公开 `Macro.execute()` 入口）。
    pub fn execute_parsed_fragment<'fragment>(
        &mut self,
        fragment: &crate::macro_runtime::ParsedFragment<'fragment>,
    ) -> Result<BodyExecution, RuntimeExecutionError<Story::Error>> {
        self.execute_fragment(fragment.nodes())
    }

    /// include 使用的内部边界：继续向当前执行链累积输出，不取走。
    pub(super) fn execute_passage_body(
        &mut self,
        passage: &HirPassage<'source>,
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        match self.execute_body(&passage.body)? {
            BodyControl::ExitScope => Ok(BodyControl::Continue),
            control => Ok(control),
        }
    }

    /// 在当前节点位置执行 include，并限制本次调用可展开的目标数量。
    pub fn execute_passage_with_includes(
        &mut self,
        passage: &HirPassage<'source>,
        include_limit: usize,
    ) -> Result<BodyExecution, RuntimeExecutionError<Story::Error>> {
        let previous_limit: Option<usize> = self.include_limit.replace(include_limit);
        let previous_count: usize = std::mem::replace(&mut self.included_passages, 0);
        let result: Result<BodyExecution, RuntimeExecutionError<Story::Error>> =
            self.execute_passage(passage);
        self.include_limit = previous_limit;
        self.included_passages = previous_count;
        result
    }

    /// include 使用同一 Runtime Context，因此 State、Macro Local 与控制信号保持连续。
    pub(super) fn execute_pending_includes(
        &mut self,
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        let Some(limit): Option<usize> = self.include_limit else {
            return Ok(BodyControl::Continue);
        };

        while let Some(request) = self.story.take_include_request() {
            if self.included_passages == limit {
                return Err(RuntimeExecutionError::IncludeLimitExceeded { limit });
            }
            self.included_passages += 1;
            let request: StoryIncludeRequest<'hir, 'source> = request;
            let control: BodyControl = self.execute_passage_body(request.passage())?;
            if !matches!(control, BodyControl::Continue) {
                return Ok(control);
            }
        }
        Ok(BodyControl::Continue)
    }

    /// 精确查询 PassageName，并把选中的 HIR Passage 交给现有执行边界。
    pub fn execute_story_passage(
        &mut self,
        story: &HirStory<'source>,
        name: &str,
    ) -> Result<BodyExecution, RuntimeExecutionError<Story::Error>> {
        let passage: &HirPassage<'source> = story
            .passage(name)
            .ok_or_else(|| RuntimeExecutionError::MissingPassage(name.to_owned()))?;
        self.execute_passage(passage)
    }

    /// 条件按 Narrava truthiness 短路，选中正文回到统一 Runtime 分派。
    pub(super) fn execute_if<'node>(
        &mut self,
        conditional: &HirIf<'node>,
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        for branch in &conditional.branches {
            let condition: Value = self.evaluate(&branch.condition)?;
            if condition.is_truthy() {
                return self.execute_body(&branch.body);
            }
        }

        match &conditional.fallback {
            Some(body) => self.execute_body(body),
            None => Ok(BodyControl::Continue),
        }
    }

    /// 每轮重新求值条件，并只在当前 while 边界消费 break 与 continue。
    pub(super) fn execute_while<'node>(
        &mut self,
        loop_node: &HirWhile<'node>,
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        loop {
            self.consume_execution_step()?;
            let condition: Value = self.evaluate(&loop_node.condition)?;
            if !condition.is_truthy() {
                return Ok(BodyControl::Continue);
            }

            match self.execute_body(&loop_node.body)? {
                BodyControl::Continue | BodyControl::ContinueLoop => continue,
                BodyControl::BreakLoop => return Ok(BodyControl::Continue),
                BodyControl::ExitScope => return Ok(BodyControl::ExitScope),
                BodyControl::StopPassage => return Ok(BodyControl::StopPassage),
            }
        }
    }

    /// 根据 HIR 中已经确定的形式执行集合或范围循环。
    pub(super) fn execute_for<'node>(
        &mut self,
        loop_node: &HirFor<'node>,
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        let (collection, keys): (&crate::expression::Expression<'node>, bool) =
            match &loop_node.kind {
                HirForKind::In { collection, .. } => (collection, true),
                HirForKind::Of { collection, .. } => (collection, false),
                HirForKind::Range {
                    start, end, step, ..
                } => return self.execute_for_range(loop_node, start, end, step.as_ref()),
            };
        let span: crate::expression::Span = collection.span;
        let collection_value: Value = self.evaluate(collection)?;
        let values: Vec<Value> = collection_iteration_values(collection_value, keys, span)
            .map_err(LogicNodeError::Evaluation)
            .map_err(RuntimeExecutionError::Logic)?;

        for value in values {
            self.consume_execution_step()?;
            self.assign(&loop_node.target.value, value)?;
            match self.execute_body(&loop_node.body)? {
                BodyControl::Continue | BodyControl::ContinueLoop => continue,
                BodyControl::BreakLoop => return Ok(BodyControl::Continue),
                BodyControl::ExitScope => return Ok(BodyControl::ExitScope),
                BodyControl::StopPassage => return Ok(BodyControl::StopPassage),
            }
        }
        Ok(BodyControl::Continue)
    }

    /// 范围包含终点；边界与步长在进入正文前各求值一次。
    fn execute_for_range<'node>(
        &mut self,
        loop_node: &HirFor<'node>,
        start: &crate::expression::Expression<'node>,
        end: &crate::expression::Expression<'node>,
        step: Option<&crate::expression::Expression<'node>>,
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        let start_number: f64 = finite_range_number(self.evaluate(start)?, start.span)
            .map_err(LogicNodeError::Evaluation)
            .map_err(RuntimeExecutionError::Logic)?;
        let end_number: f64 = finite_range_number(self.evaluate(end)?, end.span)
            .map_err(LogicNodeError::Evaluation)
            .map_err(RuntimeExecutionError::Logic)?;
        let step_number: f64 = match step {
            Some(expression) => finite_range_number(self.evaluate(expression)?, expression.span)
                .map_err(LogicNodeError::Evaluation)
                .map_err(RuntimeExecutionError::Logic)?,
            None if start_number <= end_number => 1.0,
            None => -1.0,
        };
        let step_span: crate::expression::Span = step.map_or(start.span, |value| value.span);
        if step_number == 0.0
            || (start_number < end_number && step_number < 0.0)
            || (start_number > end_number && step_number > 0.0)
        {
            return Err(RuntimeExecutionError::Logic(LogicNodeError::Evaluation(
                crate::expression::evaluator::EvalError::InvalidRange(step_span),
            )));
        }

        let ascending: bool = step_number > 0.0;
        let mut current: f64 = start_number;
        while if ascending {
            current <= end_number
        } else {
            current >= end_number
        } {
            self.consume_execution_step()?;
            self.assign(&loop_node.target.value, Value::Number(current))?;
            match self.execute_body(&loop_node.body)? {
                BodyControl::Continue | BodyControl::ContinueLoop => {}
                BodyControl::BreakLoop => return Ok(BodyControl::Continue),
                BodyControl::ExitScope => return Ok(BodyControl::ExitScope),
                BodyControl::StopPassage => return Ok(BodyControl::StopPassage),
            }
            if current == end_number {
                return Ok(BodyControl::Continue);
            }
            let next: f64 = current + step_number;
            if next == current {
                return Err(RuntimeExecutionError::Logic(LogicNodeError::Evaluation(
                    crate::expression::evaluator::EvalError::InvalidRange(step_span),
                )));
            }
            current = next;
        }
        Ok(BodyControl::Continue)
    }

    /// 主值只求值一次，首个严格匹配的 case 回到统一 Runtime 分派。
    pub(super) fn execute_switch<'node>(
        &mut self,
        switch: &HirSwitch<'node>,
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        let selected: Value = self.evaluate(&switch.value)?;
        for case in &switch.cases {
            let candidate: Value = self.evaluate(&case.value)?;
            if values_strict_equal(&selected, &candidate) {
                return self.execute_body(&case.body);
            }
        }

        match &switch.default {
            Some(body) => self.execute_body(body),
            None => Ok(BodyControl::Continue),
        }
    }

    /// Expression 求值只临时组合逻辑 Context，不扩大其所有权。
    pub(super) fn evaluate<'node>(
        &mut self,
        expression: &crate::expression::Expression<'node>,
    ) -> Result<Value, RuntimeExecutionError<Story::Error>> {
        let mut context: MacroLogicContext<'_, Story> =
            MacroLogicContext::new(self.state, self.story, self.locals);
        evaluate_with_mut(expression, &mut context)
            .map_err(LogicNodeError::Evaluation)
            .map_err(RuntimeExecutionError::Logic)
    }

    /// 将已经求得的循环值写入 HIR 目标，目标中的动态索引仍只求值一次。
    fn assign<'node>(
        &mut self,
        target: &crate::expression::Expression<'node>,
        value: Value,
    ) -> Result<(), RuntimeExecutionError<Story::Error>> {
        let mut context: MacroLogicContext<'_, Story> =
            MacroLogicContext::new(self.state, self.story, self.locals);
        assign_value_with_mut(target, value, &mut context)
            .map_err(LogicNodeError::Evaluation)
            .map_err(RuntimeExecutionError::Logic)
    }

    /// Widget 使用当前 Definitions 递归执行，并只消费属于本次调用的 exit。
    fn execute_widget<'call>(
        &mut self,
        call: &crate::hir::HirMacro<'call>,
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        let prepared: PreparedWidget<'hir, 'source> =
            prepare_widget_macro(call, self.definitions, self.state, self.story, self.locals)
                .map_err(RuntimeExecutionError::Widget)?;

        // Widget 先写入自己的缓冲区。只有完整成功后才合并，后续 after 钩子也只会
        // 接触这一份输出；失败时不能把半段文本遗留在 Passage 输出中。
        let outer_output: SemanticOutput = std::mem::take(&mut self.output);
        self.locals.enter_call(prepared.arguments);

        if let Some(callbacks) = self.macro_lifecycle.as_deref_mut() {
            let arguments: &mut [Value] = self
                .locals
                .args_mut()
                .expect("Widget 生命周期执行前必须存在调用帧");
            if let Err(diagnostic) = callbacks.before(call.name, arguments) {
                let _left: bool = self.locals.leave();
                self.output = outer_output;
                return Err(RuntimeExecutionError::MacroLifecycle(diagnostic));
            }
        }

        let result: Result<BodyControl, RuntimeExecutionError<Story::Error>> =
            self.execute_body(prepared.body);
        let mut widget_output: SemanticOutput = std::mem::take(&mut self.output);
        self.output = outer_output;

        match result {
            Ok(control) => {
                if let Some(callbacks) = self.macro_lifecycle.as_deref_mut() {
                    let arguments: &[Value] = self
                        .locals
                        .args()
                        .expect("Widget after 执行前必须存在调用帧");
                    widget_output = match callbacks.after(call.name, arguments, widget_output) {
                        Ok(output) => output,
                        Err(diagnostic) => {
                            let _left: bool = self.locals.leave();
                            return Err(RuntimeExecutionError::MacroLifecycle(diagnostic));
                        }
                    };
                }
                let _left: bool = self.locals.leave();
                self.output.append(widget_output);
                match control {
                    BodyControl::ExitScope => Ok(BodyControl::Continue),
                    control => Ok(control),
                }
            }
            Err(error) => {
                let _left: bool = self.locals.leave();
                Err(error)
            }
        }
    }

    /// 根据共享 Definition 中的 Handler 类型选择 Widget 或 Native 分派。
    pub(super) fn execute_dynamic_macro<'call>(
        &mut self,
        call: &crate::hir::HirMacro<'call>,
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        let definition: &MacroDefinition<RuntimeMacroHandler<'hir, 'source, Native>> = self
            .definitions
            .get(call.name)
            .ok_or(RuntimeExecutionError::MacroDefinition(
                MacroDefinitionError::MissingDefinition,
            ))?;
        match definition.handler {
            RuntimeMacroHandler::Widget(_) => self.execute_widget(call),
            RuntimeMacroHandler::Native(_) => self.execute_native_macro(call),
        }
    }

    /// 执行一个当前不会暂停的 Native／scripts Macro，并隔离其语义输出。
    fn execute_native_macro<'call>(
        &mut self,
        call: &crate::hir::HirMacro<'call>,
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        let raw_arguments: &str = match call.arguments {
            crate::hir::HirMacroArguments::None => "",
            crate::hir::HirMacroArguments::Raw(raw) => raw,
            crate::hir::HirMacroArguments::Expression(_) => {
                return Err(RuntimeExecutionError::NativeMacro(
                    NativeMacroError::InvalidHirArguments,
                ));
            }
        };
        let definition: &MacroDefinition<RuntimeMacroHandler<'hir, 'source, Native>> = self
            .definitions
            .get(call.name)
            .expect("动态 Macro 分派已经确认 Definition 存在");
        let expected: MacroBodyKind = definition.body_kind;
        let actual: MacroBodyKind = match call.syntax_kind {
            crate::twee::MacroSyntaxKind::Inline => MacroBodyKind::Inline,
            crate::twee::MacroSyntaxKind::Container => MacroBodyKind::Container,
        };
        if expected != actual {
            return Err(RuntimeExecutionError::NativeMacro(
                NativeMacroError::BodyKindMismatch { expected, actual },
            ));
        }
        if definition.execution_kind != MacroExecutionKind::Sync {
            return Err(RuntimeExecutionError::NativeMacro(
                NativeMacroError::AsyncUnsupported,
            ));
        }

        let arguments: Vec<Value> = match definition.argument_kind {
            crate::macro_runtime::MacroArgumentKind::Raw => Vec::new(),
            crate::macro_runtime::MacroArgumentKind::ArgumentList => {
                let parsed = parse_argument_list(raw_arguments)
                    .map_err(NativeMacroError::ArgumentList)
                    .map_err(RuntimeExecutionError::NativeMacro)?;
                let mut context: MacroLogicContext<'_, Story> =
                    MacroLogicContext::new(self.state, self.story, self.locals);
                prepare_argument_values(&parsed, |expression| {
                    evaluate_with_mut(expression, &mut context)
                })
                .map_err(NativeMacroError::ArgumentValue)
                .map_err(RuntimeExecutionError::NativeMacro)?
            }
        };

        let outer_output: SemanticOutput = std::mem::take(&mut self.output);
        let capture_names: Vec<&str> = self.capture_names.iter().map(String::as_str).collect();
        let captures: CapturedMacroLocals<Value> = self.locals.capture(&capture_names);
        self.locals.enter_call(arguments);
        if let Some(callbacks) = self.macro_lifecycle.as_deref_mut() {
            let active: &mut [Value] = self
                .locals
                .args_mut()
                .expect("Native Macro before 前必须存在调用帧");
            if let Err(diagnostic) = callbacks.before(call.name, active) {
                let _left: bool = self.locals.leave();
                self.output = outer_output;
                return Err(RuntimeExecutionError::MacroLifecycle(diagnostic));
            }
        }

        let body: MacroInvocationBody<'_, HirBodyNode<'call>> = match call.syntax_kind {
            crate::twee::MacroSyntaxKind::Inline => MacroInvocationBody::Inline,
            crate::twee::MacroSyntaxKind::Container => {
                MacroInvocationBody::Container(call.body.as_slice())
            }
        };
        let execution: Result<BodyExecution, Diagnostic> = match self.native_macros.as_deref_mut() {
            Some(callbacks) => {
                let handler: &Native = match &definition.handler {
                    RuntimeMacroHandler::Native(handler) => handler,
                    RuntimeMacroHandler::Widget(_) => {
                        unreachable!("Native 分派必须对应 Native Handler")
                    }
                };
                let arguments: Vec<Value> = self
                    .locals
                    .args()
                    .expect("Native Macro Handler 前必须存在调用帧")
                    .to_vec();
                let mut context: MacroLogicContext<'_, Story> =
                    MacroLogicContext::new(self.state, self.story, self.locals);
                callbacks.invoke(
                    handler,
                    MacroInvocation {
                        name: call.name,
                        raw_arguments,
                        arguments: &arguments,
                        body,
                        captures,
                        context: &mut context,
                    },
                )
            }
            None => {
                let _left: bool = self.locals.leave();
                self.output = outer_output;
                return Err(RuntimeExecutionError::NativeMacro(
                    NativeMacroError::MissingCallbacks,
                ));
            }
        };
        let mut execution: BodyExecution = match execution {
            Ok(execution) => execution,
            Err(diagnostic) => {
                let _left: bool = self.locals.leave();
                self.output = outer_output;
                return Err(RuntimeExecutionError::NativeMacro(
                    NativeMacroError::Handler(diagnostic),
                ));
            }
        };
        if let Some(callbacks) = self.macro_lifecycle.as_deref_mut() {
            let arguments: &[Value] = self
                .locals
                .args()
                .expect("Native Macro after 前必须存在调用帧");
            execution.output = match callbacks.after(call.name, arguments, execution.output) {
                Ok(output) => output,
                Err(diagnostic) => {
                    let _left: bool = self.locals.leave();
                    self.output = outer_output;
                    return Err(RuntimeExecutionError::MacroLifecycle(diagnostic));
                }
            };
        }
        let _left: bool = self.locals.leave();
        self.output = outer_output;
        self.output.append(execution.output);
        match execution.control {
            BodyControl::ExitScope => Ok(BodyControl::Continue),
            control => Ok(control),
        }
    }
}

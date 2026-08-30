//! 组合 Runtime 执行所需的借用，不改变各领域数据的所有者。

use crate::{
    diagnostic::Diagnostic,
    expression::{
        evaluator::{
            WritableEvaluationContext, assign_value_with_mut, evaluate_with_mut, value_to_text,
            values_strict_equal,
        },
        value::{TextValue, Value},
    },
    hir::{
        HirBodyKind, HirBodyNode, HirFor, HirForKind, HirIf, HirPassage, HirPrint, HirStory,
        HirSwitch, HirWhile,
    },
    macro_runtime::{
        CapturedMacroLocals, MacroArgumentListError, MacroArgumentValueError, MacroBodyKind,
        MacroCallOutcome, MacroDefinition, MacroDefinitionError, MacroDefinitions,
        MacroExecutionKind, MacroHandlerOutcome, MacroInvocation, MacroInvocationBody,
        MacroLifecycleCallbacks, MacroLocalScopes, MacroLogicContext, MacroSuspension,
        RuntimeMacroHandler, parse_argument_list, prepare_argument_values,
    },
    semantic::{SemanticNode, SemanticOutput},
    story::{RuntimeStoryAccess, StoryIncludeRequest},
};

use super::{
    AsyncNativeMacroCallbacks, BodyControl, BodyExecution, LogicNodeError, NativeMacroCallbacks,
    PreparedWidget, RuntimeMacroExecution, RuntimeNativePending, WidgetMacroError,
    execute_logic_node,
    logic::{collection_iteration_values, finite_range_number},
    prepare_widget_macro,
};

/// 上层节点分派保留逻辑节点与动态 Macro 各自的错误类型。
#[derive(Debug, PartialEq)]
pub enum RuntimeExecutionError<StoryError> {
    /// 同步 HIR 正文消耗的节点/循环步骤超过预算。
    ExecutionLimitExceeded { limit: usize },
    /// HIR 逻辑节点（求值、Story 请求等）失败。
    Logic(LogicNodeError<StoryError>),
    /// Widget 调用或正文执行失败。
    Widget(WidgetMacroError<StoryError>),
    /// Macro Definition 查询失败。
    MacroDefinition(MacroDefinitionError),
    /// before／after 生命周期回调返回失败 Diagnostic。
    MacroLifecycle(Diagnostic),
    /// Native／scripts Macro 调用边界失败。
    NativeMacro(NativeMacroError),
    /// 按名称执行的 Passage 不存在。
    MissingPassage(String),
    /// 本条执行链展开的 include 超过预算。
    IncludeLimitExceeded { limit: usize },
}

/// Native／scripts Macro 在进入 Handler 前后能够稳定区分的错误。
#[derive(Debug, PartialEq)]
pub enum NativeMacroError {
    /// 参数列表在边界解析阶段失败。
    ArgumentList(MacroArgumentListError),
    /// 参数 Expression 求值失败。
    ArgumentValue(MacroArgumentValueError<crate::expression::evaluator::EvalError>),
    /// 调用处使用了当前 Native 边界不支持的 Expression 参数形式。
    InvalidHirArguments,
    /// 调用处正文形态与注册定义不一致。
    BodyKindMismatch {
        expected: MacroBodyKind,
        actual: MacroBodyKind,
    },
    /// 同步入口遇到了 Async Definition。
    AsyncUnsupported,
    /// 异步入口遇到了非 Async Definition。
    ExpectedAsync,
    /// 当前执行上下文没有安装 Native Macro 调用适配器。
    MissingCallbacks,
    /// Handler 自身报告的业务或 Runtime 错误。
    Handler(Diagnostic),
}

/// Runtime 一次执行链所需的借用集合。
///
/// Definitions、State、Story 与 Macro Local 仍由各自模块持有；这里仅在执行期间组合它们。
pub struct RuntimeExecutionContext<'runtime, 'hir, 'source, Story: ?Sized, Native> {
    definitions:
        &'runtime MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'hir, 'source, Native>>>,
    state: &'runtime mut dyn WritableEvaluationContext,
    story: &'runtime mut Story,
    locals: &'runtime mut MacroLocalScopes<Value>,
    macro_lifecycle: Option<&'runtime mut dyn MacroLifecycleCallbacks>,
    native_macros: Option<&'runtime mut dyn NativeMacroCallbacks<Native, Story>>,
    include_limit: Option<usize>,
    included_passages: usize,
    execution_limit: usize,
    executed_steps: usize,
    capture_names: Vec<&'source str>,
    /// 当前执行链按源码顺序累积的有序输出；公共入口结束时取走。
    output: SemanticOutput,
}

impl<'runtime, 'hir, 'source, Story, Native>
    RuntimeExecutionContext<'runtime, 'hir, 'source, Story, Native>
where
    Story: RuntimeStoryAccess<'hir, 'source> + ?Sized,
{
    /// 组合现有所有者的借用，不移动或复制运行时状态。
    pub fn new(
        definitions: &'runtime MacroDefinitions<
            MacroDefinition<RuntimeMacroHandler<'hir, 'source, Native>>,
        >,
        state: &'runtime mut dyn WritableEvaluationContext,
        story: &'runtime mut Story,
        locals: &'runtime mut MacroLocalScopes<Value>,
    ) -> Self {
        Self {
            definitions,
            state,
            story,
            locals,
            macro_lifecycle: None,
            native_macros: None,
            include_limit: None,
            included_passages: 0,
            execution_limit: 1_000_000,
            executed_steps: 0,
            capture_names: Vec::new(),
            output: SemanticOutput::default(),
        }
    }

    /// 接入当前 Binding 的 Native／scripts Macro 调用适配器。
    pub fn with_native_macros(
        mut self,
        callbacks: &'runtime mut dyn NativeMacroCallbacks<Native, Story>,
    ) -> Self {
        self.native_macros = Some(callbacks);
        self
    }

    /// 接入 Macro 控制器提供的生命周期回调；未接入时调用保持原行为。
    pub fn with_macro_lifecycle(
        mut self,
        callbacks: &'runtime mut dyn MacroLifecycleCallbacks,
    ) -> Self {
        self.macro_lifecycle = Some(callbacks);
        self
    }

    /// 分派一个 HIR 节点；通用 Macro 查询共享 Definitions，其余节点沿用逻辑分派。
    pub fn execute_node(
        &mut self,
        node: &HirBodyNode<'source>,
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        self.consume_execution_step()?;
        match &node.kind {
            HirBodyKind::Text(text) => {
                // 静态正文直接进入 SemanticOutput，不进行二次语法解释。
                if !text.trim().is_empty() {
                    self.output.push(SemanticNode::Text(TextValue::from(*text)));
                }
                Ok(BodyControl::Continue)
            }
            HirBodyKind::Print(print) => {
                let text: TextValue = match print {
                    HirPrint::Literal(text) => TextValue::from(*text),
                    HirPrint::Expression(expression) => {
                        let value: Value = self.evaluate(expression)?;
                        value_to_text(&value).ok_or(RuntimeExecutionError::Logic(
                            LogicNodeError::InvalidText(expression.span),
                        ))?
                    }
                };
                self.output.push(SemanticNode::Text(text));
                Ok(BodyControl::Continue)
            }
            HirBodyKind::Silently(body) => self.execute_silently(body),
            HirBodyKind::Capture(capture) => {
                let previous_len: usize = self.capture_names.len();
                self.capture_names.extend(capture.locals.iter().copied());
                let result: Result<BodyControl, RuntimeExecutionError<Story::Error>> =
                    self.execute_body(&capture.body);
                self.capture_names.truncate(previous_len);
                result
            }
            HirBodyKind::Macro(call) => self.execute_dynamic_macro(call),
            HirBodyKind::If(conditional) => self.execute_if(conditional),
            HirBodyKind::Switch(switch) => self.execute_switch(switch),
            HirBodyKind::While(loop_node) => self.execute_while(loop_node),
            HirBodyKind::For(loop_node) => self.execute_for(loop_node),
            _ => {
                let mut context: MacroLogicContext<'_, Story> =
                    MacroLogicContext::new(self.state, self.story, self.locals);
                execute_logic_node(node, &mut context).map_err(RuntimeExecutionError::Logic)
            }
        }
    }

    fn consume_execution_step(&mut self) -> Result<(), RuntimeExecutionError<Story::Error>> {
        if self.executed_steps >= self.execution_limit {
            return Err(RuntimeExecutionError::ExecutionLimitExceeded {
                limit: self.execution_limit,
            });
        }
        self.executed_steps += 1;
        Ok(())
    }

    /// 按顺序执行正文，并在首个非 Continue 控制信号处停止。
    pub fn execute_body(
        &mut self,
        body: &[HirBodyNode<'source>],
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        for node in body {
            let control: BodyControl = self.execute_node(node)?;
            if !matches!(control, BodyControl::Continue) {
                return Ok(control);
            }
            let include_control: BodyControl = self.execute_pending_includes()?;
            if !matches!(include_control, BodyControl::Continue) {
                return Ok(include_control);
            }
        }
        Ok(BodyControl::Continue)
    }

    /// 执行正文中的逻辑与控制信号，但不把该块输出合并到外层。
    fn execute_silently(
        &mut self,
        body: &[HirBodyNode<'source>],
    ) -> Result<BodyControl, RuntimeExecutionError<Story::Error>> {
        let outer_output: SemanticOutput = std::mem::take(&mut self.output);
        let result: Result<BodyControl, RuntimeExecutionError<Story::Error>> =
            self.execute_body(body);
        let _discarded_output: SemanticOutput = std::mem::take(&mut self.output);
        self.output = outer_output;
        result
    }

    /// 执行一个 Passage 正文，并在该边界消费未被 Widget 消费的 `exit`。
    ///
    /// `goto` 产生的 `StopPassage` 会原样返回，交给 Story 导航流程处理。
    /// 这是公共入口：执行结束后取走本次累积的有序输出。
    pub fn execute_passage(
        &mut self,
        passage: &HirPassage<'source>,
    ) -> Result<BodyExecution, RuntimeExecutionError<Story::Error>> {
        let control: BodyControl = self.execute_passage_body(passage)?;
        Ok(BodyExecution {
            control,
            output: std::mem::take(&mut self.output),
        })
    }

    /// 执行任意 HIR 正文节点列表（动态 Twee 片段），并取走本次累积输出。
    ///
    /// 与 [`Self::execute_passage`] 共用同一执行与输出链；片段没有
    /// Passage 边界，因此不消费 `exit` 信号。
    pub fn execute_fragment(
        &mut self,
        body: &[HirBodyNode<'source>],
    ) -> Result<BodyExecution, RuntimeExecutionError<Story::Error>> {
        let control: BodyControl = self.execute_body(body)?;
        Ok(BodyExecution {
            control,
            output: std::mem::take(&mut self.output),
        })
    }

    /// 执行 VM 暂停位置提供的单个动态 Macro，并返回独立的控制信号与输出。
    ///
    /// 调用仍复用统一的 Definition、参数帧和生命周期分派。这里仅隔离输出，
    /// 使控制器可以在成功后把结果明确交回 VM，而不会取走外层执行链的内容。
    pub fn execute_macro<'call>(
        &mut self,
        call: &crate::hir::HirMacro<'call>,
    ) -> Result<BodyExecution, RuntimeExecutionError<Story::Error>> {
        let outer_output: SemanticOutput = std::mem::take(&mut self.output);
        let result: Result<BodyControl, RuntimeExecutionError<Story::Error>> =
            self.execute_dynamic_macro(call);
        let macro_output: SemanticOutput = std::mem::take(&mut self.output);
        self.output = outer_output;

        result.map(|control| BodyExecution {
            control,
            output: macro_output,
        })
    }

    /// 在给定预算内执行单个 Macro，并报告其内部实际展开的 Passage 数。
    pub fn execute_macro_with_includes<'call>(
        &mut self,
        call: &crate::hir::HirMacro<'call>,
        include_limit: usize,
    ) -> Result<RuntimeMacroExecution, RuntimeExecutionError<Story::Error>> {
        let previous_limit: Option<usize> = self.include_limit.replace(include_limit);
        let previous_count: usize = std::mem::replace(&mut self.included_passages, 0);
        let result: Result<BodyExecution, RuntimeExecutionError<Story::Error>> =
            self.execute_macro(call);
        let includes_entered: usize = self.included_passages;
        self.include_limit = previous_limit;
        self.included_passages = previous_count;

        result.map(|execution| RuntimeMacroExecution {
            execution,
            includes_entered,
        })
    }

    /// 首次执行一个 Async Native／scripts Macro，并保留可恢复调用帧。
    pub fn execute_async_native_macro<'call, Pending>(
        &mut self,
        call: &crate::hir::HirMacro<'call>,
        identity: crate::runtime::RuntimeExecutionIdentity,
        callbacks: &mut dyn AsyncNativeMacroCallbacks<Native, Story, Pending>,
    ) -> Result<
        MacroCallOutcome<RuntimeMacroExecution, RuntimeNativePending<Pending>>,
        RuntimeExecutionError<Story::Error>,
    > {
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
            .ok_or(RuntimeExecutionError::MacroDefinition(
                MacroDefinitionError::MissingDefinition,
            ))?;
        let actual: MacroBodyKind = match call.syntax_kind {
            crate::twee::MacroSyntaxKind::Inline => MacroBodyKind::Inline,
            crate::twee::MacroSyntaxKind::Container => MacroBodyKind::Container,
        };
        if definition.body_kind != actual {
            return Err(RuntimeExecutionError::NativeMacro(
                NativeMacroError::BodyKindMismatch {
                    expected: definition.body_kind,
                    actual,
                },
            ));
        }
        if definition.execution_kind != MacroExecutionKind::Async {
            return Err(RuntimeExecutionError::NativeMacro(
                NativeMacroError::ExpectedAsync,
            ));
        }
        let handler: &Native = match &definition.handler {
            RuntimeMacroHandler::Native(handler) => handler,
            RuntimeMacroHandler::Widget(_) => {
                return Err(RuntimeExecutionError::NativeMacro(
                    NativeMacroError::ExpectedAsync,
                ));
            }
        };
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

        let captures: CapturedMacroLocals<Value> = self.locals.capture(&self.capture_names);
        self.locals.enter_call(arguments);
        if let Some(lifecycle) = self.macro_lifecycle.as_deref_mut() {
            let active: &mut [Value] = self
                .locals
                .args_mut()
                .expect("Async Native before 前必须存在调用帧");
            if let Err(diagnostic) = lifecycle.before(call.name, active) {
                let _left: bool = self.locals.leave();
                return Err(RuntimeExecutionError::MacroLifecycle(diagnostic));
            }
        }
        let body: MacroInvocationBody<'_, HirBodyNode<'call>> = match call.syntax_kind {
            crate::twee::MacroSyntaxKind::Inline => MacroInvocationBody::Inline,
            crate::twee::MacroSyntaxKind::Container => {
                MacroInvocationBody::Container(call.body.as_slice())
            }
        };
        let handler_arguments: Vec<Value> = self
            .locals
            .args()
            .expect("Async Native Handler 前必须存在调用帧")
            .to_vec();
        let outcome: Result<MacroHandlerOutcome<BodyExecution, Pending>, Diagnostic> = {
            let mut context: MacroLogicContext<'_, Story> =
                MacroLogicContext::new(self.state, self.story, self.locals);
            callbacks.invoke(
                handler,
                MacroInvocation {
                    name: call.name,
                    raw_arguments,
                    arguments: &handler_arguments,
                    body,
                    captures,
                    context: &mut context,
                },
            )
        };
        match outcome {
            Err(diagnostic) => {
                let _left: bool = self.locals.leave();
                Err(RuntimeExecutionError::NativeMacro(
                    NativeMacroError::Handler(diagnostic),
                ))
            }
            Ok(MacroHandlerOutcome::Pending(handle)) => {
                let scopes = self
                    .locals
                    .suspend()
                    .expect("Async Native Pending 必须保留活动调用帧");
                Ok(MacroCallOutcome::Pending(MacroSuspension {
                    identity,
                    handle: RuntimeNativePending {
                        name: call.name.to_owned(),
                        handle,
                    },
                    scopes,
                }))
            }
            Ok(MacroHandlerOutcome::Complete(mut execution)) => {
                if let Some(lifecycle) = self.macro_lifecycle.as_deref_mut() {
                    let arguments: &[Value] = self
                        .locals
                        .args()
                        .expect("Async Native after 前必须存在调用帧");
                    execution.output = match lifecycle.after(call.name, arguments, execution.output)
                    {
                        Ok(output) => output,
                        Err(diagnostic) => {
                            let _left: bool = self.locals.leave();
                            return Err(RuntimeExecutionError::MacroLifecycle(diagnostic));
                        }
                    };
                }
                let _left: bool = self.locals.leave();
                if execution.control == BodyControl::ExitScope {
                    execution.control = BodyControl::Continue;
                }
                Ok(MacroCallOutcome::Complete(RuntimeMacroExecution {
                    execution,
                    includes_entered: 0,
                }))
            }
        }
    }
}

mod control;

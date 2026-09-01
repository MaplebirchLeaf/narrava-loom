//! 内置逻辑 Macro 可使用的 State、Story 与 Local 能力组合。

use crate::expression::{
    VariableScope,
    evaluator::{ContextWriteError, EvaluationContext, WritableEvaluationContext},
    value::Value,
};
use crate::runtime::RuntimeExecutionIdentity;

use super::{
    CapturedMacroLocals, MacroCallOutcome, MacroDispatchError, MacroHandlerOutcome,
    MacroInvocation, MacroInvocationBody, MacroLocalScopes, MacroSuspension, PreparedMacroCall,
    dispatch_macro,
};

/// 逻辑 Macro 对 Story 的最小请求能力。
pub trait MacroStoryAccess {
    type Error;

    /// 判断指定名称的 Passage 是否存在。
    fn has(&self, name: &str) -> bool;
    /// 统计当前 Story 游标以内的 Passage 访问次数。
    fn visits(&self, name: &str) -> usize;
    /// 请求在当前执行位置包含目标 Passage 正文。
    fn include(&mut self, name: &str) -> Result<(), Self::Error>;
    /// 请求导航到目标 Passage 并停止当前正文。
    fn goto(&mut self, name: &str) -> Result<(), Self::Error>;
}

/// 组合借用 State、Story 和当前 Macro Local Scope，不取得任何所有权。
pub struct MacroLogicContext<'a, Story>
where
    Story: MacroStoryAccess + ?Sized,
{
    state: &'a mut dyn WritableEvaluationContext,
    story: &'a mut Story,
    locals: &'a mut MacroLocalScopes<Value>,
    args: Option<Value>,
    execution_limit: usize,
    executed_steps: usize,
}

impl<'a, Story> MacroLogicContext<'a, Story>
where
    Story: MacroStoryAccess + ?Sized,
{
    /// 组合借用 State、Story 与当前 Macro Local 链，不取得任何所有权。
    pub fn new(
        state: &'a mut dyn WritableEvaluationContext,
        story: &'a mut Story,
        locals: &'a mut MacroLocalScopes<Value>,
    ) -> Self {
        let args: Option<Value> = locals
            .args()
            .map(|values: &[Value]| Value::array(values.to_vec()));
        Self {
            state,
            story,
            locals,
            args,
            execution_limit: 1_000_000,
            executed_steps: 0,
        }
    }

    /// 覆盖当前逻辑正文的节点/循环迭代预算。
    pub fn with_execution_limit(mut self, limit: usize) -> Self {
        self.execution_limit = limit;
        self
    }

    /// 消费一个同步逻辑步骤；false 表示本次正文必须立即终止。
    pub(crate) fn consume_execution_step(&mut self) -> bool {
        if self.executed_steps >= self.execution_limit {
            return false;
        }
        self.executed_steps += 1;
        true
    }

    pub(crate) fn execution_limit(&self) -> usize {
        self.execution_limit
    }

    /// 只读访问 State。
    pub fn state(&self) -> &dyn WritableEvaluationContext {
        self.state
    }

    /// 可写访问 State。
    pub fn state_mut(&mut self) -> &mut dyn WritableEvaluationContext {
        self.state
    }

    /// 只读访问 Story。
    pub fn story(&self) -> &Story {
        self.story
    }

    /// 可写访问 Story。
    pub fn story_mut(&mut self) -> &mut Story {
        self.story
    }

    /// 读取 `@` 局部变量；`args` 返回当前调用帧的只读快照。
    pub fn local(&self, name: &str) -> Option<&Value> {
        match name {
            "args" => self.args.as_ref(),
            _ => self.locals.get(name),
        }
    }
}

impl<Story> EvaluationContext for MacroLogicContext<'_, Story>
where
    Story: MacroStoryAccess + ?Sized,
{
    fn global(&self, name: &str) -> Option<&Value> {
        self.state.global(name)
    }

    fn setup(&self) -> Option<&Value> {
        self.state.setup()
    }

    fn variable(&self, scope: VariableScope, name: &str) -> Option<&Value> {
        match scope {
            VariableScope::Local => self.local(name),
            _ => self.state.variable(scope, name),
        }
    }
}

impl<Story> WritableEvaluationContext for MacroLogicContext<'_, Story>
where
    Story: MacroStoryAccess + ?Sized,
{
    fn set_global(&mut self, name: &str, value: Value) -> Result<(), ContextWriteError> {
        self.state.set_global(name, value)
    }

    fn set_variable(
        &mut self,
        scope: VariableScope,
        name: &str,
        value: Value,
    ) -> Result<(), ContextWriteError> {
        match scope {
            VariableScope::Local => self
                .locals
                .set(name, value)
                .map(|_previous: Option<Value>| ())
                .map_err(|_error| ContextWriteError::Rejected),
            _ => self.state.set_variable(scope, name, value),
        }
    }

    fn set_setup(&mut self, value: Value) -> Result<(), ContextWriteError> {
        self.state.set_setup(value)
    }

    fn del_global(&mut self, name: &str) -> Result<Option<Value>, ContextWriteError> {
        self.state.del_global(name)
    }

    fn del_variable(
        &mut self,
        scope: VariableScope,
        name: &str,
    ) -> Result<Option<Value>, ContextWriteError> {
        match scope {
            VariableScope::Local => self
                .locals
                .del(name)
                .map_err(|_error| ContextWriteError::Rejected),
            _ => self.state.del_variable(scope, name),
        }
    }

    fn authorize_reference_write(&mut self) -> Result<(), ContextWriteError> {
        self.state.authorize_reference_write()
    }

    fn call_script(
        &mut self,
        callable: &crate::expression::value::ScriptCallable,
        arguments: Vec<Value>,
    ) -> Result<Value, crate::expression::evaluator::ScriptCallError> {
        self.state.call_script(callable, arguments)
    }
}

/// 建立调用帧后再组合逻辑 Context，避免提前同时借用 Local Scope。
pub fn execute_prepared_logic_macro<Handler, Body, Story, Output, Pending, HandlerError, Invoke>(
    prepared: PreparedMacroCall<'_, Handler>,
    identity: RuntimeExecutionIdentity,
    body: MacroInvocationBody<'_, Body>,
    state: &mut dyn WritableEvaluationContext,
    story: &mut Story,
    locals: &mut MacroLocalScopes<Value>,
    invoke: Invoke,
) -> Result<MacroCallOutcome<Output, Pending>, MacroDispatchError<HandlerError, Pending>>
where
    Story: MacroStoryAccess + ?Sized,
    Invoke: for<'invoke> FnOnce(
        &Handler,
        MacroInvocation<'invoke, Body, MacroLogicContext<'invoke, Story>>,
    ) -> Result<MacroHandlerOutcome<Output, Pending>, HandlerError>,
{
    let PreparedMacroCall {
        name,
        raw_arguments,
        arguments,
        definition,
    } = prepared;
    let handler_arguments: Vec<Value> = arguments.clone();
    locals.enter_call(arguments);

    let result = {
        let mut context: MacroLogicContext<'_, Story> =
            MacroLogicContext::new(state, story, locals);
        let invocation = MacroInvocation {
            name,
            raw_arguments,
            arguments: &handler_arguments,
            body,
            captures: CapturedMacroLocals::empty(),
            context: &mut context,
        };
        dispatch_macro(definition, invocation, invoke)
    };

    match result {
        Ok(MacroHandlerOutcome::Complete(output)) => {
            let _left: bool = locals.leave();
            Ok(MacroCallOutcome::Complete(output))
        }
        Ok(MacroHandlerOutcome::Pending(handle)) => {
            Ok(MacroCallOutcome::Pending(MacroSuspension {
                identity,
                handle,
                scopes: locals.suspend().expect("活动逻辑 Macro 调用必须可以暂停"),
            }))
        }
        Err(error) => {
            let _left: bool = locals.leave();
            Err(error)
        }
    }
}

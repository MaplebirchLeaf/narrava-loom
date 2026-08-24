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

    fn has(&self, name: &str) -> bool;
    fn include(&mut self, name: &str) -> Result<(), Self::Error>;
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
}

impl<'a, Story> MacroLogicContext<'a, Story>
where
    Story: MacroStoryAccess + ?Sized,
{
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
        }
    }

    pub fn state(&self) -> &dyn WritableEvaluationContext {
        self.state
    }

    pub fn state_mut(&mut self) -> &mut dyn WritableEvaluationContext {
        self.state
    }

    pub fn story(&self) -> &Story {
        self.story
    }

    pub fn story_mut(&mut self) -> &mut Story {
        self.story
    }

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

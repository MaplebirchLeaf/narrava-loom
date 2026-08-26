//! Runtime Macro 的共享测试上下文与分组入口。

use std::path::Path;

use crate::macro_runtime::{
    CapturedMacroLocals, InteractionTargetError, MacroArgument, MacroArgumentIssue,
    MacroArgumentKind, MacroArgumentListError, MacroArgumentValueError, MacroBodyKind,
    MacroCallOutcome, MacroCallPreparationError, MacroCallPreparationIssue, MacroDefinition,
    MacroDefinitionError, MacroDefinitions, MacroDispatchError, MacroEvaluationContext,
    MacroExecutionKind, MacroHandlerOutcome, MacroInteraction, MacroInteractionError,
    MacroInteractions, MacroInvocation, MacroInvocationBody, MacroLifecycleCallbacks,
    MacroLifecycleController, MacroLifecycleError, MacroLifecycleExecutionContext,
    MacroLifecycleHookSequence, MacroLifecycleSubscriptionError, MacroLifecycleSubscriptionId,
    MacroLifecycleSubscriptions, MacroLocalError, MacroLocalScopes, MacroLogicContext,
    MacroResumeError, MacroResumeOutcome, MacroStoryAccess, MacroSuspension, PreparedMacroCall,
    RuntimeMacroHandler, WidgetRegistrationReport, button_with_body, checkbox, dispatch_macro,
    enter_argument_call, execute_prepared_logic_macro, execute_prepared_macro,
    execute_prepared_sync_macro_with_lifecycle, goto, include, link, link_with_body,
    parse_argument_list, parse_interaction_target, prepare_argument_values, prepare_macro_call,
    print, radiobutton, register_story_widgets, register_widget, replace, resume_macro_suspension,
    run, set, textbox, unset,
};
use crate::{
    hir::{
        HirBodyKind, HirBodyNode, HirCapture, HirFor, HirForKind, HirForTarget, HirIf, HirIfBranch,
        HirMacro, HirMacroArguments, HirPassage, HirStory, HirSwitch, HirSwitchCase, HirWhile,
        HirWidget,
    },
    presentation::{
        InteractionId, NavigationRole, PresentationInputKind, PresentationNode, PresentationOutput,
        PresentationRegion, PresentationTarget, PresentationValue, TextStyle, TextTone,
    },
    runtime::{
        AsyncNativeMacroCallbacks, BodyControl, BodyExecution, LogicNodeError,
        NativeMacroCallbacks, NativeMacroError, RuntimeExecutionContext, RuntimeExecutionError,
        RuntimeExecutionIdentity, RuntimeMacroExecution, RuntimeNativePending,
        RuntimeNativeResumeError, WidgetMacroError, execute_logic_body, execute_widget_body,
        execute_widget_macro, resume_async_native_macro,
    },
    source::Source,
    state::State,
    story::{RuntimeStoryAccess, StoryIncludeRequest},
    twee::Span as TweeSpan,
};

use crate::diagnostic::{Diagnostic, DiagnosticLocator, DiagnosticSeverity};
use crate::expression::{
    Span, VariableScope,
    evaluator::{
        ContextWriteError, EvalError, EvaluationContext, WritableEvaluationContext, evaluate_with,
        evaluate_with_mut,
    },
    parse,
    value::{TextValue, Value},
};

mod arguments;
mod control_flow;
mod definitions;
mod diagnostics_and_calls;
mod fragments_and_output;
mod handler_dispatch;
mod hooks;
mod interactions;
mod local_scopes;
mod logic_context;
mod widgets_and_runtime;

struct EmptyEvaluationContext;

impl EvaluationContext for EmptyEvaluationContext {
    fn global(&self, _name: &str) -> Option<&Value> {
        None
    }
}

struct InteractionEvaluationContext {
    location_name: Value,
    location: Value,
}

impl EvaluationContext for InteractionEvaluationContext {
    fn global(&self, _name: &str) -> Option<&Value> {
        None
    }

    fn variable(&self, scope: VariableScope, name: &str) -> Option<&Value> {
        match (scope, name) {
            (VariableScope::Variables, "LocationName") => Some(&self.location_name),
            (VariableScope::Variables, "Location") => Some(&self.location),
            _ => None,
        }
    }
}

struct LogicStateContext {
    count: Value,
}

impl EvaluationContext for LogicStateContext {
    fn global(&self, _name: &str) -> Option<&Value> {
        None
    }

    fn variable(&self, scope: VariableScope, name: &str) -> Option<&Value> {
        matches!((scope, name), (VariableScope::Variables, "count")).then_some(&self.count)
    }
}

impl WritableEvaluationContext for LogicStateContext {
    fn set_global(&mut self, _name: &str, _value: Value) -> Result<(), ContextWriteError> {
        Err(ContextWriteError::Rejected)
    }

    fn set_variable(
        &mut self,
        scope: VariableScope,
        name: &str,
        value: Value,
    ) -> Result<(), ContextWriteError> {
        if matches!((scope, name), (VariableScope::Variables, "count")) {
            self.count = value;
            return Ok(());
        }
        Err(ContextWriteError::Rejected)
    }

    fn del_global(&mut self, _name: &str) -> Result<Option<Value>, ContextWriteError> {
        Err(ContextWriteError::Rejected)
    }

    fn del_variable(
        &mut self,
        scope: VariableScope,
        name: &str,
    ) -> Result<Option<Value>, ContextWriteError> {
        if scope == VariableScope::Variables && name == "count" {
            return Ok(Some(std::mem::replace(&mut self.count, Value::Undefined)));
        }
        Err(ContextWriteError::Rejected)
    }
}

#[derive(Default)]
struct LogicStoryContext {
    included: Vec<String>,
    destination: Option<String>,
}

impl MacroStoryAccess for LogicStoryContext {
    type Error = &'static str;

    fn has(&self, name: &str) -> bool {
        matches!(name, "Start" | "End")
    }

    fn include(&mut self, name: &str) -> Result<(), Self::Error> {
        self.included.push(name.to_owned());
        Ok(())
    }

    fn goto(&mut self, name: &str) -> Result<(), Self::Error> {
        self.destination = Some(name.to_owned());
        Ok(())
    }
}

impl<'hir, 'source> RuntimeStoryAccess<'hir, 'source> for LogicStoryContext {
    fn take_include_request(&mut self) -> Option<StoryIncludeRequest<'hir, 'source>> {
        None
    }
}

fn logic_node(kind: HirBodyKind<'_>) -> HirBodyNode<'_> {
    HirBodyNode {
        kind,
        span: logic_span(),
    }
}

fn logic_set(source: &str) -> HirBodyNode<'_> {
    logic_node(HirBodyKind::Set(Box::new(
        parse(source).expect("测试 set 应可解析"),
    )))
}

fn logic_if<'source>(
    condition: &'source str,
    body: Vec<HirBodyNode<'source>>,
) -> HirBodyNode<'source> {
    logic_node(HirBodyKind::If(HirIf {
        branches: vec![HirIfBranch {
            condition: parse(condition).expect("测试 if 条件应可解析"),
            body,
        }],
        fallback: None,
    }))
}

fn logic_for_target<'source>(source: &'source str) -> HirForTarget<'source> {
    HirForTarget {
        value: parse(source).expect("测试 for 目标应可解析"),
        span: logic_span(),
    }
}

fn logic_span() -> TweeSpan {
    TweeSpan {
        start: 0,
        end: 1,
        line: 1,
        column: 1,
    }
}

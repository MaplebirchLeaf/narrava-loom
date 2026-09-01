//! Engine 导航事务测试。

use std::{cell::RefCell, collections::BTreeMap, path::Path};

use crate::{
    diagnostic::Diagnostic,
    engine::{
        Engine, EngineExecutionLimits, EngineMacroInteractionContinuation,
        EngineMacroInteractionTransaction, EngineMirBeginRequest, EngineMirContinuation,
        EngineMirExecutionError, EngineMirMacroCallbackFailure, EngineMirMacroDispatch,
        EngineMirMacroExecutionError, EngineMirMacroInvocation, EngineMirVmResume,
        EngineNavigation, EngineNavigationChain, EngineNavigationError, EngineNewGame,
        EngineRequestedExecutionError, EngineRequestedNavigation, EngineStart, EngineStartError,
        EngineStoryInit, PassageLifecycleContext, PassageLifecyclePhase,
    },
    expression::{
        parse,
        value::{TextValue, Value},
    },
    hir::{
        HirBodyKind, HirBodyNode, HirCapture, HirMacro, HirMacroArguments, HirPassage, HirStory,
        HirWidget,
    },
    host::{
        HostApi, HostDriveResult, HostExecutionToken, HostMacroInteractionDriveContext,
        HostMacroInteractionPending, HostMacroInteractionResume, HostPendingExecutions,
        HostResumeCallbacks,
    },
    i18n::{I18nRuntimeLanguage, I18nTemplate, I18nTemplateMessage, I18nValidatedTemplate},
    lir::LirProgram,
    macro_runtime::{
        CapturedMacroLocals, MacroDefinition, MacroDefinitions, MacroHandlerOutcome,
        MacroInteraction, MacroInteractions, MacroLocalScopes, MacroResumeOutcome,
        MacroStoryAccess, MacroSuspension, RuntimeMacroHandler, WidgetRegistrationReport,
        register_story_widgets, register_widget,
    },
    mir::{MirMacroBody, MirStory},
    runtime::{
        BodyControl, BodyExecution, RuntimeExecutionContext, RuntimeExecutionError,
        RuntimeExecutionIdentity, RuntimeMacroBodyContinuation, RuntimeMacroExecution,
    },
    semantic::{InteractionId, SemanticNode, SemanticOutput},
    source::Source,
    state::State,
    story::{
        RuntimeStoryAccess, Story, StoryHistoryEntry, StoryIncludeRequest, StoryNavigationError,
        StoryRuntimeRequestError, StoryRuntimeRequests,
    },
    twee::{MacroSyntaxKind, Span as TweeSpan},
    vm::{MirExecutionError, MirExecutionFrame, MirStep},
};

/// 构造只携带控制信号、没有语义输出的执行结果，供引擎闭包测试使用。
fn execution(control: BodyControl) -> BodyExecution {
    BodyExecution {
        control,
        ..Default::default()
    }
}

// 测试按引擎职责拆分；`include!` 让所有用例继续共享本模块的辅助类型，
// 避免为了物理拆文件而改变测试可见性或重复大段构造代码。
include!("engine/interaction_lifecycle.rs");

struct EngineRuntimeStory;

impl MacroStoryAccess for EngineRuntimeStory {
    type Error = &'static str;

    fn has(&self, _name: &str) -> bool {
        false
    }

    fn include(&mut self, _name: &str) -> Result<(), Self::Error> {
        Err("本测试不执行 include")
    }

    fn goto(&mut self, _name: &str) -> Result<(), Self::Error> {
        Err("本测试不执行嵌套 goto")
    }
}

impl<'hir, 'source> RuntimeStoryAccess<'hir, 'source> for EngineRuntimeStory {
    fn take_include_request(&mut self) -> Option<StoryIncludeRequest<'hir, 'source>> {
        None
    }
}

include!("engine/navigation.rs");
include!("engine/mir_transactions.rs");
include!("engine/continuation_navigation.rs");
include!("engine/include.rs");
include!("engine/startup.rs");

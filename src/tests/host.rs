//! 最小 Host API 的输入与输出边界测试。

use std::{collections::BTreeMap, path::Path};

use crate::{
    GameIdentity,
    diagnostic::{Diagnostic, DiagnosticSeverity},
    engine::{
        EngineExecutionLimits, EngineMirContinuation, EngineMirMacroCallbackFailure,
        EngineMirMacroInvocation,
    },
    expression::{
        parse,
        value::{TextValue, Value},
    },
    hir::{
        HirBodyKind, HirBodyNode, HirMacro, HirMacroArguments, HirPassage, HirPrint, HirStory,
        HirWidget,
    },
    host::{
        HostApi, HostDriveResult, HostExecutionToken, HostInput, HostMirAdvanceRequest,
        HostMirRequest, HostPendingExecutions, HostStateView, HostUpdate,
    },
    i18n::{
        I18nLanguageChain, I18nRuntimeLanguage, I18nTemplate, I18nTemplateMessage,
        NlangPackageEntry, NlangPackageInput, NlangValidatedPackage,
    },
    lir::LirProgram,
    macro_runtime::{
        CapturedMacroLocals, MacroDefinition, MacroDefinitions, MacroInteraction,
        MacroInteractions, MacroLocalScopes, MacroResumeOutcome, MacroSuspension,
        RuntimeMacroHandler, WidgetRegistrationReport, link, parse_argument_list,
        prepare_argument_values, register_story_widgets,
    },
    mir::MirStory,
    presentation::{InteractionId, PresentationNode, PresentationOutput},
    runtime::{
        BodyControl, BodyExecution, RuntimeExecutionContext, RuntimeExecutionIdentity,
        RuntimeMacroExecution,
    },
    source::Source,
    state::State,
    story::{Story, StoryRuntimeRequests},
    twee::{MacroSyntaxKind, Span},
};

fn host_update_with_navigation(id: InteractionId, target: &str) -> HostUpdate {
    HostUpdate::new(
        "Start",
        PresentationOutput::from_nodes(vec![PresentationNode::Navigation {
            role: crate::presentation::NavigationRole::Link,
            id,
            label: TextValue::from("前往"),
            target: target.to_owned(),
        }]),
    )
}

// 大型测试集按用例边界拆成物理分片；门面保留共享导入与模块说明。
// include 保持原模块命名、私有辅助项可见性和测试发现路径不变。
include!("host/part_01.rs");
include!("host/part_02.rs");

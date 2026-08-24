//! Engine 跨异步等待保存的事务状态。
//!
//! Interaction 正文与 Passage/MIR 导航拥有不同的恢复边界，分别位于子模块中；
//! 本文件只保留共享依赖与稳定公开导出。

use crate::{
    bytecode::{BytecodeMacroBody, BytecodePassage, BytecodeProgram},
    expression::value::Value,
    i18n::I18nRuntimeLanguage,
    macro_runtime::{
        CapturedMacroLocals, MacroHandlerOutcome, MacroInteraction, MacroLocalScopes,
        MacroLogicContext, MacroResumeOutcome, MacroStoryAccess, MacroSuspension,
    },
    mir::MirMacroBody,
    presentation::{InteractionId, PresentationNode, PresentationOutput},
    runtime::{
        BodyControl, RuntimeExecutionIdentity, RuntimeMacroBodyContinuation,
        RuntimeMacroBodyContinuationResume, RuntimeMacroBodyResumed, RuntimeMacroContinuation,
        RuntimeMacroContinuationError, RuntimeMacroContinuationResume,
        RuntimeMacroContinuationResumeError, RuntimeMacroExecution, RuntimeMacroResumed,
    },
    state::{State, StateCheckpoint},
    story::{
        Story, StoryHistoryEntry, StoryNavigationError, StoryRuntimePending,
        StoryRuntimeRequestError, StoryRuntimeRequests, StorySnapshot, StorySnapshotError,
    },
    vm::{MirExecutionError, MirStep},
};

use super::{
    Engine, EngineExecutionLimits, EngineMirBeginCheckpointRequest, EngineMirBeginError,
    EngineMirBeginRequest, EngineNavigationChain, PassageLifecycleContext, PassageLifecyclePhase,
};

mod interaction;
mod mir;

pub use interaction::*;
pub use mir::*;

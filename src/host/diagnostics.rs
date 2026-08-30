//! Engine、Story 与 VM 失败到稳定 Host Diagnostic 的映射。
//!
//! Core 内部错误携带可回滚事务和实现细节；Host 只能取得稳定错误码与面向作者的
//! 信息。本模块集中维护这道转换边界，避免各个 Host API 入口产生不一致的诊断。

use super::*;

/// Host 边界只暴露稳定诊断，不泄漏 Engine 的事务错误结构。
pub(super) fn host_error(code: &str, message: &str) -> Diagnostic {
    Diagnostic::new(code, DiagnosticSeverity::Error, message)
}

/// 拆出可回滚的事务和真正的失败原因。
///
/// Host 可以改写“回滚也失败”，但回滚成功时必须保留这份诊断，
/// 不得用第二条泛化的“继续执行失败”覆盖它。
pub(super) fn mir_resume_failure<'hir, 'source>(
    error: EngineMirVmResumeError<'hir, 'source>,
) -> (EngineMirResumedTransaction<'hir, 'source>, Diagnostic) {
    match error {
        EngineMirVmResumeError::Story(transaction) => (
            *transaction,
            host_error("engine.story.failed", "Story 运行时请求失败"),
        ),
        EngineMirVmResumeError::StoryRequest { error, transaction } => {
            let diagnostic = match error {
                StoryRuntimeRequestError::Navigation(error) => story_navigation_diagnostic(error),
                StoryRuntimeRequestError::GotoAlreadyPending => {
                    host_error("story.goto.already_pending", "已有未消费的 goto 请求")
                }
            };
            (*transaction, diagnostic)
        }
        EngineMirVmResumeError::Vm { error, transaction } => {
            let diagnostic = match error {
                MirExecutionError::InstructionLimitExceeded { limit } => host_error(
                    "engine.vm.instruction_limit_exceeded",
                    &format!("单次执行超过 Bytecode 指令预算：{limit}"),
                ),
                MirExecutionError::Evaluation(error) => error.diagnostic(),
                MirExecutionError::MissingPassage => {
                    host_error("engine.vm.missing_passage", "VM 找不到当前 Passage")
                }
                MirExecutionError::DifferentI18nCatalog => host_error(
                    "engine.vm.different_i18n_catalog",
                    "VM 语言目录与当前编译结果不匹配",
                ),
                MirExecutionError::InvalidInstructionPointer => {
                    host_error("engine.vm.invalid_instruction_pointer", "VM 指令位置无效")
                }
                MirExecutionError::MissingValueSlot(_) => {
                    host_error("engine.vm.missing_value_slot", "VM 值槽不存在")
                }
                MirExecutionError::MissingIteratorSlot(_) => {
                    host_error("engine.vm.missing_iterator_slot", "VM 迭代器槽不存在")
                }
                MirExecutionError::InvalidText(_) => {
                    host_error("engine.vm.invalid_text", "VM 文本指令无效")
                }
                MirExecutionError::ExpectedMacroPending => {
                    host_error("engine.vm.expected_macro_pending", "VM 期待 Macro 暂停边界")
                }
                MirExecutionError::MacroBodyIncludeUnsupported => host_error(
                    "engine.vm.macro_body_include_unsupported",
                    "Macro 正文尚不支持 include",
                ),
            };
            (*transaction, diagnostic)
        }
        EngineMirVmResumeError::IncludeLimitExceeded { limit, transaction } => (
            *transaction,
            host_error(
                "engine.include.limit_exceeded",
                &format!("include 深度超过限制：{limit}"),
            ),
        ),
        EngineMirVmResumeError::UnexpectedMacroControl {
            control,
            transaction,
        } => (
            *transaction,
            host_error(
                "engine.macro.unexpected_control",
                &format!("Macro 返回了当前边界不接受的控制信号：{control:?}"),
            ),
        ),
    }
}

/// 把 MIR 启动失败映射为稳定诊断；`Continue` 分支先回滚失败事务。
pub(super) fn mir_begin_diagnostic<'hir, 'source>(
    error: EngineMirBeginError<'hir, 'source, Diagnostic>,
    state: &mut State,
    story: &mut Story<'hir, 'source>,
) -> Diagnostic {
    match error {
        EngineMirBeginError::Preparation(error) => match error {
            EngineNavigationError::Navigation(error) => story_navigation_diagnostic(error),
            EngineNavigationError::Rollback { .. } => host_error(
                "engine.rollback.failed",
                "LIR Passage 启动失败，且 Story 检查点无法恢复",
            ),
            EngineNavigationError::Execution(error) => match error {
                EngineRequestedExecutionError::Runtime(
                    EngineMirBeginExecutionError::MissingMirPassage(name),
                ) => host_error(
                    "engine.mir.missing_passage",
                    &format!("MIR 中缺少 Passage：{name}"),
                ),
                EngineRequestedExecutionError::Runtime(
                    EngineMirBeginExecutionError::Lifecycle(error),
                )
                | EngineRequestedExecutionError::Lifecycle {
                    error: EngineMirBeginExecutionError::Lifecycle(error),
                    ..
                } => error,
                EngineRequestedExecutionError::Lifecycle {
                    error: EngineMirBeginExecutionError::MissingMirPassage(name),
                    ..
                } => host_error(
                    "engine.mir.missing_passage",
                    &format!("MIR 中缺少 Passage：{name}"),
                ),
                EngineRequestedExecutionError::PassageLimitExceeded { limit } => host_error(
                    "engine.execution.passage_limit_exceeded",
                    &format!("单次事务执行的 Passage 数量超过限制：{limit}"),
                ),
                _ => host_error(
                    "engine.mir.begin_failed",
                    "LIR Passage 启动请求不符合 Engine 协议",
                ),
            },
        },
        EngineMirBeginError::Continue(error) => {
            let (transaction, diagnostic) = mir_resume_failure(*error);
            let rollback_failed: bool = transaction.rollback(state, story).is_err();
            if rollback_failed {
                host_error(
                    "engine.rollback.failed",
                    "LIR Passage 启动失败，且 Story 检查点无法恢复",
                )
            } else {
                diagnostic
            }
        }
    }
}

/// 把 Story 导航错误映射为稳定诊断。
pub(super) fn story_navigation_diagnostic(error: StoryNavigationError) -> Diagnostic {
    let code: &str = match &error {
        StoryNavigationError::MissingPassage(_) => "story.navigation.missing_passage",
        StoryNavigationError::SpecialPassage(_) => "story.navigation.special_passage",
        StoryNavigationError::DifferentStoryRequest => "story.navigation.different_story",
        StoryNavigationError::HistoryIdExhausted => "story.history.id_exhausted",
    };
    host_error(code, &error.to_string())
}

/// 把请求执行错误映射为稳定诊断。
pub(super) fn execution_diagnostic(error: EngineRequestedExecutionError<Diagnostic>) -> Diagnostic {
    match error {
        EngineRequestedExecutionError::Runtime(error)
        | EngineRequestedExecutionError::Lifecycle { error, .. } => error,
        EngineRequestedExecutionError::MissingGotoRequest => host_error(
            "engine.goto.missing_request",
            "Runtime 请求跳转，但 Story 中没有待处理的 goto 请求",
        ),
        EngineRequestedExecutionError::UnexpectedGotoRequest => host_error(
            "engine.goto.unexpected_request",
            "Story 留有 goto 请求，但 Runtime 没有请求跳转",
        ),
        EngineRequestedExecutionError::StoryInitGotoUnsupported => host_error(
            "engine.story_init.goto_unsupported",
            "StoryInit 初始化阶段不能请求 Passage 跳转",
        ),
        EngineRequestedExecutionError::UnexpectedControl(_) => host_error(
            "engine.control.unexpected",
            "Passage 顶层返回了当前 Engine 阶段不接受的控制信号",
        ),
        EngineRequestedExecutionError::Confirmation(error) => story_navigation_diagnostic(error),
        EngineRequestedExecutionError::PassageLimitExceeded { limit } => host_error(
            "engine.execution.passage_limit_exceeded",
            &format!("单次事务执行的 Passage 数量超过限制：{limit}"),
        ),
        EngineRequestedExecutionError::UnconsumedIncludeRequests { count } => host_error(
            "engine.include.unconsumed_requests",
            &format!("Runtime 结束时仍有未消费的 include 请求：{count}"),
        ),
    }
}

/// 把导航错误映射为稳定诊断，包含回滚失败分支。
pub(super) fn navigation_diagnostic(
    error: EngineNavigationError<EngineRequestedExecutionError<Diagnostic>>,
) -> Diagnostic {
    match error {
        EngineNavigationError::Navigation(error) => story_navigation_diagnostic(error),
        EngineNavigationError::Execution(error) => execution_diagnostic(error),
        EngineNavigationError::Rollback { .. } => host_error(
            "engine.rollback.failed",
            "Passage 事务失败，且 Story 检查点无法恢复",
        ),
    }
}

/// 把 Engine 启动失败映射为稳定诊断。
pub(super) fn start_diagnostic(error: EngineStartError<Diagnostic>) -> Diagnostic {
    match error {
        EngineStartError::AlreadyStarted { current } => host_error(
            "engine.start.already_started",
            &format!("Story 已经启动，当前位置为：{current}"),
        ),
        EngineStartError::Execution(error) => navigation_diagnostic(error),
        EngineStartError::Rollback { .. } => host_error(
            "engine.rollback.failed",
            "Story 启动失败，且 Story 检查点无法恢复",
        ),
    }
}

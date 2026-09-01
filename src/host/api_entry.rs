//! Host 启动、玩家 Interaction 与 MIR 入口编排。

use super::*;

impl HostApi {
    /// 由 Runtime 控制器发起显式 Passage 导航，并复用完整的 MIR、事务、
    /// lifecycle、Reaction 与 Macro continuation 链。
    #[allow(clippy::too_many_arguments)]
    pub fn navigate_mir_with_reaction<
        'hir,
        'source,
        Pending,
        DispatchError,
        Lifecycle,
        Reaction,
        Dispatch,
    >(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        name: &str,
        request: HostMirRequest<'_>,
        lifecycle: Lifecycle,
        reaction: Reaction,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Reaction: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
    {
        let checkpoint = (state.checkpoint(), story.snapshot());
        Self::begin_mir_entry(
            pending,
            state,
            story,
            mir,
            HostMirEntryRequest {
                name,
                params: request.params,
                identity: request.identity,
                limits: request.limits,
                language: request.language,
            },
            Some(checkpoint),
            lifecycle,
            reaction,
            dispatch,
        )
    }

    /// 建立不授予写权限的 State 借用视图。
    pub fn state(state: &State) -> HostStateView<'_> {
        HostStateView::new(state)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn history_mir<'hir, 'source, Pending, DispatchError, Lifecycle, Dispatch>(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        backward: bool,
        request: HostMirRequest<'_>,
        lifecycle: Lifecycle,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
    {
        Self::history_mir_with_reaction(
            pending,
            state,
            story,
            mir,
            backward,
            request,
            lifecycle,
            no_passage_reaction,
            dispatch,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_mir<'hir, 'source, Pending, DispatchError, Initialize, Lifecycle, Dispatch>(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        request: HostMirRequest<'_>,
        initialize: Initialize,
        lifecycle: Lifecycle,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
        Initialize: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            EngineExecutionLimits,
        ) -> Result<BodyExecution, Diagnostic>,
    {
        Self::start_mir_with_reaction(
            pending,
            state,
            story,
            mir,
            request,
            initialize,
            lifecycle,
            no_passage_reaction,
            dispatch,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_special_mir<'hir, 'source, Pending, DispatchError, Lifecycle, Dispatch>(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        name: &str,
        request: HostMirRequest<'_>,
        lifecycle: Lifecycle,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
    {
        Self::render_special_mir_with_reaction(
            pending,
            state,
            story,
            mir,
            name,
            request,
            lifecycle,
            no_passage_reaction,
            dispatch,
        )
    }

    pub fn advance_mir<'hir, 'source, Pending, DispatchError, Lifecycle, Dispatch>(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        request: HostMirAdvanceRequest<'_, '_>,
        lifecycle: Lifecycle,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
    {
        Self::advance_mir_with_reaction(
            pending,
            state,
            story,
            mir,
            request,
            lifecycle,
            no_passage_reaction,
            dispatch,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn advance_macro_interaction_mir<
        'hir,
        'source,
        Pending,
        DispatchError,
        Lifecycle,
        Dispatch,
        ExecuteAction,
    >(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        interactions: &mut MacroInteractions<'hir, 'source>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        request: HostMirAdvanceRequest<'_, '_>,
        execute_action: ExecuteAction,
        lifecycle: Lifecycle,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
        ExecuteAction: FnMut(
            &'hir [crate::hir::HirBodyNode<'source>],
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            &mut MacroLocalScopes<Value>,
        ) -> Result<BodyExecution, Diagnostic>,
    {
        Self::advance_macro_interaction_mir_with_reaction(
            pending,
            interactions,
            state,
            story,
            mir,
            request,
            execute_action,
            lifecycle,
            no_passage_reaction,
            dispatch,
        )
    }

    /// 沿 Story 历史游标重新执行上一项或下一项，不制造新的访问记录。
    #[allow(clippy::too_many_arguments)]
    pub fn history_mir_with_reaction<
        'hir,
        'source,
        Pending,
        DispatchError,
        Lifecycle,
        Reaction,
        Dispatch,
    >(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        backward: bool,
        request: HostMirRequest<'_>,
        mut lifecycle: Lifecycle,
        mut reaction: Reaction,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Reaction: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
    {
        let state_checkpoint = state.checkpoint();
        let story_snapshot = story.snapshot();
        let current = if backward {
            story.back()
        } else {
            story.forward()
        }
        .copied()
        .map_err(|error| {
            Box::new(HostDriveError {
                diagnostic: host_error("story.history.unavailable", &error.to_string()),
                pending: None,
            })
        })?;
        let target_state = story.state_snapshot(current.id()).ok_or_else(|| {
            Box::new(HostDriveError {
                diagnostic: host_error(
                    "story.history.state_unavailable",
                    "历史项缺少进入 Passage 前的持久 State 快照",
                ),
                pending: None,
            })
        })?;
        state.restore_snapshot(target_state);
        let boundary = Engine::begin_mir_chain_from_entry(
            state,
            story,
            mir,
            current,
            request.params,
            request.identity,
            request.limits,
            request.language,
            state_checkpoint,
            story_snapshot,
            &mut lifecycle,
            &mut reaction,
        )
        .map_err(|error| {
            Box::new(HostDriveError {
                diagnostic: mir_begin_diagnostic(error, state, story),
                pending: None,
            })
        })?;
        Self::drive_stable_with_reaction(
            HostStable {
                execution: HostExecutionToken::from_identity(request.identity),
                boundary,
            },
            pending,
            state,
            story,
            mir,
            lifecycle,
            reaction,
            dispatch,
        )
    }

    /// 验证玩家看到的导航，并一次性取得对应的延迟 Macro 动作。
    ///
    /// 验证全部成功前不会消费动作；Binding 因此不能用伪造 ID 或目标触发 HIR 正文。
    pub fn take_macro_interaction<'hir, 'source>(
        presented: &HostUpdate,
        interaction: &InteractionId,
        interactions: &mut MacroInteractions<'hir, 'source>,
    ) -> Result<MacroInteraction<'hir, 'source>, Diagnostic> {
        let presented_target: &str = presented
            .surface
            .interaction_target(interaction)
            .ok_or_else(|| {
                host_error(
                    "host.unknown_interaction",
                    &format!(
                        "交互身份未出现在上一份 SemanticOutput 中：{}",
                        interaction.as_str()
                    ),
                )
            })?;
        let action: &MacroInteraction<'hir, 'source> =
            interactions.get(interaction).ok_or_else(|| {
                host_error(
                    "host.missing_macro_interaction",
                    "交互没有对应的延迟 Macro 动作",
                )
            })?;
        if action.target() != presented_target {
            return Err(host_error(
                "host.macro_interaction_target_mismatch",
                "SemanticOutput 目标与延迟 Macro 动作不一致",
            ));
        }
        Ok(interactions
            .take(interaction)
            .expect("已验证存在的 Macro Interaction 必须能被取走"))
    }

    /// 从固定 `Start` Passage 启动 MIR/VM，并直接驱动到 Ready 或 Pending。
    // 三个回调保留为独立参数，才能让 Rust 对每次短借用应用正确的高阶生命周期。
    #[allow(clippy::too_many_arguments)]
    pub fn start_mir_with_reaction<
        'hir,
        'source,
        Pending,
        DispatchError,
        Initialize,
        Lifecycle,
        Reaction,
        Dispatch,
    >(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        request: HostMirRequest<'_>,
        mut initialize: Initialize,
        mut lifecycle: Lifecycle,
        reaction: Reaction,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Reaction: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
        Initialize: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            EngineExecutionLimits,
        ) -> Result<BodyExecution, Diagnostic>,
    {
        if let Some(current) = story.current() {
            return Err(Box::new(HostDriveError {
                diagnostic: host_error(
                    "engine.start.already_started",
                    &format!("Story 已经启动，当前位置为：{}", current.name),
                ),
                pending: None,
            }));
        }
        let HostMirRequest {
            params,
            identity,
            limits,
            language,
        } = request;
        let state_checkpoint: StateCheckpoint = state.checkpoint();
        let story_snapshot: StorySnapshot<'hir, 'source> = story.snapshot();
        Engine::execute_story_init(state, story, limits, &mut initialize).map_err(|error| {
            Box::new(HostDriveError {
                diagnostic: navigation_diagnostic(error),
                pending: None,
            })
        })?;
        Self::begin_mir_entry(
            pending,
            state,
            story,
            mir,
            HostMirEntryRequest {
                name: crate::story::special::START_PASSAGE,
                params,
                identity,
                limits,
                language,
            },
            Some((state_checkpoint, story_snapshot)),
            &mut lifecycle,
            reaction,
            dispatch,
        )
    }

    /// 在调用方提供的隔离 State/Story 视图中渲染一个特殊 Passage。
    #[allow(clippy::too_many_arguments)]
    pub fn render_special_mir_with_reaction<
        'hir,
        'source,
        Pending,
        DispatchError,
        Lifecycle,
        Reaction,
        Dispatch,
    >(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        name: &str,
        request: HostMirRequest<'_>,
        lifecycle: Lifecycle,
        reaction: Reaction,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Reaction: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
    {
        if !crate::story::special::is_host_region(name) {
            return Err(Box::new(HostDriveError {
                diagnostic: host_error(
                    "host.special.invalid",
                    "只能渲染已登记的 Host Region Passage",
                ),
                pending: None,
            }));
        }
        let HostMirRequest {
            params,
            identity,
            limits,
            language,
        } = request;
        Self::begin_mir_entry(
            pending,
            state,
            story,
            mir,
            HostMirEntryRequest {
                name,
                params,
                identity,
                limits,
                language,
            },
            None,
            lifecycle,
            reaction,
            dispatch,
        )
    }

    /// 验证上一份 SemanticOutput 的玩家动作，再进入目标 Passage 的 MIR 链。
    #[allow(clippy::too_many_arguments)]
    pub fn advance_mir_with_reaction<
        'hir,
        'source,
        Pending,
        DispatchError,
        Lifecycle,
        Reaction,
        Dispatch,
    >(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        request: HostMirAdvanceRequest<'_, '_>,
        mut lifecycle: Lifecycle,
        reaction: Reaction,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Reaction: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
    {
        let HostMirAdvanceRequest {
            presented,
            input,
            params,
            identity,
            limits,
            language,
        } = request;
        let HostInput::Activate { interaction } = input else {
            return Err(Box::new(HostDriveError {
                diagnostic: host_error(
                    "host.async_input.requires_pending",
                    "异步恢复或取消必须交给持有对应 continuation 的 Host 异步入口",
                ),
                pending: None,
            }));
        };
        let target: &str = presented
            .surface
            .interaction_target(&interaction)
            .ok_or_else(|| {
                Box::new(HostDriveError {
                    diagnostic: host_error(
                        "host.unknown_interaction",
                        &format!(
                            "交互身份未出现在上一份 SemanticOutput 中：{}",
                            interaction.as_str()
                        ),
                    ),
                    pending: None,
                })
            })?;
        Self::begin_mir_entry(
            pending,
            state,
            story,
            mir,
            HostMirEntryRequest {
                name: target,
                params,
                identity,
                limits,
                language,
            },
            None,
            &mut lifecycle,
            reaction,
            dispatch,
        )
    }

    /// 执行容器 Interaction 正文，并在同一检查点上进入其导航目标。
    ///
    /// 当前正文入口是同步边界；异步 Macro 需要后续专用 continuation，不能在这里
    /// 假装已经完成。正文输出不会短暂呈现，正文应主要用于 State 与逻辑副作用。
    #[allow(clippy::too_many_arguments)]
    pub fn advance_macro_interaction_mir_with_reaction<
        'hir,
        'source,
        Pending,
        DispatchError,
        Lifecycle,
        Reaction,
        Dispatch,
        ExecuteAction,
    >(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        interactions: &mut MacroInteractions<'hir, 'source>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        request: HostMirAdvanceRequest<'_, '_>,
        mut execute_action: ExecuteAction,
        mut lifecycle: Lifecycle,
        reaction: Reaction,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Reaction: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
        ExecuteAction: FnMut(
            &'hir [crate::hir::HirBodyNode<'source>],
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            &mut MacroLocalScopes<Value>,
        ) -> Result<BodyExecution, Diagnostic>,
    {
        let HostMirAdvanceRequest {
            presented,
            input,
            params,
            identity,
            limits,
            language,
        } = request;
        let HostInput::Activate { interaction } = input else {
            return Err(Box::new(HostDriveError {
                diagnostic: host_error(
                    "host.async_input.requires_pending",
                    "异步恢复或取消必须交给持有对应 continuation 的 Host 异步入口",
                ),
                pending: None,
            }));
        };
        let action: MacroInteraction<'hir, 'source> =
            Self::take_macro_interaction(presented, &interaction, interactions).map_err(
                |diagnostic: Diagnostic| {
                    Box::new(HostDriveError {
                        diagnostic,
                        pending: None,
                    })
                },
            )?;
        let (target, body, captures) = action.into_parts();
        let restore_captures = captures.clone();
        let state_checkpoint: StateCheckpoint = state.checkpoint();
        let story_snapshot: StorySnapshot<'hir, 'source> = story.snapshot();
        let mut scopes: MacroLocalScopes<Value> = captures.into_scopes();
        let (execution, include_count, has_goto) = {
            let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
                StoryRuntimeRequests::new(story);
            let execution: Result<BodyExecution, Diagnostic> =
                execute_action(body, state, &mut requests, &mut scopes);
            let include_count: usize = requests.pending_include_count();
            let has_goto: bool = requests.take_goto().is_some();
            (execution, include_count, has_goto)
        };
        let action_error: Option<Diagnostic> = match execution {
            Err(error) => Some(error),
            Ok(_) if include_count != 0 => Some(host_error(
                "host.macro_interaction.unconsumed_includes",
                "延迟 Interaction 正文留下了未消费的 include 请求",
            )),
            Ok(_) if has_goto => Some(host_error(
                "host.macro_interaction.unexpected_goto",
                "延迟 Interaction 正文不能覆盖自身的导航目标",
            )),
            Ok(execution)
                if !matches!(
                    execution.control,
                    crate::runtime::BodyControl::Continue | crate::runtime::BodyControl::ExitScope
                ) =>
            {
                Some(host_error(
                    "host.macro_interaction.unexpected_control",
                    "延迟 Interaction 正文返回了不属于当前作用域的控制信号",
                ))
            }
            Ok(_) => None,
        };
        if let Some(diagnostic) = action_error {
            state.restore_checkpoint(state_checkpoint);
            let rollback_failed: bool = story.restore(story_snapshot).is_err();
            interactions
                .add(
                    interaction,
                    MacroInteraction::new(&target, body, restore_captures),
                )
                .expect("失败后原 Interaction ID 必须仍为空闲");
            return Err(Box::new(HostDriveError {
                diagnostic: if rollback_failed {
                    host_error(
                        "engine.rollback.failed",
                        "Interaction 正文失败，且 Story 检查点无法恢复",
                    )
                } else {
                    diagnostic
                },
                pending: None,
            }));
        }

        let result = Self::begin_mir_entry(
            pending,
            state,
            story,
            mir,
            HostMirEntryRequest {
                name: &target,
                params,
                identity,
                limits,
                language,
            },
            Some((state_checkpoint, story_snapshot)),
            &mut lifecycle,
            reaction,
            dispatch,
        );
        if result.is_err() {
            interactions
                .add(
                    interaction,
                    MacroInteraction::new(&target, body, restore_captures),
                )
                .expect("导航启动失败后原 Interaction ID 必须仍为空闲");
        }
        result
    }

    // 内部入口沿用公开边界的独立生命周期回调，避免仅为计数增加一次性包装类型。
    /// 共享的 MIR 入口：建立 Engine 请求，并把初始边界驱动到 Ready 或 Pending。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn begin_mir_entry<
        'hir,
        'source,
        Pending,
        DispatchError,
        Lifecycle,
        Reaction,
        Dispatch,
    >(
        pending: &mut HostPendingExecutions<EngineMirContinuation<'hir, 'source, Pending>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        mir: &BytecodeProgram,
        request: HostMirEntryRequest<'_>,
        checkpoint: Option<(StateCheckpoint, StorySnapshot<'hir, 'source>)>,
        mut lifecycle: Lifecycle,
        mut reaction: Reaction,
        dispatch: Dispatch,
    ) -> Result<HostDriveResult, Box<HostDriveError<Pending>>>
    where
        Lifecycle: FnMut(
            crate::engine::PassageLifecyclePhase,
            crate::engine::PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), Diagnostic>,
        Reaction: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, Diagnostic>,
        Dispatch: FnMut(
            EngineMirMacroInvocation<'_>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            MacroLocalScopes<Value>,
        ) -> Result<
            MacroResumeOutcome<RuntimeMacroExecution, Pending>,
            EngineMirMacroCallbackFailure<DispatchError>,
        >,
    {
        let HostMirEntryRequest {
            name,
            params,
            identity,
            limits,
            language,
        } = request;
        let request: EngineMirBeginRequest<'_> = EngineMirBeginRequest {
            name,
            params,
            identity,
            limits,
            language,
        };
        let boundary: EngineMirVmResume<'hir, 'source> = match checkpoint {
            Some((state_checkpoint, story_snapshot)) => {
                Engine::begin_mir_chain_from_checkpoint_with_reaction(
                    state,
                    story,
                    mir,
                    EngineMirBeginCheckpointRequest {
                        request,
                        state_checkpoint,
                        story_snapshot,
                    },
                    &mut lifecycle,
                    &mut reaction,
                )
            }
            None => Engine::begin_mir_chain_with_reaction(
                state,
                story,
                mir,
                request,
                &mut lifecycle,
                &mut reaction,
            ),
        }
        .map_err(|error| {
            Box::new(HostDriveError {
                diagnostic: mir_begin_diagnostic(error, state, story),
                pending: None,
            })
        })?;
        let stable: HostStable<'hir, 'source> = HostStable {
            execution: HostExecutionToken::from_identity(identity),
            boundary,
        };
        Self::drive_stable_with_reaction(
            stable, pending, state, story, mir, lifecycle, reaction, dispatch,
        )
    }
}

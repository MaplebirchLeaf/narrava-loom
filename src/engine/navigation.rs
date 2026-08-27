//! Engine 导航事务。
//!
//! 单 Passage 导航、带 Story 请求确认的导航与连续 goto 链都在本文件实现；
//! 所有导航共享同一检查点与回滚路径。

use super::*;

impl Engine {
    /// 导航到指定 Passage，并在事务内执行一次不携带请求的正文调用。
    pub fn navigate<'hir, 'source, Output, ExecutionError, Execute>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        name: &str,
        execute: Execute,
    ) -> Result<EngineNavigation<'hir, 'source, Output>, EngineNavigationError<ExecutionError>>
    where
        Execute: FnOnce(&HirPassage<'source>, &mut State) -> Result<Output, ExecutionError>,
    {
        let state_checkpoint: StateCheckpoint = state.checkpoint();
        let story_snapshot: StorySnapshot<'hir, 'source> = story.snapshot();
        let entry: StoryHistoryEntry<'hir, 'source> = *story
            .goto(name)
            .map_err(EngineNavigationError::Navigation)?;
        let _removed_temporary: usize = state.temporary_clear();

        match execute(entry.passage(), state) {
            Ok(output) => Ok(EngineNavigation { entry, output }),
            Err(execution) => Err(Self::rollback(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                execution,
            )),
        }
    }

    /// 执行单个 Passage，并在停止后确认至多一个 pending goto。
    pub fn navigate_with_requests<'hir, 'source, RuntimeError, Execute>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        name: &str,
        execute: Execute,
    ) -> Result<
        EngineRequestedNavigation<'hir, 'source>,
        EngineNavigationError<EngineRequestedExecutionError<RuntimeError>>,
    >
    where
        Execute: FnOnce(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
        ) -> Result<BodyExecution, RuntimeError>,
    {
        let state_checkpoint: StateCheckpoint = state.checkpoint();
        let story_snapshot: StorySnapshot<'hir, 'source> = story.snapshot();
        let entered: StoryHistoryEntry<'hir, 'source> = *story
            .goto(name)
            .map_err(EngineNavigationError::Navigation)?;
        let _removed_temporary: usize = state.temporary_clear();
        // Adapter 只在正文执行期间借用 Story；离开代码块后 Engine 才能确认请求。
        let (execution, pending, include_count): (
            Result<BodyExecution, RuntimeError>,
            Option<StoryNavigationRequest<'hir, 'source>>,
            usize,
        ) = {
            let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
                StoryRuntimeRequests::new(story);
            let execution: Result<BodyExecution, RuntimeError> =
                execute(entered.passage(), state, &mut requests);
            let pending: Option<StoryNavigationRequest<'hir, 'source>> = requests.take_goto();
            let include_count: usize = requests.pending_include_count();
            (execution, pending, include_count)
        };

        let execution: BodyExecution = match execution {
            Ok(execution) => execution,
            Err(error) => {
                return Err(Self::rollback(
                    state,
                    story,
                    state_checkpoint,
                    story_snapshot,
                    EngineRequestedExecutionError::Runtime(error),
                ));
            }
        };
        let control: BodyControl = execution.control;

        if include_count != 0 {
            return Err(Self::rollback(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                EngineRequestedExecutionError::UnconsumedIncludeRequests {
                    count: include_count,
                },
            ));
        }

        match (control, pending) {
            (BodyControl::Continue, None) => Ok(EngineRequestedNavigation {
                entered,
                requested: None,
                output: execution.output,
            }),
            (BodyControl::StopPassage, Some(request)) => match story.confirm_navigation(request) {
                Ok(entry) => Ok(EngineRequestedNavigation {
                    entered,
                    requested: Some(*entry),
                    output: execution.output,
                }),
                Err(error) => Err(Self::rollback(
                    state,
                    story,
                    state_checkpoint,
                    story_snapshot,
                    EngineRequestedExecutionError::Confirmation(error),
                )),
            },
            (BodyControl::StopPassage, None) => Err(Self::rollback(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                EngineRequestedExecutionError::MissingGotoRequest,
            )),
            (BodyControl::Continue, Some(_request)) => Err(Self::rollback(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                EngineRequestedExecutionError::UnexpectedGotoRequest,
            )),
            (control, _pending) => Err(Self::rollback(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                EngineRequestedExecutionError::UnexpectedControl(control),
            )),
        }
    }

    /// 连续执行 goto 目标；未安装生命周期回调时保持原有行为。
    pub fn navigate_chain_with_requests<'hir, 'source, RuntimeError, Execute>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        name: &str,
        limits: EngineExecutionLimits,
        execute: Execute,
    ) -> Result<
        EngineNavigationChain<'hir, 'source>,
        EngineNavigationError<EngineRequestedExecutionError<RuntimeError>>,
    >
    where
        Execute: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            EngineExecutionLimits,
        ) -> Result<BodyExecution, RuntimeError>,
    {
        let params: Value = Value::Undefined;
        Self::navigate_chain_with_lifecycle(
            state,
            story,
            name,
            &params,
            limits,
            |_phase: PassageLifecyclePhase,
             _context: PassageLifecycleContext<'_, '_, '_, '_>,
             _state: &mut State| { Ok::<(), RuntimeError>(()) },
            execute,
        )
    }

    /// 连续执行导航 Passage，并在事务内发布不依赖宿主渲染的生命周期阶段。
    pub fn navigate_chain_with_lifecycle<'hir, 'source, RuntimeError, Lifecycle, Execute>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        name: &str,
        params: &Value,
        limits: EngineExecutionLimits,
        mut lifecycle: Lifecycle,
        mut execute: Execute,
    ) -> Result<
        EngineNavigationChain<'hir, 'source>,
        EngineNavigationError<EngineRequestedExecutionError<RuntimeError>>,
    >
    where
        Lifecycle: FnMut(
            PassageLifecyclePhase,
            PassageLifecycleContext<'_, 'hir, 'source, '_>,
            &mut State,
        ) -> Result<(), RuntimeError>,
        Execute: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            EngineExecutionLimits,
        ) -> Result<BodyExecution, RuntimeError>,
    {
        let state_checkpoint: StateCheckpoint = state.checkpoint();
        let story_snapshot: StorySnapshot<'hir, 'source> = story.snapshot();
        let mut current: StoryHistoryEntry<'hir, 'source> = *story
            .goto(name)
            .map_err(EngineNavigationError::Navigation)?;
        let mut entries: Vec<StoryHistoryEntry<'hir, 'source>> = vec![current];
        let mut executed: usize = 0;
        let mut output: SemanticOutput = SemanticOutput::default();
        let empty_params: Value = Value::Undefined;

        loop {
            if executed == limits.passages {
                return Err(Self::rollback(
                    state,
                    story,
                    state_checkpoint,
                    story_snapshot,
                    EngineRequestedExecutionError::PassageLimitExceeded {
                        limit: limits.passages,
                    },
                ));
            }

            // 每个 Passage 都从新的 temporary 生命周期开始。
            let _removed_temporary: usize = state.temporary_clear();
            // 只有入口 Passage 接收本次调用参数；goto 目标以后由导航请求携带自己的参数。
            let current_params: &Value = if executed == 0 { params } else { &empty_params };
            let context: PassageLifecycleContext<'_, 'hir, 'source, '_> =
                PassageLifecycleContext::new(current, current_params);
            for phase in [PassageLifecyclePhase::Init, PassageLifecyclePhase::Start] {
                if let Err(error) = lifecycle(phase, context, state) {
                    return Err(Self::rollback(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        EngineRequestedExecutionError::Lifecycle { phase, error },
                    ));
                }
            }
            let (execution, pending, include_count): (
                Result<BodyExecution, RuntimeError>,
                Option<StoryNavigationRequest<'hir, 'source>>,
                usize,
            ) = {
                let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
                    StoryRuntimeRequests::new(story);
                let execution: Result<BodyExecution, RuntimeError> =
                    execute(current.passage(), state, &mut requests, limits);
                let pending: Option<StoryNavigationRequest<'hir, 'source>> = requests.take_goto();
                let include_count: usize = requests.pending_include_count();
                (execution, pending, include_count)
            };
            executed += 1;

            let mut execution: BodyExecution = match execution {
                Ok(execution) => execution,
                Err(error) => {
                    return Err(Self::rollback(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        EngineRequestedExecutionError::Runtime(error),
                    ));
                }
            };
            let control: BodyControl = execution.control;
            // 记录本跳是否包含作者导航动作，作为后续安全返回目标的选择依据。
            story.record_navigation(execution.output.has_navigation());

            if include_count != 0 {
                return Err(Self::rollback(
                    state,
                    story,
                    state_checkpoint,
                    story_snapshot,
                    EngineRequestedExecutionError::UnconsumedIncludeRequests {
                        count: include_count,
                    },
                ));
            }

            match (control, pending) {
                (BodyControl::Continue, None) => {
                    // `[exit]` 仍执行正文，但不会进入宿主的呈现阶段。
                    if !current.passage().has_tag("exit") {
                        // 本跳没有作者导航动作时，追加指向最近安全返回点的语义动作。
                        // 历史中没有可用目标时暂不追加（启动入口回退与 Diagnostic 后续接入）。
                        if !execution.output.has_navigation()
                            && let Some(target) = story.safe_return_target()
                        {
                            execution.output.push(SemanticNode::SafeReturn {
                                id: InteractionId::from_key(format!("safe-return:{}", target.name)),
                                target: target.name.to_owned(),
                            });
                        }
                        // Render 阶段已取得语义输出；Display 阶段将它暴露给 Host。
                        let render_context: PassageLifecycleContext<'_, '_, '_, '_> =
                            PassageLifecycleContext::with_output(
                                current,
                                current_params,
                                &execution.output,
                            );
                        for phase in [
                            PassageLifecyclePhase::Render,
                            PassageLifecyclePhase::Display,
                        ] {
                            if let Err(error) = lifecycle(phase, render_context, state) {
                                return Err(Self::rollback(
                                    state,
                                    story,
                                    state_checkpoint,
                                    story_snapshot,
                                    EngineRequestedExecutionError::Lifecycle { phase, error },
                                ));
                            }
                        }
                    }
                    output.append(execution.output);
                    return Ok(EngineNavigationChain { entries, output });
                }
                (BodyControl::StopPassage, Some(request)) => {
                    output.append(execution.output);
                    if let Err(error) = lifecycle(PassageLifecyclePhase::End, context, state) {
                        return Err(Self::rollback(
                            state,
                            story,
                            state_checkpoint,
                            story_snapshot,
                            EngineRequestedExecutionError::Lifecycle {
                                phase: PassageLifecyclePhase::End,
                                error,
                            },
                        ));
                    }
                    let confirmed: Result<StoryHistoryEntry<'hir, 'source>, StoryNavigationError> =
                        story.confirm_navigation(request).copied();
                    match confirmed {
                        Ok(entry) => {
                            current = entry;
                            entries.push(entry);
                        }
                        Err(error) => {
                            return Err(Self::rollback(
                                state,
                                story,
                                state_checkpoint,
                                story_snapshot,
                                EngineRequestedExecutionError::Confirmation(error),
                            ));
                        }
                    }
                }
                (BodyControl::StopPassage, None) => {
                    return Err(Self::rollback(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        EngineRequestedExecutionError::MissingGotoRequest,
                    ));
                }
                (BodyControl::Continue, Some(_request)) => {
                    return Err(Self::rollback(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        EngineRequestedExecutionError::UnexpectedGotoRequest,
                    ));
                }
                (control, _pending) => {
                    return Err(Self::rollback(
                        state,
                        story,
                        state_checkpoint,
                        story_snapshot,
                        EngineRequestedExecutionError::UnexpectedControl(control),
                    ));
                }
            }
        }
    }
}

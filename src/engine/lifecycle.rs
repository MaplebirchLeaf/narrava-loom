use super::*;

impl Engine {
    /// 执行可选 StoryInit；它不确认导航，也不发布普通 Passage 生命周期。
    pub fn execute_story_init<'hir, 'source, RuntimeError, Execute>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        limits: EngineExecutionLimits,
        mut execute: Execute,
    ) -> Result<EngineStoryInit, EngineNavigationError<EngineRequestedExecutionError<RuntimeError>>>
    where
        Execute: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            EngineExecutionLimits,
        ) -> Result<BodyExecution, RuntimeError>,
    {
        let Some(passage): Option<&'hir HirPassage<'source>> = story.story_init() else {
            return Ok(EngineStoryInit::Missing);
        };
        let state_checkpoint: StateCheckpoint = state.checkpoint();
        let story_snapshot: StorySnapshot<'hir, 'source> = story.snapshot();
        let (execution, has_goto, include_count): (
            Result<BodyExecution, RuntimeError>,
            bool,
            usize,
        ) = {
            let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
                StoryRuntimeRequests::new(story);
            let execution: Result<BodyExecution, RuntimeError> =
                execute(passage, state, &mut requests, limits);
            let has_goto: bool = requests.take_goto().is_some();
            let include_count: usize = requests.pending_include_count();
            (execution, has_goto, include_count)
        };

        let control: BodyControl = match execution {
            Ok(execution) => execution.control,
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
        if has_goto {
            return Err(Self::rollback(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                EngineRequestedExecutionError::StoryInitGotoUnsupported,
            ));
        }
        if control != BodyControl::Continue {
            return Err(Self::rollback(
                state,
                story,
                state_checkpoint,
                story_snapshot,
                EngineRequestedExecutionError::UnexpectedControl(control),
            ));
        }
        Ok(EngineStoryInit::Executed)
    }

    pub fn start<'hir, 'source, RuntimeError, Execute>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        name: &str,
        limits: EngineExecutionLimits,
        execute: Execute,
    ) -> Result<EngineStart<'hir, 'source>, EngineStartError<RuntimeError>>
    where
        Execute: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            EngineExecutionLimits,
        ) -> Result<BodyExecution, RuntimeError>,
    {
        let params: Value = Value::Undefined;
        Self::start_with_lifecycle(
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

    /// 首次启动 Story，并把入口参数交给初始 Passage 的生命周期。
    pub fn start_with_lifecycle<'hir, 'source, RuntimeError, Lifecycle, Execute>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        name: &str,
        params: &Value,
        limits: EngineExecutionLimits,
        lifecycle: Lifecycle,
        mut execute: Execute,
    ) -> Result<EngineStart<'hir, 'source>, EngineStartError<RuntimeError>>
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
        if let Some(current) = story.current() {
            return Err(EngineStartError::AlreadyStarted {
                current: current.name.to_owned(),
            });
        }

        // 外层检查点覆盖 StoryInit 与起始 Passage，避免启动失败留下初始化副作用。
        let state_checkpoint: StateCheckpoint = state.checkpoint();
        let story_snapshot: StorySnapshot<'hir, 'source> = story.snapshot();
        let _story_init: EngineStoryInit =
            Self::execute_story_init(state, story, limits, &mut execute)
                .map_err(EngineStartError::Execution)?;
        let navigation: EngineNavigationChain<'hir, 'source> =
            match Self::navigate_chain_with_lifecycle(
                state, story, name, params, limits, lifecycle, execute,
            ) {
                Ok(navigation) => navigation,
                Err(execution) => {
                    state.restore_checkpoint(state_checkpoint);
                    return match story.restore(story_snapshot) {
                        Ok(()) => Err(EngineStartError::Execution(execution)),
                        Err(story) => Err(EngineStartError::Rollback { execution, story }),
                    };
                }
            };
        // 成功链必定至少含有已经执行的起始 Passage。
        let initial: StoryHistoryEntry<'hir, 'source> = *navigation
            .entries
            .first()
            .expect("成功的 Engine 启动链必须包含起始 Passage");
        let current: StoryHistoryEntry<'hir, 'source> = *navigation
            .entries
            .last()
            .expect("成功的 Engine 启动链必须包含当前位置");
        Ok(EngineStart {
            initial,
            current,
            entries: navigation.entries,
            output: navigation.output,
        })
    }

    /// 结束旧 Passage、重置游戏领域并从指定 Passage 原子启动新游戏。
    pub fn new_game_with_lifecycle<'hir, 'source, RuntimeError, Lifecycle, Execute>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        name: &str,
        params: &Value,
        limits: EngineExecutionLimits,
        mut lifecycle: Lifecycle,
        execute: Execute,
    ) -> Result<EngineNewGame<'hir, 'source>, EngineNewGameError<RuntimeError>>
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
        let empty_params: Value = Value::Undefined;

        if let Some(current) = story.current_entry().copied() {
            let context: PassageLifecycleContext<'_, 'hir, 'source, '_> =
                PassageLifecycleContext::new(current, &empty_params);
            if let Err(error) = lifecycle(PassageLifecyclePhase::End, context, state) {
                return Err(Self::rollback_new_game(
                    state,
                    story,
                    state_checkpoint,
                    story_snapshot,
                    EngineNewGameFailure::Lifecycle {
                        phase: PassageLifecyclePhase::End,
                        error,
                    },
                ));
            }
        }

        let state_reset: StateReset = state.reset_game();
        let history_removed: usize = story.reset();
        let start: EngineStart<'hir, 'source> = match Self::start_with_lifecycle(
            state, story, name, params, limits, lifecycle, execute,
        ) {
            Ok(start) => start,
            Err(error) => {
                return Err(Self::rollback_new_game(
                    state,
                    story,
                    state_checkpoint,
                    story_snapshot,
                    EngineNewGameFailure::Start(error),
                ));
            }
        };

        Ok(EngineNewGame {
            state: state_reset,
            history_removed,
            start,
        })
    }
}

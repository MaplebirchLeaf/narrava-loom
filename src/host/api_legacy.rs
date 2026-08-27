//! 非 MIR 的兼容 Host 入口；只负责同步 HIR 执行结果收束。

use super::*;

impl HostApi {
    /// 启动 Story，并把 Engine 的内部历史结果收束成 Host 更新。
    pub fn start<'hir, 'source, Execute>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        limits: EngineExecutionLimits,
        execute: Execute,
    ) -> Result<HostUpdate, Diagnostic>
    where
        Execute: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            EngineExecutionLimits,
        ) -> Result<BodyExecution, Diagnostic>,
    {
        let started: EngineStart<'hir, 'source> = Engine::start(
            state,
            story,
            crate::story::special::START_PASSAGE,
            limits,
            execute,
        )
        .map_err(start_diagnostic)?;
        Ok(HostUpdate::new(
            started.current.passage().name,
            started.output,
        ))
    }

    /// 验证并执行 Host 输入；平台事件不得绕过 Engine 直接改写 Story。
    pub fn advance<'hir, 'source, Execute>(
        state: &mut State,
        story: &mut Story<'hir, 'source>,
        presented: &HostUpdate,
        input: HostInput,
        limits: EngineExecutionLimits,
        execute: Execute,
    ) -> Result<HostUpdate, Diagnostic>
    where
        Execute: FnMut(
            &HirPassage<'source>,
            &mut State,
            &mut StoryRuntimeRequests<'_, 'hir, 'source>,
            EngineExecutionLimits,
        ) -> Result<BodyExecution, Diagnostic>,
    {
        match input {
            HostInput::Activate { interaction } => {
                let target: String = presented
                    .surface
                    .interaction_target(&interaction)
                    .ok_or_else(|| {
                        host_error(
                            "host.unknown_interaction",
                            &format!(
                                "交互身份未出现在上一份 Surface 中：{}",
                                interaction.as_str()
                            ),
                        )
                    })?
                    .to_owned();
                let navigation =
                    Engine::navigate_chain_with_requests(state, story, &target, limits, execute)
                        .map_err(navigation_diagnostic)?;
                let current = navigation
                    .entries
                    .last()
                    .expect("成功的 Host 导航必须包含当前位置");
                Ok(HostUpdate::new(current.passage().name, navigation.output))
            }
            HostInput::Resume { .. } | HostInput::Cancel { .. } => Err(host_error(
                "host.async_input.requires_pending",
                "异步恢复或取消必须交给持有对应 continuation 的 Host 异步入口",
            )),
        }
    }
}

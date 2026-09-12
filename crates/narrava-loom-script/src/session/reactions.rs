//! 命令完成后的 Reaction 安全点；导航与输出延续同一事务。

use super::*;

impl<'hir, 'source> RuntimeSession<'hir, 'source> {
    /// 在命令边界统一处理作者 Event 与持久 State 变化。Setter、Event.emit 与
    /// Reaction cond 都只收集事实，不允许重入 Engine；所有叙事效果在这里顺序提交。
    pub(super) fn settle_reactions(
        &mut self,
        update: RuntimeUpdate,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let transaction = self.transaction.as_mut().expect("执行命令必须持有事务");
        let mut before: Rc<StateSnapshot> = transaction.before.clone();
        let mut output: SemanticOutput = std::mem::take(&mut transaction.output);
        let mut goto: Option<String> = None;
        let mut settled = false;

        for _round in 0..256 {
            let mut reactions = self
                .script
                .resolve_queued_event_reactions(self.story.current(), &mut self.state)
                .map_err(crate::ScriptError::into_host_error)?;
            reactions.extend(
                self.script
                    .resolve_state_reactions(self.story.current(), &before, &mut self.state)
                    .map_err(crate::ScriptError::into_host_error)?,
            );
            if reactions.is_empty() {
                settled = true;
                break;
            }
            // 下一轮必须从“本轮效果执行前”继续比较，才能观察效果自身写入的 State。
            let effect_before = self.state.snapshot();
            let (reaction_output, reaction_goto) = self.apply_reaction_effects(reactions)?;
            output.append(reaction_output);
            if let Some(target) = reaction_goto
                && goto.replace(target).is_some()
            {
                return Err(HostErrorDto::new(
                    "reaction.multiple_goto",
                    "同一 Reaction 安全点只能发起一次导航",
                ));
            }
            before = Rc::new(effect_before);
        }
        if !settled {
            return Err(HostErrorDto::new(
                "reaction.execution_limit",
                "Reaction 安全点超过 256 轮执行上限",
            ));
        }

        if let Some(target) = goto {
            self.transaction
                .as_mut()
                .expect("Reaction 导航延续当前事务")
                .before = before;
            // 与普通 goto 一致，被替代 Passage 的输出（含特殊区域与交互）不进入目标页。
            // 仅保留同一 Reaction 安全点在 goto 前明确产生的效果输出。
            self.transaction
                .as_mut()
                .expect("Reaction 导航延续当前事务")
                .output = output;
            let driven = self.navigate(&target)?;
            return if matches!(driven, RuntimeUpdate::Pending { .. }) {
                Ok(driven)
            } else {
                self.settle_reactions(driven)
            };
        }

        let has_output: bool = !output.is_empty();
        if has_output && let Some(presented) = self.presented.as_ref() {
            let mut amended = presented.as_ref().clone();
            amended.append_surface(output);
            self.presented = Some(Rc::new(amended));
        }
        let inputs_changed: bool = self.sync_presented_inputs()?;
        if (has_output || inputs_changed)
            && let Some(presented) = self.presented.as_ref()
        {
            let dto =
                encode_host_update(presented, self.story.can_back(), self.story.can_forward());
            return Ok(RuntimeUpdate::Ready { update: dto });
        }
        Ok(update)
    }

    fn apply_reaction_effects(
        &mut self,
        reactions: Vec<narrava_loom_core::reaction::ReactionEffect>,
    ) -> Result<(SemanticOutput, Option<String>), HostErrorDto> {
        let mut requests = narrava_loom_core::story::StoryRuntimeRequests::new(&self.story);
        let mut output = SemanticOutput::default();
        for effect in reactions {
            let execution = crate::reaction_runtime::apply_effect(
                self.hir,
                &effect,
                &mut self.state,
                &mut requests,
            )
            .map_err(diagnostic)?;
            output.append(execution.output);
        }
        let target = requests
            .take_goto()
            .map(|request| request.passage().name.to_owned());
        Ok((output, target))
    }
}

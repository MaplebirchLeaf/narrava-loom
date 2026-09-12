//! 隔离 State/Story 视图中的公共区域渲染与异步恢复。

use super::*;

impl<'hir, 'source> RuntimeSession<'hir, 'source> {
    pub(super) fn finish_specials(
        &mut self,
        mut update: HostUpdate,
        mut next_special: usize,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        self.script.finish_refresh_body(&mut self.state);
        let specials: [(&str, RegionId); 4] = [
            (HEADER_PASSAGE, RegionId::header()),
            (FOOTER_PASSAGE, RegionId::footer()),
            (BAR_PASSAGE, RegionId::bar()),
            (BAR_STOWED_PASSAGE, RegionId::bar_stowed()),
        ];
        while let Some((name, region)) = specials.get(next_special).cloned() {
            next_special += 1;
            if !self.story.has(name) {
                continue;
            }
            self.script
                .audio_control("audioScope", serde_json::json!([name]))
                .map_err(crate::ScriptError::into_host_error)?;
            let mut view_state: State = self.state.fork_view();
            let mut view_story: Story<'hir, 'source> = self.story.fork_view();
            let mut continuations = HostPendingExecutions::new();
            let params: Value = Value::Null;
            let identity: RuntimeExecutionIdentity = self.identity(SPECIAL_STORY_ID);
            let language: Option<Rc<I18nRuntimeLanguage>> = self.language.clone();
            let result = HostApi::render_special_mir(
                &mut continuations,
                &mut view_state,
                &mut view_story,
                self.bytecode,
                name,
                HostMirRequest {
                    params: &params,
                    identity,
                    limits: limits(),
                    language: language.as_deref(),
                },
                |_phase, _context, _state| Ok::<(), Diagnostic>(()),
                |invocation, state, requests, scopes| {
                    dispatch_macro(
                        self.script.as_ref(),
                        self.hir,
                        &mut self.interactions,
                        invocation,
                        state,
                        requests,
                        scopes,
                    )
                },
            )
            .map_err(|error| diagnostic(error.diagnostic.clone()))?;
            match result {
                HostDriveResult::Ready(rendered) => {
                    update.append_region(region, rendered.surface().clone())
                }
                HostDriveResult::Pending { execution } => {
                    let script_pending: &ScriptPending = &continuations
                        .get(execution)
                        .expect("Engine 已登记辅助区域暂停执行")
                        .runtime()
                        .suspension()
                        .handle;
                    let operation: PendingOperation = protocol_pending(script_pending);
                    self.pending = Some(Pending::Special(Box::new(SpecialExecution {
                        operation: script_pending.id(),
                        execution,
                        update,
                        next_special,
                        region,
                        state: view_state,
                        story: view_story,
                        continuations,
                    })));
                    return Ok(RuntimeUpdate::Pending { operation });
                }
            }
        }
        self.script
            .audio_control("audioScope", serde_json::json!(["passage", false]))
            .map_err(crate::ScriptError::into_host_error)?;
        let dto = encode_host_update(&update, self.story.can_back(), self.story.can_forward());
        self.presented = Some(Rc::new(update));
        Ok(RuntimeUpdate::Ready { update: dto })
    }

    pub(super) fn resume_special(
        &mut self,
        mut waiting: SpecialExecution<'hir, 'source>,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let resumed = HostApi::resume_pending(
            &mut waiting.continuations,
            &mut waiting.state,
            &mut waiting.story,
            self.bytecode,
            waiting.execution,
            |handle, state, _requests, _scopes| resume_script(self.script.as_ref(), handle, state),
        )
        .map_err(diagnostic)?;
        let result = match resumed {
            HostResumeOutcome::Pending { execution } => HostDriveResult::Pending { execution },
            HostResumeOutcome::Continue(resumed) => {
                let stable = HostApi::continue_resumed(
                    *resumed,
                    &mut waiting.state,
                    &mut waiting.story,
                    self.bytecode,
                )
                .map_err(diagnostic)?;
                HostApi::drive_stable(
                    stable,
                    &mut waiting.continuations,
                    &mut waiting.state,
                    &mut waiting.story,
                    self.bytecode,
                    |_phase, _context, _state| Ok::<(), Diagnostic>(()),
                    |invocation, state, requests, scopes| {
                        dispatch_macro(
                            self.script.as_ref(),
                            self.hir,
                            &mut self.interactions,
                            invocation,
                            state,
                            requests,
                            scopes,
                        )
                    },
                )
                .map_err(|error| diagnostic(error.diagnostic.clone()))?
            }
        };
        match result {
            HostDriveResult::Ready(rendered) => {
                waiting
                    .update
                    .append_region(waiting.region, rendered.surface().clone());
                self.finish_specials(waiting.update, waiting.next_special)
            }
            HostDriveResult::Pending { execution } => {
                let script_pending: &ScriptPending = &waiting
                    .continuations
                    .get(execution)
                    .expect("Engine 已登记辅助区域暂停执行")
                    .runtime()
                    .suspension()
                    .handle;
                let operation: PendingOperation = protocol_pending(script_pending);
                waiting.operation = script_pending.id();
                waiting.execution = execution;
                self.pending = Some(Pending::Special(Box::new(waiting)));
                Ok(RuntimeUpdate::Pending { operation })
            }
        }
    }
}

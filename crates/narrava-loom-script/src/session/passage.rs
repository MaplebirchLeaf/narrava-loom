//! Passage 启动、导航、历史重放与输入。

use super::*;

impl<'hir, 'source> RuntimeSession<'hir, 'source> {
    pub(super) fn navigate(&mut self, target: &str) -> Result<RuntimeUpdate, HostErrorDto> {
        let params = Value::Null;
        let identity = self.identity(STORY_ID);
        let language = self.language.clone();
        let result = HostApi::navigate_mir_with_reaction(
            &mut self.continuations,
            &mut self.state,
            &mut self.story,
            self.bytecode,
            target,
            HostMirRequest {
                params: &params,
                identity,
                limits: limits(),
                language: language.as_deref(),
            },
            |phase, context, _state| emit_passage_event(self.script.as_ref(), phase, context),
            |passage, state, requests| {
                crate::reaction_runtime::apply_lifecycle_reactions(
                    self.script.as_ref(),
                    self.hir,
                    passage,
                    state,
                    requests,
                )
            },
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
        .map_err(|error| diagnostic(error.diagnostic.clone()));
        self.drive_main(result)
    }

    pub(super) fn start(&mut self) -> Result<RuntimeUpdate, HostErrorDto> {
        if self.presented.is_some() {
            return Err(HostErrorDto::new(
                "runtime_session.already_started",
                "当前 RuntimeSession 已经启动",
            ));
        }
        narrava_loom_core::location::validate_passages(self.state.location(), self.hir)
            .map_err(diagnostic)?;
        let params: Value = Value::Null;
        let identity: RuntimeExecutionIdentity = self.identity(STORY_ID);
        let language: Option<Rc<I18nRuntimeLanguage>> = self.language.clone();
        let result = HostApi::start_mir_with_reaction(
            &mut self.continuations,
            &mut self.state,
            &mut self.story,
            self.bytecode,
            HostMirRequest {
                params: &params,
                identity,
                limits: limits(),
                language: language.as_deref(),
            },
            |_passage, _state, _requests, _limits| {
                Ok::<BodyExecution, Diagnostic>(BodyExecution::default())
            },
            |phase, context, _state| emit_passage_event(self.script.as_ref(), phase, context),
            |passage, state, requests| {
                crate::reaction_runtime::apply_lifecycle_reactions(
                    self.script.as_ref(),
                    self.hir,
                    passage,
                    state,
                    requests,
                )
            },
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
        .map_err(|error| diagnostic(error.diagnostic.clone()));
        self.drive_main(result)
    }

    pub(super) fn activate(&mut self, interaction: &str) -> Result<RuntimeUpdate, HostErrorDto> {
        let previous: Rc<HostUpdate> = self.presented.clone().ok_or_else(|| {
            HostErrorDto::new("runtime_session.not_started", "必须先启动 Runtime")
        })?;
        let id: InteractionId = InteractionId::parse(interaction)
            .map_err(|error| HostErrorDto::new("runtime_session.interaction", error.to_string()))?;
        if self
            .interactions
            .get(&id)
            .is_some_and(|action| action.target().is_none())
        {
            return self.activate_action(&id);
        }
        let params: Value = Value::Null;
        let identity: RuntimeExecutionIdentity = self.identity(STORY_ID);
        let language: Option<Rc<I18nRuntimeLanguage>> = self.language.clone();
        let request = HostMirAdvanceRequest {
            presented: previous.as_ref(),
            input: HostInput::activate(id.clone()),
            params: &params,
            identity,
            limits: limits(),
            language: language.as_deref(),
        };
        let result = if self.interactions.has(&id) {
            let mut next_interactions: MacroInteractions<'hir, 'source> = MacroInteractions::new();
            let result = HostApi::advance_macro_interaction_mir_with_reaction(
                &mut self.continuations,
                &mut self.interactions,
                &mut self.state,
                &mut self.story,
                self.bytecode,
                request,
                |body, state, requests, scopes| {
                    let mut context = MacroLogicContext::new(state, requests, scopes);
                    let control = execute_logic_body(body, &mut context).map_err(|error| {
                        Diagnostic::new(
                            "runtime_session.interaction_body",
                            DiagnosticSeverity::Error,
                            &format!("Interaction 正文执行失败：{error:?}"),
                        )
                    })?;
                    Ok::<BodyExecution, Diagnostic>(BodyExecution {
                        control,
                        output: SemanticOutput::default(),
                    })
                },
                |phase, context, _state| emit_passage_event(self.script.as_ref(), phase, context),
                |passage, state, requests| {
                    crate::reaction_runtime::apply_lifecycle_reactions(
                        self.script.as_ref(),
                        self.hir,
                        passage,
                        state,
                        requests,
                    )
                },
                |invocation, state, requests, scopes| {
                    dispatch_macro(
                        self.script.as_ref(),
                        self.hir,
                        &mut next_interactions,
                        invocation,
                        state,
                        requests,
                        scopes,
                    )
                },
            );
            if result.is_ok() {
                self.interactions = next_interactions;
            }
            result
        } else {
            HostApi::advance_mir_with_reaction(
                &mut self.continuations,
                &mut self.state,
                &mut self.story,
                self.bytecode,
                request,
                |phase, context, _state| emit_passage_event(self.script.as_ref(), phase, context),
                |passage, state, requests| {
                    crate::reaction_runtime::apply_lifecycle_reactions(
                        self.script.as_ref(),
                        self.hir,
                        passage,
                        state,
                        requests,
                    )
                },
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
        }
        .map_err(|error| diagnostic(error.diagnostic.clone()));
        self.drive_main(result)
    }

    pub(super) fn history(&mut self, backward: bool) -> Result<RuntimeUpdate, HostErrorDto> {
        if self.presented.is_none() {
            return Err(HostErrorDto::new(
                "runtime_session.not_started",
                "必须先启动 Runtime",
            ));
        }
        if (backward && !self.story.can_back()) || (!backward && !self.story.can_forward()) {
            return Err(HostErrorDto::new(
                "runtime_session.history_unavailable",
                "Story 历史中没有该方向的条目",
            ));
        }
        self.replay(if backward {
            narrava_loom_core::host::HostReplayTarget::Previous
        } else {
            narrava_loom_core::host::HostReplayTarget::Next
        })
    }

    pub(super) fn replay(
        &mut self,
        target: narrava_loom_core::host::HostReplayTarget,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        if target == narrava_loom_core::host::HostReplayTarget::RefreshCurrent {
            self.script.set_refresh(Some(&self.state));
        }
        let params = Value::Null;
        let identity = self.identity(STORY_ID);
        let language = self.language.clone();
        let result = HostApi::replay_mir_with_reaction(
            &mut self.continuations,
            &mut self.state,
            &mut self.story,
            self.bytecode,
            target,
            HostMirRequest {
                params: &params,
                identity,
                limits: limits(),
                language: language.as_deref(),
            },
            |phase, context, _state| emit_passage_event(self.script.as_ref(), phase, context),
            |passage, state, requests| {
                crate::reaction_runtime::apply_lifecycle_reactions(
                    self.script.as_ref(),
                    self.hir,
                    passage,
                    state,
                    requests,
                )
            },
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
        .map_err(|error| diagnostic(error.diagnostic.clone()));
        self.drive_main(result)
    }

    pub(super) fn input(
        &mut self,
        interaction: &str,
        value: serde_json::Value,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let previous: &HostUpdate = self.presented.as_deref().ok_or_else(|| {
            HostErrorDto::new("runtime_session.not_started", "必须先启动 Runtime")
        })?;
        let id: InteractionId = InteractionId::parse(interaction).map_err(|error| {
            HostErrorDto::new("runtime_session.input_interaction", error.to_string())
        })?;
        let binding = previous.surface().input_binding(&id).ok_or_else(|| {
            HostErrorDto::new(
                "runtime_session.unknown_input",
                "输入身份未出现在上一份 Surface 中",
            )
        })?;
        let semantic: SemanticValue = json_to_semantic(&value)?;
        if !binding.accepts(&semantic) {
            return Err(HostErrorDto::new(
                "runtime_session.input_value",
                "输入值不属于当前控件允许的值集合",
            ));
        }
        let expression = narrava_loom_core::macro_runtime::parse_input_receiver(&binding.receiver)
            .map_err(diagnostic)?;
        let core_value: Value =
            json_to_value(&value).map_err(crate::ScriptError::into_host_error)?;
        if let Err(error) = assign_value_with_mut(&expression, core_value, &mut self.state) {
            return Err(HostErrorDto::new(
                "runtime_session.input_assignment",
                format!("{error:?}"),
            ));
        }
        Ok(RuntimeUpdate::Applied)
    }
}

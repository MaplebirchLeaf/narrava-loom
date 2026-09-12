//! 无导航 link 的独立 Bytecode 正文；沿用 Session 检查点和 Macro 暂停契约。
use super::*;
use narrava_loom_core::{
    bytecode::BytecodeMacroBody,
    engine::EngineMirMacroInvocation,
    macro_runtime::{
        MacroLocalScopes, MacroResumeOutcome, MacroSuspension, resume_macro_suspension,
    },
    mir::MirMacroBody,
    runtime::{BodyControl, RuntimeMacroExecution},
    semantic::SemanticKey,
    story::StoryRuntimeRequests,
    vm::{MirExecutionFrame, MirStep},
};

pub(super) struct ActionExecution {
    bytecode: BytecodeMacroBody,
    frame: MirExecutionFrame,
    scopes: MacroLocalScopes<Value>,
    identity: RuntimeExecutionIdentity,
}

pub(super) struct PendingAction {
    execution: ActionExecution,
    suspension: MacroSuspension<ScriptPending>,
}

impl PendingAction {
    pub(super) fn operation(&self) -> u64 {
        self.suspension.handle.id()
    }
}

fn failure(error: impl std::fmt::Debug) -> HostErrorDto {
    HostErrorDto::new("runtime_session.action_body", format!("{error:?}"))
}

impl<'hir, 'source> RuntimeSession<'hir, 'source> {
    pub(super) fn activate_action(
        &mut self,
        id: &InteractionId,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let presented: &HostUpdate = self.presented.as_deref().expect("activated after start");
        if !presented.surface().contains_interaction(id) {
            return Err(HostErrorDto::new(
                "runtime_session.unknown_interaction",
                "动作不在当前输出中",
            ));
        }
        // 保留入口，关闭后可再次打开；失败由 Session 统一恢复整张动作表。
        let action = self
            .interactions
            .get(id)
            .expect("registered action")
            .clone();
        let (_, body, captures) = action.into_parts();
        let mir: MirMacroBody<'hir, 'source> = MirMacroBody::lower(body).map_err(failure)?;
        let bytecode: BytecodeMacroBody = BytecodeMacroBody::compile(&mir);
        let frame: MirExecutionFrame = MirExecutionFrame::new_macro(&bytecode);
        let identity: RuntimeExecutionIdentity = self.identity(STORY_ID);
        self.drive_action(ActionExecution {
            bytecode,
            frame,
            scopes: captures.into_scopes(),
            identity,
        })
    }

    fn drive_action(
        &mut self,
        mut execution: ActionExecution,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        loop {
            let mut requests: StoryRuntimeRequests<'_, 'hir, 'source> =
                StoryRuntimeRequests::new(&self.story);
            let step: MirStep = {
                let mut context =
                    MacroLogicContext::new(&mut self.state, &mut requests, &mut execution.scopes);
                execution
                    .frame
                    .step_macro(&execution.bytecode, &mut context)
                    .map_err(failure)?
            };
            match step {
                MirStep::Running => {}
                MirStep::NavigationPending => return Err(failure("无导航 link 正文不能 goto")),
                MirStep::Halted => {
                    let mut output: SemanticOutput = SemanticOutput::default();
                    let content: SemanticOutput = execution.frame.into_output();
                    for (index, node) in content.nodes().iter().enumerate() {
                        let key: SemanticKey = content.key(index).cloned().unwrap_or_else(|| {
                            SemanticKey::parse(format!(
                                "action:{}:{index}",
                                execution.identity.chain
                            ))
                            .expect("nonempty key")
                        });
                        output.push_keyed(key, node.clone()).map_err(failure)?;
                    }
                    let mut update: HostUpdate = self
                        .presented
                        .as_deref()
                        .expect("action after start")
                        .clone();
                    update.apply_action_output(output);
                    self.interactions.retain_visible(update.surface());
                    let dto = encode_host_update(
                        &update,
                        self.story.can_back(),
                        self.story.can_forward(),
                    );
                    self.presented = Some(Rc::new(update));
                    return Ok(RuntimeUpdate::Ready { update: dto });
                }
                MirStep::MacroPending => {
                    let call = execution
                        .frame
                        .pending_macro_body(&execution.bytecode)
                        .expect("pending call");
                    let names: Vec<&str> = execution
                        .frame
                        .pending_macro_body_captures(&execution.bytecode)
                        .expect("pending captures");
                    let invocation = EngineMirMacroInvocation {
                        call,
                        identity: execution.identity,
                        location: execution.frame.location(),
                        captures: execution.scopes.capture(&names),
                    };
                    let outcome = dispatch_macro(
                        self.script.as_ref(),
                        self.hir,
                        &mut self.interactions,
                        invocation,
                        &mut self.state,
                        &mut requests,
                        std::mem::take(&mut execution.scopes),
                    )
                    .map_err(|error| failure(error.error))?;
                    if requests.pending_include_count() != 0 || requests.take_goto().is_some() {
                        return Err(failure("无导航 link 正文不能发起 Passage 请求"));
                    }
                    match outcome {
                        MacroResumeOutcome::Pending(suspension) => {
                            return Ok(self.suspend_action(execution, suspension));
                        }
                        MacroResumeOutcome::Complete { output, scopes } => {
                            complete(&mut execution, output, scopes)?;
                        }
                    }
                }
            }
        }
    }

    fn suspend_action(
        &mut self,
        execution: ActionExecution,
        suspension: MacroSuspension<ScriptPending>,
    ) -> RuntimeUpdate {
        let operation: PendingOperation = protocol_pending(&suspension.handle);
        self.pending = Some(Pending::Action(Box::new(PendingAction {
            execution,
            suspension,
        })));
        RuntimeUpdate::Pending { operation }
    }

    pub(super) fn resume_action(
        &mut self,
        waiting: PendingAction,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let PendingAction {
            mut execution,
            suspension,
        } = waiting;
        let outcome = resume_macro_suspension(execution.identity, suspension, |handle, _scopes| {
            resume_script(self.script.as_ref(), handle, &mut self.state)
        })
        .map_err(failure)?;
        match outcome {
            MacroResumeOutcome::Pending(suspension) => {
                Ok(self.suspend_action(execution, suspension))
            }
            MacroResumeOutcome::Complete { output, scopes } => {
                complete(&mut execution, output, scopes)?;
                self.drive_action(execution)
            }
        }
    }
}

fn complete(
    execution: &mut ActionExecution,
    output: RuntimeMacroExecution,
    scopes: MacroLocalScopes<Value>,
) -> Result<(), HostErrorDto> {
    if output.execution.control != BodyControl::Continue || output.includes_entered != 0 {
        return Err(failure("动作 Macro 返回了未支持的控制信号"));
    }
    execution
        .frame
        .complete_macro_body(&execution.bytecode, output.execution.output)
        .map_err(failure)?;
    execution.scopes = scopes;
    Ok(())
}

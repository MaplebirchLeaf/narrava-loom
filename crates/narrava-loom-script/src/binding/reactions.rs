//! 作者事件队列、Reaction 求值与脚本事件发布。

use crate::{
    EcmaBinding, EcmaRuntime, QueuedAuthorEvent, ScriptError, js_string, json_to_value,
    script_error, value_to_json,
};
use boa_engine::Source;
use narrava_loom_core::{
    expression::evaluator::ScriptCallError,
    expression::value::{ScriptCallable, Value},
    reaction::{
        ReactionEffect, ReactionResolveError, resolve_event_queue, resolve_lifecycle_reactions,
        resolve_state_changes,
    },
    state::{State, StateSnapshot},
};

impl EcmaBinding {
    /// 捕获不含脚本函数的 Reaction 运行状态，用于命令事务与 Save。
    pub fn reaction_state(&self) -> Vec<narrava_loom_core::reaction::ReactionRuntimeState> {
        self.runtime.borrow().reactions.borrow().runtime_state()
    }

    /// 将运行状态恢复到启动脚本已经注册的定义集合。
    pub fn restore_reaction_state(
        &self,
        state: &[narrava_loom_core::reaction::ReactionRuntimeState],
    ) -> Result<(), ScriptError> {
        self.runtime
            .borrow()
            .reactions
            .borrow_mut()
            .restore_runtime_state(state)
            .map_err(|error| ScriptError::new("script.reaction_restore", error.to_string()))
    }

    /// 取走尚未交给 Reaction Resolver 的作者 Event；内置生命周期 Event 不在此队列。
    pub fn drain_author_events(&self) -> Result<Vec<QueuedAuthorEvent>, ScriptError> {
        let mut runtime = self.runtime.borrow_mut();
        let result = runtime
            .context
            .eval(Source::from_bytes(
                "JSON.stringify(__narrava.takeAuthorEvents())",
            ))
            .map_err(|error| script_error("script.event_queue", error))?;
        let json: String = js_string(&result, &mut runtime.context)?;
        let events: Vec<serde_json::Value> = serde_json::from_str(&json)
            .map_err(|error| ScriptError::new("script.event_queue", error.to_string()))?;
        events
            .into_iter()
            .map(|event: serde_json::Value| {
                let name: String = event["name"]
                    .as_str()
                    .ok_or_else(|| ScriptError::new("script.event_queue", "Event 名称无效"))?
                    .to_owned();
                let payload: Value = json_to_value(&event["payload"])?;
                Ok(QueuedAuthorEvent { name, payload })
            })
            .collect()
    }

    /// 在 Runtime 安全点解析当前作者 Event 队列，不直接执行叙事或导航效果。
    pub fn resolve_queued_event_reactions(
        &self,
        passage: Option<&narrava_loom_core::hir::HirPassage<'_>>,
        state: &mut State,
    ) -> Result<Vec<ReactionEffect>, ScriptError> {
        let events: Vec<(String, Value)> = self
            .drain_author_events()?
            .into_iter()
            .map(|event: QueuedAuthorEvent| (event.name, event.payload))
            .collect();
        if events.is_empty() {
            return Ok(Vec::new());
        }
        let mut runtime = self.runtime.borrow_mut();
        let registry = runtime.reactions.clone();
        let mut resolved: Vec<ReactionEffect> = Vec::new();
        let _read_view = crate::reaction_adapter::begin_resolution(&runtime.context)
            .map_err(|error| script_error("script.reaction_resolve", error))?;
        resolve_event_queue(
            &mut registry.borrow_mut(),
            passage,
            events,
            256,
            |condition, emit_payload, payload, effect| {
                runtime.evaluate_reaction(
                    condition,
                    emit_payload,
                    vec![payload.clone()],
                    state,
                    effect,
                )
            },
            |_id, effect| {
                resolved.push(effect.clone());
                Ok(())
            },
        )
        .map_err(resolution_error)?;
        runtime.publish_reaction_events(&resolved)?;
        Ok(resolved)
    }

    /// 比较一次已提交命令前后的 `$` 状态，并解析由其继续产生的 Event Reaction。
    pub fn resolve_state_reactions(
        &self,
        passage: Option<&narrava_loom_core::hir::HirPassage<'_>>,
        before: &StateSnapshot,
        state: &mut State,
    ) -> Result<Vec<ReactionEffect>, ScriptError> {
        let after: StateSnapshot = state.snapshot();
        let mut runtime = self.runtime.borrow_mut();
        let registry = runtime.reactions.clone();
        let mut resolved: Vec<ReactionEffect> = Vec::new();
        let _read_view = crate::reaction_adapter::begin_resolution(&runtime.context)
            .map_err(|error| script_error("script.reaction_resolve", error))?;
        resolve_state_changes(
            &mut registry.borrow_mut(),
            passage,
            before,
            &after,
            256,
            |condition, emit_payload, payload, effect| {
                runtime.evaluate_reaction(
                    condition,
                    emit_payload,
                    vec![payload.clone()],
                    state,
                    effect,
                )
            },
            |_id, effect| {
                resolved.push(effect.clone());
                Ok(())
            },
        )
        .map_err(resolution_error)?;
        runtime.publish_reaction_events(&resolved)?;
        Ok(resolved)
    }

    /// 在普通 Passage 的 Reaction Phase 解析 lifecycle 规则。
    pub fn resolve_lifecycle_reactions(
        &self,
        passage: &narrava_loom_core::hir::HirPassage<'_>,
        state: &mut State,
    ) -> Result<Vec<ReactionEffect>, ScriptError> {
        let mut runtime = self.runtime.borrow_mut();
        let registry = runtime.reactions.clone();
        let mut resolved: Vec<ReactionEffect> = Vec::new();
        let _read_view = crate::reaction_adapter::begin_resolution(&runtime.context)
            .map_err(|error| script_error("script.reaction_resolve", error))?;
        resolve_lifecycle_reactions(
            &mut registry.borrow_mut(),
            passage,
            256,
            |condition, emit_payload, payload, effect| {
                runtime.evaluate_reaction(
                    condition,
                    emit_payload,
                    vec![payload.clone()],
                    state,
                    effect,
                )
            },
            |_id, effect| {
                resolved.push(effect.clone());
                Ok(())
            },
        )
        .map_err(resolution_error)?;
        runtime.publish_reaction_events(&resolved)?;
        Ok(resolved)
    }
}

impl EcmaRuntime {
    fn publish_reaction_events(&mut self, effects: &[ReactionEffect]) -> Result<(), ScriptError> {
        for effect in effects {
            if let Some(event) = &effect.emit {
                self.emit_reaction_event(&event.name, &event.payload)?;
            }
        }
        Ok(())
    }

    fn evaluate_reaction(
        &mut self,
        condition: Option<&ScriptCallable>,
        emit_payload: Option<&ScriptCallable>,
        arguments: Vec<Value>,
        state: &mut State,
        effect: &ReactionEffect,
    ) -> Result<Option<ReactionEffect>, ScriptError> {
        if let Some(condition) = condition {
            let accepted = self
                .call(condition, arguments.clone(), state)
                .map(|value: Value| value.is_truthy())
                .map_err(|error| {
                    callback_error(error, "script.reaction_condition", "Reaction cond 执行失败")
                })?;
            if !accepted {
                return Ok(None);
            }
        }

        let mut resolved: ReactionEffect = effect.clone();
        if let Some(emit_payload) = emit_payload {
            let payload = self.call(emit_payload, arguments, state).map_err(|error| {
                callback_error(
                    error,
                    "script.reaction_emit_payload",
                    "Reaction emit payload 执行失败",
                )
            })?;
            let Some(event) = resolved.emit.as_mut() else {
                return Err(ScriptError::new(
                    "script.reaction_emit_payload",
                    "Reaction 动态 emit payload 缺少事件定义",
                ));
            };
            event.payload = payload;
        }
        Ok(Some(resolved))
    }

    fn emit_reaction_event(&mut self, name: &str, payload: &Value) -> Result<(), ScriptError> {
        let payload = value_to_json(payload)
            .map_err(|()| ScriptError::new("script.reaction_event", "Reaction Event 无法序列化"))?;
        let expression = format!(
            "__narrava.emitReaction({}, {})",
            serde_json::to_string(name).expect("事件名必须可序列化"),
            payload
        );
        self.context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map(|_| ())
            .map_err(|error| script_error("script.reaction_event", error))
    }
}

fn callback_error(error: ScriptCallError, code: &str, message: &str) -> ScriptError {
    match error {
        ScriptCallError::Diagnostic(diagnostic) => {
            crate::protocol_adapter::diagnostic(*diagnostic).into()
        }
        ScriptCallError::Unavailable | ScriptCallError::Failed => ScriptError::new(code, message),
    }
}

fn resolution_error(error: ReactionResolveError<ScriptError>) -> ScriptError {
    match error {
        ReactionResolveError::Operation(error) => error,
        error => ScriptError::new("script.reaction_resolve", format!("{error:?}")),
    }
}

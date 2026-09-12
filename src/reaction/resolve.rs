//! Event、State 与生命周期 Reaction 的有界解析。

use super::*;
use crate::state::StateSnapshot;
use std::collections::{HashSet, VecDeque};

/// 在 Engine 安全点解析作者 Event；setter 与 `Event.emit` 都不得直接重入此流程。
pub fn resolve_event_queue<Callback, E>(
    registry: &mut ReactionRegistry<Callback>,
    passage: Option<&HirPassage<'_>>,
    events: impl IntoIterator<Item = (String, Value)>,
    execution_limit: usize,
    mut evaluate_reaction: impl FnMut(
        Option<&Callback>,
        Option<&Callback>,
        &Value,
        &ReactionEffect,
    ) -> Result<Option<ReactionEffect>, E>,
    mut execute_effect: impl FnMut(&ReactionId, &ReactionEffect) -> Result<(), E>,
) -> Result<ReactionResolution, ReactionResolveError<E>>
where
    Callback: Clone,
{
    struct QueuedEvent {
        name: String,
        payload: Value,
        lineage: HashSet<(String, ReactionId)>,
    }

    let mut queue: VecDeque<QueuedEvent> = events
        .into_iter()
        .map(|(name, payload): (String, Value)| QueuedEvent {
            name,
            payload,
            lineage: HashSet::new(),
        })
        .collect();
    let mut resolution: ReactionResolution = ReactionResolution::default();

    while let Some(event) = queue.pop_front() {
        for id in registry.event_candidates(&event.name, passage) {
            let pair: (String, ReactionId) = (event.name.clone(), id.clone());
            let Some(entry) = registry
                .get(id.as_str())
                .filter(|entry| entry.enabled())
                .cloned()
            else {
                continue;
            };
            if event.lineage.contains(&pair) {
                return Err(ReactionResolveError::EventCycle {
                    event: event.name,
                    reaction: id,
                });
            }
            if resolution.triggered.len() >= execution_limit {
                return Err(ReactionResolveError::ExecutionLimitExceeded {
                    limit: execution_limit,
                });
            }
            let Some(effect) = evaluate_reaction(
                entry.condition(),
                entry.emit_payload(),
                &event.payload,
                &entry.definition().effect,
            )
            .map_err(ReactionResolveError::Operation)?
            else {
                continue;
            };
            execute_effect(&id, &effect).map_err(ReactionResolveError::Operation)?;
            registry
                .record_success(id.as_str())
                .map_err(ReactionResolveError::Reaction)?;
            resolution.triggered.push(id.clone());

            if let Some(ReactionEvent { name, payload }) = effect.emit {
                let mut lineage: HashSet<(String, ReactionId)> = event.lineage.clone();
                lineage.insert(pair);
                queue.push_back(QueuedEvent {
                    name,
                    payload,
                    lineage,
                });
            }
        }
    }
    Ok(resolution)
}

/// 比较一次已提交命令前后的持久状态，并在同一安全点继续解析其 Event 链。
pub fn resolve_state_changes<Callback, E>(
    registry: &mut ReactionRegistry<Callback>,
    passage: Option<&HirPassage<'_>>,
    before: &StateSnapshot,
    after: &StateSnapshot,
    execution_limit: usize,
    mut evaluate_reaction: impl FnMut(
        Option<&Callback>,
        Option<&Callback>,
        &Value,
        &ReactionEffect,
    ) -> Result<Option<ReactionEffect>, E>,
    mut execute_effect: impl FnMut(&ReactionId, &ReactionEffect) -> Result<(), E>,
) -> Result<ReactionResolution, ReactionResolveError<E>>
where
    Callback: Clone,
{
    let paths: Vec<StatePath> = registry.state_paths().to_vec();
    let mut resolution: ReactionResolution = ReactionResolution::default();
    let mut emitted: Vec<(String, Value)> = Vec::new();

    for path in paths {
        let before_value: Value = before.variables_path(path.body());
        let after_value: Value = after.variables_path(path.body());
        if before_value == after_value {
            continue;
        }
        let argument: Value = Value::object(vec![
            (String::from("before"), before_value),
            (String::from("after"), after_value),
        ]);
        for id in registry.state_candidates(&path, passage) {
            if resolution.triggered.len() >= execution_limit {
                return Err(ReactionResolveError::ExecutionLimitExceeded {
                    limit: execution_limit,
                });
            }
            let Some(entry) = registry
                .get(id.as_str())
                .filter(|entry| entry.enabled())
                .cloned()
            else {
                continue;
            };
            let Some(effect) = evaluate_reaction(
                entry.condition(),
                entry.emit_payload(),
                &argument,
                &entry.definition().effect,
            )
            .map_err(ReactionResolveError::Operation)?
            else {
                continue;
            };
            execute_effect(&id, &effect).map_err(ReactionResolveError::Operation)?;
            registry
                .record_success(id.as_str())
                .map_err(ReactionResolveError::Reaction)?;
            resolution.triggered.push(id);
            if let Some(event) = effect.emit {
                emitted.push((event.name, event.payload));
            }
        }
    }

    let remaining: usize = execution_limit.saturating_sub(resolution.triggered.len());
    let event_resolution = resolve_event_queue(
        registry,
        passage,
        emitted,
        remaining,
        &mut evaluate_reaction,
        &mut execute_effect,
    )?;
    resolution.triggered.extend(event_resolution.triggered);
    Ok(resolution)
}

/// 在普通 Passage 的 Start 与正文之间解析 lifecycle Reaction 及其 Event 链。
pub fn resolve_lifecycle_reactions<Callback, E>(
    registry: &mut ReactionRegistry<Callback>,
    passage: &HirPassage<'_>,
    execution_limit: usize,
    mut evaluate_reaction: impl FnMut(
        Option<&Callback>,
        Option<&Callback>,
        &Value,
        &ReactionEffect,
    ) -> Result<Option<ReactionEffect>, E>,
    mut execute_effect: impl FnMut(&ReactionId, &ReactionEffect) -> Result<(), E>,
) -> Result<ReactionResolution, ReactionResolveError<E>>
where
    Callback: Clone,
{
    let mut resolution: ReactionResolution = ReactionResolution::default();
    let mut emitted: Vec<(String, Value)> = Vec::new();
    for id in registry.lifecycle_candidates(passage) {
        if resolution.triggered.len() >= execution_limit {
            return Err(ReactionResolveError::ExecutionLimitExceeded {
                limit: execution_limit,
            });
        }
        let Some(entry) = registry
            .get(id.as_str())
            .filter(|entry| entry.enabled())
            .cloned()
        else {
            continue;
        };
        let Some(effect) = evaluate_reaction(
            entry.condition(),
            entry.emit_payload(),
            &Value::Null,
            &entry.definition().effect,
        )
        .map_err(ReactionResolveError::Operation)?
        else {
            continue;
        };
        execute_effect(&id, &effect).map_err(ReactionResolveError::Operation)?;
        registry
            .record_success(id.as_str())
            .map_err(ReactionResolveError::Reaction)?;
        resolution.triggered.push(id);
        if let Some(event) = effect.emit {
            emitted.push((event.name, event.payload));
        }
    }

    let remaining: usize = execution_limit.saturating_sub(resolution.triggered.len());
    let event_resolution = resolve_event_queue(
        registry,
        Some(passage),
        emitted,
        remaining,
        &mut evaluate_reaction,
        &mut execute_effect,
    )?;
    resolution.triggered.extend(event_resolution.triggered);
    Ok(resolution)
}

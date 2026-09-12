//! Reaction 注册索引、触发次数与可存档状态。

use super::*;
use std::collections::HashSet;

impl<Callback> ReactionRegistry<Callback> {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            event_index: HashMap::new(),
            state_index: HashMap::new(),
            state_paths: Vec::new(),
            lifecycle_index: Vec::new(),
        }
    }

    pub fn add(
        &mut self,
        definition: ReactionDefinition,
        callbacks: impl Into<ReactionCallbacks<Callback>>,
    ) -> Result<(), ReactionError> {
        validate_definition(&definition)?;
        if self.entries.contains_key(&definition.id) {
            return Err(ReactionError::Duplicate(definition.id.as_str().to_owned()));
        }
        let id: ReactionId = definition.id.clone();
        match &definition.trigger {
            ReactionTrigger::Event(name) => self
                .event_index
                .entry(name.clone())
                .or_default()
                .push(id.clone()),
            ReactionTrigger::State(path) => {
                if !self.state_index.contains_key(path) {
                    self.state_paths.push(path.clone());
                }
                self.state_index
                    .entry(path.clone())
                    .or_default()
                    .push(id.clone());
            }
            ReactionTrigger::Lifecycle => self.lifecycle_index.push(id.clone()),
        }
        self.entries.insert(
            id,
            RegisteredReaction {
                enabled: definition.enabled,
                definition,
                callbacks: callbacks.into(),
                triggered: 0,
                destroyed: false,
            },
        );
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&RegisteredReaction<Callback>> {
        self.entries.get(id).filter(|entry| !entry.destroyed)
    }

    pub fn event_candidates(
        &self,
        name: &str,
        passage: Option<&HirPassage<'_>>,
    ) -> Vec<ReactionId> {
        self.enabled_candidates(
            self.event_index
                .get(name)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            passage,
        )
    }

    pub fn state_candidates(
        &self,
        path: &StatePath,
        passage: Option<&HirPassage<'_>>,
    ) -> Vec<ReactionId> {
        self.enabled_candidates(
            self.state_index
                .get(path)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            passage,
        )
    }

    pub fn state_paths(&self) -> &[StatePath] {
        &self.state_paths
    }

    pub fn lifecycle_candidates(&self, passage: &HirPassage<'_>) -> Vec<ReactionId> {
        self.enabled_candidates(&self.lifecycle_index, Some(passage))
    }

    pub fn enable(&mut self, id: &str) -> Result<bool, ReactionError> {
        let entry: &mut RegisteredReaction<Callback> = self.entry_mut(id)?;
        if entry
            .definition
            .limit
            .is_some_and(|limit: u64| entry.triggered >= limit)
        {
            return Ok(false);
        }
        let changed: bool = !entry.enabled;
        entry.enabled = true;
        Ok(changed)
    }

    pub fn disable(&mut self, id: &str) -> Result<bool, ReactionError> {
        let entry: &mut RegisteredReaction<Callback> = self.entry_mut(id)?;
        let changed: bool = entry.enabled;
        entry.enabled = false;
        Ok(changed)
    }

    pub fn reset(&mut self, id: &str) -> Result<bool, ReactionError> {
        let entry: &mut RegisteredReaction<Callback> = self.entry_mut(id)?;
        let changed: bool = entry.triggered != 0 || entry.enabled != entry.definition.enabled;
        entry.triggered = 0;
        entry.enabled = entry.definition.enabled;
        Ok(changed)
    }

    pub fn record_success(&mut self, id: &str) -> Result<ReactionSuccess, ReactionError> {
        let entry: &mut RegisteredReaction<Callback> = self
            .entries
            .get_mut(id)
            .ok_or_else(|| ReactionError::Missing(id.to_owned()))?;
        entry.triggered = entry
            .triggered
            .checked_add(1)
            .ok_or_else(|| ReactionError::TriggerCountExhausted(id.to_owned()))?;
        if entry.definition.once {
            entry.enabled = false;
            entry.destroyed = true;
            return Ok(ReactionSuccess::Destroyed);
        }
        if entry
            .definition
            .limit
            .is_some_and(|limit: u64| entry.triggered >= limit)
        {
            entry.enabled = false;
            return Ok(ReactionSuccess::Disabled);
        }
        Ok(ReactionSuccess::Active)
    }

    fn enabled_candidates(
        &self,
        ids: &[ReactionId],
        passage: Option<&HirPassage<'_>>,
    ) -> Vec<ReactionId> {
        ids.iter()
            .filter(|id: &&ReactionId| {
                self.entries.get(*id).is_some_and(|entry| {
                    !entry.destroyed
                        && entry.enabled()
                        && entry.definition.passage.as_ref().is_none_or(|selector| {
                            passage.is_some_and(|passage| selector.matches(passage))
                        })
                })
            })
            .cloned()
            .collect()
    }

    fn entry_mut(&mut self, id: &str) -> Result<&mut RegisteredReaction<Callback>, ReactionError> {
        self.entries
            .get_mut(id)
            .filter(|entry| !entry.destroyed)
            .ok_or_else(|| ReactionError::Missing(id.to_owned()))
    }

    /// 捕获全部规则的可存档运行状态，按 ID 排序以保持确定输出。
    pub fn runtime_state(&self) -> Vec<ReactionRuntimeState> {
        let mut state: Vec<ReactionRuntimeState> = self
            .entries
            .iter()
            .map(|(id, entry)| ReactionRuntimeState {
                id: id.as_str().to_owned(),
                enabled: entry.enabled,
                triggered: entry.triggered,
                destroyed: entry.destroyed,
            })
            .collect();
        state.sort_by(|left, right| left.id.cmp(&right.id));
        state
    }

    /// 原子恢复已注册规则的运行状态；未知或重复 ID 会拒绝整次恢复。
    pub fn restore_runtime_state(
        &mut self,
        state: &[ReactionRuntimeState],
    ) -> Result<(), ReactionError> {
        let mut seen: HashSet<&str> = HashSet::new();
        for item in state {
            if !seen.insert(item.id.as_str()) {
                return Err(ReactionError::InvalidRuntimeState(item.id.clone()));
            }
            let Some(entry) = self.entries.get(item.id.as_str()) else {
                return Err(ReactionError::Missing(item.id.clone()));
            };
            if (item.destroyed && (!entry.definition.once || item.enabled))
                || (entry.definition.once && item.triggered > 1)
                || entry.definition.limit.is_some_and(|limit| {
                    item.triggered > limit || (item.triggered == limit && item.enabled)
                })
            {
                return Err(ReactionError::InvalidRuntimeState(item.id.clone()));
            }
        }
        for item in state {
            let entry = self.entries.get_mut(item.id.as_str()).unwrap();
            entry.enabled = item.enabled;
            entry.triggered = item.triggered;
            entry.destroyed = item.destroyed;
        }
        Ok(())
    }
}

impl<Callback> Default for ReactionRegistry<Callback> {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_definition(definition: &ReactionDefinition) -> Result<(), ReactionError> {
    if let ReactionTrigger::Event(name) = &definition.trigger
        && (name.is_empty() || name.chars().any(char::is_whitespace))
    {
        return Err(ReactionError::InvalidEvent(name.clone()));
    }
    if definition.effect.exit && definition.trigger != ReactionTrigger::Lifecycle {
        return Err(ReactionError::ExitWithoutLifecycle);
    }
    if definition.effect.replace.is_some()
        && definition.effect.widget.is_none()
        && definition.effect.include.is_none()
    {
        return Err(ReactionError::ReplaceWithoutContent);
    }
    if definition.effect.widget.is_some() && definition.effect.include.is_some() {
        return Err(ReactionError::MultipleContentSources);
    }
    if definition.limit == Some(0) {
        return Err(ReactionError::InvalidLimit);
    }
    if definition.once && definition.limit.is_some() {
        return Err(ReactionError::OnceWithLimit);
    }
    let has_effect: bool = definition.effect.widget.is_some()
        || definition.effect.include.is_some()
        || definition.effect.goto.is_some()
        || definition.effect.emit.is_some()
        || definition.effect.exit;
    if !has_effect {
        return Err(ReactionError::MissingEffect);
    }
    Ok(())
}

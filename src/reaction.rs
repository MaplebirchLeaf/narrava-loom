//! 声明式叙事反应规则、索引与运行次数状态。

use std::{borrow::Borrow, collections::HashMap, error::Error, fmt};

use regex::Regex;

use crate::{expression::value::Value, hir::HirPassage};

/// 作者公开使用的稳定 Reaction ID。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReactionId(String);

impl ReactionId {
    pub fn parse(id: impl Into<String>) -> Result<Self, ReactionError> {
        let id: String = id.into();
        if id.is_empty() || id.chars().any(char::is_whitespace) {
            return Err(ReactionError::InvalidId(id));
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for ReactionId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

/// `$name.path` 形式的持久状态路径。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StatePath(String);

impl StatePath {
    pub fn parse(path: impl Into<String>) -> Result<Self, ReactionError> {
        let path: String = path.into();
        let Some(body) = path.strip_prefix('$') else {
            return Err(ReactionError::InvalidStatePath(path));
        };
        if body.is_empty()
            || body
                .split('.')
                .any(|part: &str| part.is_empty() || part.chars().any(char::is_whitespace))
        {
            return Err(ReactionError::InvalidStatePath(path));
        }
        Ok(Self(path))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 一条规则为什么进入候选集合；每条规则只有一个明确来源。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReactionTrigger {
    Event(String),
    State(StatePath),
    Lifecycle,
}

/// Passage 名称的精确或正则匹配。
#[derive(Clone, Debug)]
pub enum PassageMatcher {
    Exact(String),
    Regex { source: String, compiled: Regex },
}

impl PassageMatcher {
    pub fn exact(name: impl Into<String>) -> Result<Self, ReactionError> {
        let name: String = name.into();
        if name.is_empty() {
            return Err(ReactionError::InvalidPassageMatcher(name));
        }
        Ok(Self::Exact(name))
    }

    pub fn regex(source: impl Into<String>) -> Result<Self, ReactionError> {
        let source: String = source.into();
        let compiled: Regex = Regex::new(&source)
            .map_err(|_| ReactionError::InvalidPassageMatcher(source.clone()))?;
        Ok(Self::Regex { source, compiled })
    }

    fn matches(&self, name: &str) -> bool {
        match self {
            Self::Exact(expected) => expected == name,
            Self::Regex { compiled, .. } => compiled.is_match(name),
        }
    }
}

impl PartialEq for PassageMatcher {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Exact(left), Self::Exact(right)) => left == right,
            (Self::Regex { source: left, .. }, Self::Regex { source: right, .. }) => left == right,
            _ => false,
        }
    }
}

impl Eq for PassageMatcher {}

/// Passage Tag 的 any/all/none 静态条件。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PassageTagSelector {
    pub any: Vec<String>,
    pub all: Vec<String>,
    pub none: Vec<String>,
}

/// Lifecycle Reaction 的静态 Passage 选择器。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PassageSelector {
    pub matches: Vec<PassageMatcher>,
    pub excludes: Vec<PassageMatcher>,
    pub tags: PassageTagSelector,
}

impl PassageSelector {
    pub fn matches(&self, passage: &HirPassage<'_>) -> bool {
        let name_matches: bool = self.matches.is_empty()
            || self
                .matches
                .iter()
                .any(|matcher: &PassageMatcher| matcher.matches(passage.name));
        name_matches
            && !self
                .excludes
                .iter()
                .any(|matcher: &PassageMatcher| matcher.matches(passage.name))
            && (self.tags.any.is_empty()
                || self
                    .tags
                    .any
                    .iter()
                    .any(|tag: &String| passage.has_tag(tag)))
            && self
                .tags
                .all
                .iter()
                .all(|tag: &String| passage.has_tag(tag))
            && self
                .tags
                .none
                .iter()
                .all(|tag: &String| !passage.has_tag(tag))
    }
}

/// Reaction 成立后交给 Engine/Runtime 执行的结构化效果。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReactionEffect {
    pub widget: Option<String>,
    pub include: Option<String>,
    pub replace: Option<String>,
    pub goto: Option<String>,
    pub emit: Option<(String, Value)>,
    pub exit: bool,
}

/// 不包含脚本 callback 的拥有型规则定义。
#[derive(Clone, Debug, PartialEq)]
pub struct ReactionDefinition {
    pub id: ReactionId,
    pub trigger: ReactionTrigger,
    pub passage: Option<PassageSelector>,
    pub effect: ReactionEffect,
    pub enabled: bool,
    pub once: bool,
    pub limit: Option<u64>,
    pub tags: Vec<String>,
}

/// Definition、脚本条件句柄与可存档运行状态。
#[derive(Clone, Debug)]
pub struct RegisteredReaction<Condition> {
    definition: ReactionDefinition,
    condition: Option<Condition>,
    enabled: bool,
    triggered: u64,
}

impl<Condition> RegisteredReaction<Condition> {
    pub fn definition(&self) -> &ReactionDefinition {
        &self.definition
    }

    pub fn condition(&self) -> Option<&Condition> {
        self.condition.as_ref()
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn triggered(&self) -> u64 {
        self.triggered
    }
}

/// 成功触发后 Registry 对规则采取的生命周期动作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactionSuccess {
    Active,
    Disabled,
    Destroyed,
}

/// 按触发来源建立索引的 Reaction Registry。
#[derive(Clone, Debug)]
pub struct ReactionRegistry<Condition> {
    entries: HashMap<ReactionId, RegisteredReaction<Condition>>,
    event_index: HashMap<String, Vec<ReactionId>>,
    state_index: HashMap<StatePath, Vec<ReactionId>>,
    lifecycle_index: Vec<ReactionId>,
}

impl<Condition> ReactionRegistry<Condition> {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            event_index: HashMap::new(),
            state_index: HashMap::new(),
            lifecycle_index: Vec::new(),
        }
    }

    pub fn add(
        &mut self,
        definition: ReactionDefinition,
        condition: Option<Condition>,
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
            ReactionTrigger::State(path) => self
                .state_index
                .entry(path.clone())
                .or_default()
                .push(id.clone()),
            ReactionTrigger::Lifecycle => self.lifecycle_index.push(id.clone()),
        }
        self.entries.insert(
            id,
            RegisteredReaction {
                enabled: definition.enabled,
                definition,
                condition,
                triggered: 0,
            },
        );
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&RegisteredReaction<Condition>> {
        self.entries.get(id)
    }

    pub fn event_candidates(&self, name: &str) -> Vec<ReactionId> {
        self.enabled_candidates(
            self.event_index
                .get(name)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        )
    }

    pub fn state_candidates(&self, path: &StatePath) -> Vec<ReactionId> {
        self.enabled_candidates(
            self.state_index
                .get(path)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        )
    }

    pub fn lifecycle_candidates(&self, passage: &HirPassage<'_>) -> Vec<ReactionId> {
        self.lifecycle_index
            .iter()
            .filter(|id: &&ReactionId| {
                self.entries
                    .get(*id)
                    .is_some_and(|entry: &RegisteredReaction<Condition>| {
                        entry.enabled
                            && entry
                                .definition
                                .passage
                                .as_ref()
                                .is_none_or(|selector: &PassageSelector| selector.matches(passage))
                    })
            })
            .cloned()
            .collect()
    }

    pub fn enable(&mut self, id: &str) -> Result<bool, ReactionError> {
        let entry: &mut RegisteredReaction<Condition> = self.entry_mut(id)?;
        let changed: bool = !entry.enabled;
        entry.enabled = true;
        Ok(changed)
    }

    pub fn disable(&mut self, id: &str) -> Result<bool, ReactionError> {
        let entry: &mut RegisteredReaction<Condition> = self.entry_mut(id)?;
        let changed: bool = entry.enabled;
        entry.enabled = false;
        Ok(changed)
    }

    pub fn reset(&mut self, id: &str) -> Result<bool, ReactionError> {
        let entry: &mut RegisteredReaction<Condition> = self.entry_mut(id)?;
        let changed: bool = entry.triggered != 0 || !entry.enabled;
        entry.triggered = 0;
        entry.enabled = entry.definition.enabled;
        Ok(changed)
    }

    pub fn record_success(&mut self, id: &str) -> Result<ReactionSuccess, ReactionError> {
        let key: ReactionId = self
            .entries
            .get_key_value(id)
            .map(|(key, _entry): (&ReactionId, &RegisteredReaction<Condition>)| key.clone())
            .ok_or_else(|| ReactionError::Missing(id.to_owned()))?;
        let entry: &mut RegisteredReaction<Condition> = self.entries.get_mut(&key).unwrap();
        entry.triggered = entry
            .triggered
            .checked_add(1)
            .ok_or_else(|| ReactionError::TriggerCountExhausted(id.to_owned()))?;
        if entry.definition.once {
            self.remove_indexed(&key);
            self.entries.remove(&key);
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

    fn enabled_candidates(&self, ids: &[ReactionId]) -> Vec<ReactionId> {
        ids.iter()
            .filter(|id: &&ReactionId| {
                self.entries
                    .get(*id)
                    .is_some_and(RegisteredReaction::enabled)
            })
            .cloned()
            .collect()
    }

    fn entry_mut(&mut self, id: &str) -> Result<&mut RegisteredReaction<Condition>, ReactionError> {
        self.entries
            .get_mut(id)
            .ok_or_else(|| ReactionError::Missing(id.to_owned()))
    }

    fn remove_indexed(&mut self, id: &ReactionId) {
        for ids in self.event_index.values_mut() {
            ids.retain(|candidate: &ReactionId| candidate != id);
        }
        for ids in self.state_index.values_mut() {
            ids.retain(|candidate: &ReactionId| candidate != id);
        }
        self.lifecycle_index
            .retain(|candidate: &ReactionId| candidate != id);
    }
}

impl<Condition> Default for ReactionRegistry<Condition> {
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
    if definition.passage.is_some() && definition.trigger != ReactionTrigger::Lifecycle {
        return Err(ReactionError::PassageWithoutLifecycle);
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReactionError {
    InvalidId(String),
    InvalidEvent(String),
    InvalidStatePath(String),
    InvalidPassageMatcher(String),
    Duplicate(String),
    Missing(String),
    MissingEffect,
    PassageWithoutLifecycle,
    ExitWithoutLifecycle,
    ReplaceWithoutContent,
    MultipleContentSources,
    InvalidLimit,
    OnceWithLimit,
    TriggerCountExhausted(String),
}

impl fmt::Display for ReactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Reaction 失败: {self:?}")
    }
}

impl Error for ReactionError {}

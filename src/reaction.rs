//! 声明式叙事反应规则、索引与运行次数状态。

use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet, VecDeque},
    error::Error,
    fmt,
};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::{expression::value::Value, hir::HirPassage, state::StateSnapshot};

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

    fn body(&self) -> &str {
        self.0.strip_prefix('$').expect("StatePath 已在构造时验证")
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
    Regex {
        source: String,
        flags: String,
        compiled: Regex,
    },
}

impl PassageMatcher {
    pub fn exact(name: impl Into<String>) -> Result<Self, ReactionError> {
        let name: String = name.into();
        if name.is_empty() {
            return Err(ReactionError::InvalidPassageMatcher(name));
        }
        Ok(Self::Exact(name))
    }

    pub fn regex(
        source: impl Into<String>,
        flags: impl Into<String>,
    ) -> Result<Self, ReactionError> {
        let source: String = source.into();
        let flags: String = flags.into();
        let mut seen: HashSet<char> = HashSet::new();
        if flags
            .chars()
            .any(|flag: char| !matches!(flag, 'i' | 'm' | 's' | 'u') || !seen.insert(flag))
        {
            return Err(ReactionError::InvalidPassageFlags(flags));
        }
        let compiled: Regex = RegexBuilder::new(&source)
            .case_insensitive(flags.contains('i'))
            .multi_line(flags.contains('m'))
            .dot_matches_new_line(flags.contains('s'))
            .build()
            .map_err(|_| ReactionError::InvalidPassageMatcher(source.clone()))?;
        Ok(Self::Regex {
            source,
            flags,
            compiled,
        })
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
            (
                Self::Regex {
                    source: left_source,
                    flags: left_flags,
                    ..
                },
                Self::Regex {
                    source: right_source,
                    flags: right_flags,
                    ..
                },
            ) => left_source == right_source && left_flags == right_flags,
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

/// 依据当前 Passage 名称与 Tag 筛选 Reaction 候选。
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

/// Reaction 派生的拥有型事件。
#[derive(Clone, Debug, PartialEq)]
pub struct ReactionEvent {
    pub name: String,
    pub payload: Value,
}

/// Reaction 成立后交给 Engine/Runtime 执行的结构化效果。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReactionEffect {
    pub widget: Option<String>,
    pub include: Option<String>,
    pub replace: Option<String>,
    pub goto: Option<String>,
    pub emit: Option<ReactionEvent>,
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

/// 规则在脚本 Runtime 中持有的两类动态回调。
#[derive(Clone, Debug, Default)]
pub struct ReactionCallbacks<Callback> {
    pub condition: Option<Callback>,
    pub emit_payload: Option<Callback>,
}

impl<Callback> From<Option<Callback>> for ReactionCallbacks<Callback> {
    fn from(condition: Option<Callback>) -> Self {
        Self {
            condition,
            emit_payload: None,
        }
    }
}

/// Definition、脚本条件句柄与可存档运行状态。
#[derive(Clone, Debug)]
pub struct RegisteredReaction<Callback> {
    definition: ReactionDefinition,
    callbacks: ReactionCallbacks<Callback>,
    enabled: bool,
    triggered: u64,
    destroyed: bool,
}

/// Save 与事务边界使用的拥有型 Reaction 运行状态；不包含脚本回调或定义。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactionRuntimeState {
    pub id: String,
    pub enabled: bool,
    pub triggered: u64,
    pub destroyed: bool,
}

impl<Callback> RegisteredReaction<Callback> {
    pub fn definition(&self) -> &ReactionDefinition {
        &self.definition
    }

    pub fn condition(&self) -> Option<&Callback> {
        self.callbacks.condition.as_ref()
    }

    pub fn emit_payload(&self) -> Option<&Callback> {
        self.callbacks.emit_payload.as_ref()
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

/// 一次 Event Reaction 安全队列执行的结果。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReactionResolution {
    pub triggered: Vec<ReactionId>,
}

/// Resolver 本身的边界错误；具体 callback/effect 错误保持原类型。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReactionResolveError<E> {
    Reaction(ReactionError),
    Operation(E),
    ExecutionLimitExceeded { limit: usize },
    EventCycle { event: String, reaction: ReactionId },
}

/// 按触发来源建立索引的 Reaction Registry。
#[derive(Clone, Debug)]
pub struct ReactionRegistry<Callback> {
    entries: HashMap<ReactionId, RegisteredReaction<Callback>>,
    event_index: HashMap<String, Vec<ReactionId>>,
    state_index: HashMap<StatePath, Vec<ReactionId>>,
    state_paths: Vec<StatePath>,
    lifecycle_index: Vec<ReactionId>,
}

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
        let key: ReactionId = self
            .entries
            .get_key_value(id)
            .map(|(key, _entry): (&ReactionId, &RegisteredReaction<Callback>)| key.clone())
            .ok_or_else(|| ReactionError::Missing(id.to_owned()))?;
        let entry: &mut RegisteredReaction<Callback> = self.entries.get_mut(&key).unwrap();
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReactionError {
    InvalidId(String),
    InvalidEvent(String),
    InvalidStatePath(String),
    InvalidPassageMatcher(String),
    InvalidPassageFlags(String),
    Duplicate(String),
    Missing(String),
    MissingEffect,
    ExitWithoutLifecycle,
    ReplaceWithoutContent,
    MultipleContentSources,
    InvalidLimit,
    OnceWithLimit,
    TriggerCountExhausted(String),
    InvalidRuntimeState(String),
}

impl fmt::Display for ReactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Reaction 失败: {self:?}")
    }
}

impl Error for ReactionError {}

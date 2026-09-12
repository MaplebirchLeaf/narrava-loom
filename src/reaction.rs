//! 声明式叙事反应规则、索引与运行次数状态。

use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

mod registry;
mod resolve;

pub use resolve::{resolve_event_queue, resolve_lifecycle_reactions, resolve_state_changes};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

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

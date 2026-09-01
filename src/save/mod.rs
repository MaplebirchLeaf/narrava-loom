//! 平台无关的存档文档、校验和 State／Story 恢复事务。

mod controller;
mod value;

pub use controller::{
    SaveCompletion, SaveController, SaveControllerError, SaveLifecycleCallbacks,
    SaveLifecycleController, SaveLifecycleSubscriptionError, SaveLifecycleSubscriptionId,
    SaveLifecycleSubscriptions, SaveOperation, SaveOutcome, SaveRequest, SaveRequestId,
};

use std::{collections::BTreeMap, error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    GameIdentity,
    diagnostic::{Diagnostic, DiagnosticSeverity},
    state::{State, StateSnapshot},
    story::{Story, StoryHistoryEntry},
};

use value::SaveValueGraph;

const SAVE_MAGIC: &[u8; 8] = b"NRSAVE\0\x02";

/// 一份不包含平台对象、脚本函数或临时执行状态的存档。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveDocument {
    game: SaveGame,
    state: SaveValueGraph,
    story: SaveStory,
    reactions: Vec<crate::reaction::ReactionRuntimeState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveGame {
    id: String,
    version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveStory {
    history: Vec<SaveStoryEntry>,
    position: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveStoryEntry {
    passage: String,
    had_navigation: bool,
    state: SaveValueGraph,
}

/// Save 捕获、JSON 边界或恢复阶段的稳定失败原因。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    UnsupportedValue { path: String },
    InvalidValueGraph { message: String },
    InvalidStory { message: String },
    MissingPassage { name: String },
    GameMismatch { expected: String, actual: String },
    Encode { message: String },
    Decode { message: String },
    Restore { message: String },
}

impl SaveDocument {
    /// 捕获 `$variables` 与完整 Story 时间线；其他运行环境由启动流程重建。
    pub fn capture(
        game: &GameIdentity,
        state: &State,
        story: &Story<'_, '_>,
    ) -> Result<Self, SaveError> {
        let graph: SaveValueGraph = SaveValueGraph::encode(state.persistent_variables())?;
        let history: Vec<SaveStoryEntry> = story
            .history()
            .iter()
            .map(|entry: &StoryHistoryEntry<'_, '_>| {
                let snapshot: &StateSnapshot =
                    story
                        .state_snapshot(entry.id())
                        .ok_or_else(|| SaveError::InvalidStory {
                            message: format!(
                                "历史项 {:?} 缺少进入 Passage 前的持久 State 快照",
                                entry.id()
                            ),
                        })?;
                Ok(SaveStoryEntry {
                    passage: entry.passage().name.to_owned(),
                    had_navigation: entry.had_navigation(),
                    state: SaveValueGraph::encode(snapshot.persistent_variables())?,
                })
            })
            .collect::<Result<Vec<SaveStoryEntry>, SaveError>>()?;
        Ok(Self {
            game: SaveGame {
                id: game.id().to_owned(),
                version: game.version().to_string(),
            },
            state: graph,
            story: SaveStory {
                history,
                position: story.position(),
            },
            reactions: Vec::new(),
        })
    }

    /// 附加由 Script Runtime 捕获的 Reaction 计数、启用与销毁状态。
    pub fn with_reactions(mut self, reactions: Vec<crate::reaction::ReactionRuntimeState>) -> Self {
        self.reactions = reactions;
        self
    }

    /// 读取存档中的 Reaction 运行状态；旧存档默认返回空集合。
    pub fn reactions(&self) -> &[crate::reaction::ReactionRuntimeState] {
        &self.reactions
    }

    /// 编码正式 `.nsave`：固定 magic/schema header 后接紧凑、确定性的字段协议。
    pub fn to_bytes(&self) -> Result<Vec<u8>, SaveError> {
        let payload: Vec<u8> = postcard::to_allocvec(self).map_err(|error| SaveError::Encode {
            message: error.to_string(),
        })?;
        let mut bytes: Vec<u8> = Vec::with_capacity(SAVE_MAGIC.len() + payload.len());
        bytes.extend_from_slice(SAVE_MAGIC);
        bytes.extend_from_slice(&payload);
        Ok(bytes)
    }

    /// 解码正式 `.nsave`；未知 magic/schema 在反序列化前即被拒绝。
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        let payload: &[u8] = bytes
            .strip_prefix(SAVE_MAGIC)
            .ok_or_else(|| SaveError::Decode {
                message: String::from("存档 magic 或 schema version 不受支持"),
            })?;
        postcard::from_bytes(payload).map_err(|error| SaveError::Decode {
            message: error.to_string(),
        })
    }

    /// 完整验证后恢复 State 与 Story；任一运行时失败都会回滚两个领域。
    pub fn restore<'hir, 'source>(
        &self,
        game: &GameIdentity,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
    ) -> Result<(), SaveError> {
        self.validate_game(game)?;
        self.validate_story(story)?;
        let variables: BTreeMap<String, crate::expression::value::Value> = self.state.decode()?;
        let history_states: Vec<StateSnapshot> = self
            .story
            .history
            .iter()
            .map(|entry: &SaveStoryEntry| entry.state.decode().map(StateSnapshot::from_variables))
            .collect::<Result<Vec<StateSnapshot>, SaveError>>()?;
        self.restore_story(story, history_states)?;
        state.restore(StateSnapshot::from_variables(variables));
        Ok(())
    }

    /// 校验存档的游戏标识（id 与版本）与当前启动环境一致。
    fn validate_game(&self, game: &GameIdentity) -> Result<(), SaveError> {
        let actual: String = format!("{}@{}", game.id(), game.version());
        let expected: String = format!("{}@{}", self.game.id, self.game.version);
        if self.game.id != game.id() || self.game.version != game.version().to_string() {
            return Err(SaveError::GameMismatch { expected, actual });
        }
        Ok(())
    }

    /// 校验 history 与 position 一致，且所有历史 Passage 当前仍存在。
    fn validate_story(&self, story: &Story<'_, '_>) -> Result<(), SaveError> {
        match (self.story.history.is_empty(), self.story.position) {
            (true, None) => {}
            (false, Some(position)) if position < self.story.history.len() => {}
            _ => {
                return Err(SaveError::InvalidStory {
                    message: String::from("Story history 与 position 不一致"),
                });
            }
        }
        for entry in &self.story.history {
            if entry.passage == crate::story::special::STORY_INIT_PASSAGE
                || !story.has(entry.passage.as_str())
            {
                return Err(SaveError::MissingPassage {
                    name: entry.passage.clone(),
                });
            }
        }
        Ok(())
    }

    /// 用存档历史重建 Story 时间线，并把游标移回存档时的位置。
    fn restore_story(
        &self,
        story: &mut Story<'_, '_>,
        history_states: Vec<StateSnapshot>,
    ) -> Result<(), SaveError> {
        story
            .replace_timeline(
                self.story
                    .history
                    .iter()
                    .map(|entry: &SaveStoryEntry| (entry.passage.as_str(), entry.had_navigation)),
                self.story.position,
            )
            .map_err(|error| SaveError::Restore {
                message: error.to_string(),
            })?;
        let history_ids: Vec<_> = story.history().iter().map(StoryHistoryEntry::id).collect();
        for (id, state) in history_ids.into_iter().zip(history_states) {
            story.record_state_snapshot(id, state);
        }
        Ok(())
    }
}

impl SaveError {
    /// 转换为 Host、Logger 与调试器共用的稳定 Diagnostic。
    pub fn diagnostic(&self) -> Diagnostic {
        let code: &str = match self {
            Self::UnsupportedValue { .. } => "save.unsupported_value",
            Self::InvalidValueGraph { .. } => "save.invalid_value_graph",
            Self::InvalidStory { .. } => "save.invalid_story",
            Self::MissingPassage { .. } => "save.missing_passage",
            Self::GameMismatch { .. } => "save.game_mismatch",
            Self::Encode { .. } => "save.encode",
            Self::Decode { .. } => "save.decode",
            Self::Restore { .. } => "save.restore",
        };
        Diagnostic::new(code, DiagnosticSeverity::Error, self.to_string().as_str())
    }
}

impl fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedValue { path } => write!(formatter, "值不可保存：{path}"),
            Self::InvalidValueGraph { message } => {
                write!(formatter, "存档 Value 图无效：{message}")
            }
            Self::InvalidStory { message } => write!(formatter, "存档 Story 无效：{message}"),
            Self::MissingPassage { name } => write!(formatter, "存档 Passage 不存在：{name}"),
            Self::GameMismatch { expected, actual } => {
                write!(formatter, "存档属于 {expected}，当前游戏是 {actual}")
            }
            Self::Encode { message } => write!(formatter, "存档二进制编码失败：{message}"),
            Self::Decode { message } => write!(formatter, "存档二进制解码失败：{message}"),
            Self::Restore { message } => write!(formatter, "存档恢复失败：{message}"),
        }
    }
}

impl Error for SaveError {}

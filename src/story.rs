//! Story 的 Passage 查询与导航状态。

use std::{collections::VecDeque, error::Error, fmt};

use crate::{
    hir::{HirPassage, HirStory},
    macro_runtime::MacroStoryAccess,
};

mod runtime_requests;

pub use runtime_requests::*;

pub mod special;

/// Story 拒绝导航时保留稳定、可供上层转换的原因。
#[derive(Debug, PartialEq, Eq)]
pub enum StoryNavigationError {
    MissingPassage(String),
    SpecialPassage(String),
    DifferentStoryRequest,
    HistoryIdExhausted,
}

impl fmt::Display for StoryNavigationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPassage(name) => write!(formatter, "Passage 不存在：{name}"),
            Self::SpecialPassage(name) => write!(formatter, "特殊 Passage 不能用于导航：{name}"),
            Self::DifferentStoryRequest => formatter.write_str("导航请求属于另一份编译结果"),
            Self::HistoryIdExhausted => formatter.write_str("Story 历史编号已耗尽"),
        }
    }
}

impl Error for StoryNavigationError {}

/// Story 历史游标无法移动时的稳定原因。
#[derive(Debug, PartialEq, Eq)]
pub enum StoryHistoryError {
    NoPrevious,
    NoNext,
}

impl fmt::Display for StoryHistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPrevious => formatter.write_str("没有可回退的 Passage"),
            Self::NoNext => formatter.write_str("没有可前进的 Passage"),
        }
    }
}

impl Error for StoryHistoryError {}

/// Story 快照不能用于另一份编译结果。
#[derive(Debug, PartialEq, Eq)]
pub enum StorySnapshotError {
    DifferentStory,
}

impl fmt::Display for StorySnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DifferentStory => formatter.write_str("Story 快照属于另一份编译结果"),
        }
    }
}

impl Error for StorySnapshotError {}

/// 借用当前有效 HIR，并单独保存已经确认的导航状态。
pub struct Story<'hir, 'source> {
    compiled: &'hir HirStory<'source>,
    history: Vec<StoryHistoryEntry<'hir, 'source>>,
    position: Option<usize>,
    next_history_id: u64,
}

impl<'hir, 'source> Story<'hir, 'source> {
    /// 装载可用 Passage；装载本身不代表已经进入起始 Passage。
    pub fn new(compiled: &'hir HirStory<'source>) -> Self {
        Self {
            compiled,
            history: Vec::new(),
            position: None,
            next_history_id: 1,
        }
    }

    /// PassageName 区分大小写，不提供回退查询。
    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn get(&self, name: &str) -> Option<&'hir HirPassage<'source>> {
        self.compiled.passage(name)
    }

    /// 按精确 Tag 返回源码顺序中的 Passage，不为 Tag 预设游戏含义。
    pub fn tagged<'query>(
        &'query self,
        tag: &'query str,
    ) -> impl Iterator<Item = &'query HirPassage<'source>> + 'query {
        self.compiled
            .passages
            .iter()
            .filter(move |passage: &&HirPassage<'source>| passage.has_tag(tag))
    }

    /// 精确查询可选 StoryInit；查询本身不建立 current 或 history。
    pub fn story_init(&self) -> Option<&'hir HirPassage<'source>> {
        self.get(special::STORY_INIT_PASSAGE)
    }

    pub fn current(&self) -> Option<&HirPassage<'source>> {
        self.current_entry().map(StoryHistoryEntry::passage)
    }

    pub fn current_entry(&self) -> Option<&StoryHistoryEntry<'hir, 'source>> {
        self.position
            .and_then(|position: usize| self.history.get(position))
    }

    /// 历史只包含已经确认的导航，不包含查询或失败请求。
    pub fn history(&self) -> &[StoryHistoryEntry<'hir, 'source>] {
        &self.history
    }

    /// 清空当前导航时间线，但保留历史编号高水位以避免旧 ID 被复用。
    pub fn reset(&mut self) -> usize {
        let removed: usize = self.history.len();
        self.history.clear();
        self.position = None;
        removed
    }

    /// 只在当前有效时间线中查询稳定历史编号。
    pub fn history_get(&self, id: StoryHistoryId) -> Option<&StoryHistoryEntry<'hir, 'source>> {
        self.history
            .iter()
            .find(|entry: &&StoryHistoryEntry<'hir, 'source>| entry.id == id)
    }

    /// 捕获当前导航时间线；State 由自己的快照边界独立处理。
    pub fn snapshot(&self) -> StorySnapshot<'hir, 'source> {
        StorySnapshot {
            compiled: self.compiled,
            history: self.history.clone(),
            position: self.position,
            next_history_id: self.next_history_id,
        }
    }

    /// 为 Host 辅助区域复制当前时间线；副本导航不会进入真实历史。
    pub fn fork_view(&self) -> Self {
        let mut view: Self = Self::new(self.compiled);
        view.restore(self.snapshot())
            .expect("同一 Story 建立的视图必须接受自己的快照");
        view
    }

    /// 恢复同一编译 Story 的时间线，但不回退历史编号分配器。
    pub fn restore(
        &mut self,
        snapshot: StorySnapshot<'hir, 'source>,
    ) -> Result<(), StorySnapshotError> {
        if !std::ptr::eq(self.compiled, snapshot.compiled) {
            return Err(StorySnapshotError::DifferentStory);
        }
        self.history = snapshot.history;
        self.position = snapshot.position;
        self.next_history_id = self.next_history_id.max(snapshot.next_history_id);
        Ok(())
    }

    /// 当前 Passage 在完整导航时间线中的零基位置。
    pub fn position(&self) -> Option<usize> {
        self.position
    }

    /// 按区分大小写的 PassageName 统计已经确认的访问记录。
    pub fn visits(&self, name: &str) -> usize {
        let visits: usize = self
            .history
            .iter()
            .filter(|entry| entry.passage.name == name)
            .count();
        visits
    }

    /// 确认一次导航；只有存在的目标才会同时更新 current 与 history。
    pub fn goto(
        &mut self,
        name: &str,
    ) -> Result<&StoryHistoryEntry<'hir, 'source>, StoryNavigationError> {
        let request: StoryNavigationRequest<'hir, 'source> = self.request_goto(name)?;
        self.confirm_navigation(request)
    }

    /// 验证区分大小写的目标，但不修改导航时间线。
    pub fn request_goto(
        &self,
        name: &str,
    ) -> Result<StoryNavigationRequest<'hir, 'source>, StoryNavigationError> {
        let passage: &HirPassage<'source> = self
            .compiled
            .passage(name)
            .ok_or_else(|| StoryNavigationError::MissingPassage(name.to_owned()))?;
        if name == special::STORY_INIT_PASSAGE {
            return Err(StoryNavigationError::SpecialPassage(name.to_owned()));
        }
        Ok(StoryNavigationRequest {
            compiled: self.compiled,
            passage,
        })
    }

    /// 提交已经验证且属于当前编译 Story 的导航请求。
    pub fn confirm_navigation(
        &mut self,
        request: StoryNavigationRequest<'hir, 'source>,
    ) -> Result<&StoryHistoryEntry<'hir, 'source>, StoryNavigationError> {
        if !std::ptr::eq(self.compiled, request.compiled) {
            return Err(StoryNavigationError::DifferentStoryRequest);
        }
        let id: StoryHistoryId = StoryHistoryId(self.next_history_id);
        let next_history_id: u64 = self
            .next_history_id
            .checked_add(1)
            .ok_or(StoryNavigationError::HistoryIdExhausted)?;
        let retained: usize = self.position.map_or(0, |position: usize| position + 1);
        self.history.truncate(retained);
        self.history.push(StoryHistoryEntry {
            id,
            passage: request.passage,
            had_navigation: false,
        });
        self.position = Some(self.history.len() - 1);
        self.next_history_id = next_history_id;
        Ok(self.history.last().expect("成功导航后必须存在历史项"))
    }

    /// 记录当前位置 Passage 本次执行是否包含作者导航动作。
    ///
    /// 由 Engine 在拿到本跳输出后调用；该标记是安全返回目标选择的依据。
    pub fn record_navigation(&mut self, had_navigation: bool) {
        if let Some(position) = self.position
            && let Some(entry) = self.history.get_mut(position)
        {
            entry.had_navigation = had_navigation;
        }
    }

    /// 安全返回目标：当前位置之前最近一个已 Display、包含作者导航动作的普通 Passage。
    ///
    /// `[exit]` Passage 不进入安全返回目标集合。
    pub fn safe_return_target(&self) -> Option<&HirPassage<'source>> {
        let current: usize = self.position?;
        self.history[..current]
            .iter()
            .rev()
            .filter(|entry: &&StoryHistoryEntry<'_, '_>| {
                entry.had_navigation && !entry.passage.has_tag("exit")
            })
            .map(|entry: &StoryHistoryEntry<'_, '_>| entry.passage)
            .next()
    }

    /// 向历史中的前一个位置移动；保留游标之后的记录供后续前进。
    pub fn back(&mut self) -> Result<&StoryHistoryEntry<'hir, 'source>, StoryHistoryError> {
        let current: usize = self.position.ok_or(StoryHistoryError::NoPrevious)?;
        let previous: usize = current
            .checked_sub(1)
            .ok_or(StoryHistoryError::NoPrevious)?;
        self.position = Some(previous);
        Ok(&self.history[previous])
    }

    /// 向历史中的后一个位置移动，不追加新的访问记录。
    pub fn forward(&mut self) -> Result<&StoryHistoryEntry<'hir, 'source>, StoryHistoryError> {
        let current: usize = self.position.ok_or(StoryHistoryError::NoNext)?;
        let next: usize = current.checked_add(1).ok_or(StoryHistoryError::NoNext)?;
        let target: &StoryHistoryEntry<'hir, 'source> =
            self.history.get(next).ok_or(StoryHistoryError::NoNext)?;
        self.position = Some(next);
        Ok(target)
    }
}

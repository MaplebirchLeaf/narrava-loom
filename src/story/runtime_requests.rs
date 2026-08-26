//! Runtime 发起但尚未提交的 Story include 与 goto 请求。
//!
//! 请求先绑定编译结果并脱离 Story 的可变时间线；Engine 只有在事务成功时才确认
//! 导航。异步 continuation 因而可以安全转移请求所有权，并在恢复时验证 Story 身份。

use super::*;

/// Runtime Story Adapter 拒绝请求时的稳定原因。
#[derive(Debug, PartialEq, Eq)]
pub enum StoryRuntimeRequestError {
    Navigation(StoryNavigationError),
    GotoAlreadyPending,
}

impl fmt::Display for StoryRuntimeRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Navigation(error) => error.fmt(formatter),
            Self::GotoAlreadyPending => formatter.write_str("已有未消费的 goto 请求"),
        }
    }
}

impl Error for StoryRuntimeRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Navigation(error) => Some(error),
            Self::GotoAlreadyPending => None,
        }
    }
}

/// 一次 Story 导航记录的稳定身份，可供 Engine 关联其他领域的检查点。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StoryHistoryId(pub(super) u64);

/// Story 时间线中的单次已确认导航。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoryHistoryEntry<'hir, 'source> {
    pub(super) id: StoryHistoryId,
    pub(super) passage: &'hir HirPassage<'source>,
    /// 该 Passage 本次执行输出是否包含作者导航动作；由 Engine 执行后记录。
    pub(super) had_navigation: bool,
}

/// 已验证但尚未写入 Story history 的导航请求。
pub struct StoryNavigationRequest<'hir, 'source> {
    pub(super) compiled: &'hir HirStory<'source>,
    pub(super) passage: &'hir HirPassage<'source>,
}

/// 已验证、等待 Runtime 在当前位置执行的 include 请求。
pub struct StoryIncludeRequest<'hir, 'source> {
    passage: &'hir HirPassage<'source>,
}

/// Runtime 在节点边界消费 include 请求所需的窄接口。
pub trait RuntimeStoryAccess<'hir, 'source>: MacroStoryAccess {
    fn take_include_request(&mut self) -> Option<StoryIncludeRequest<'hir, 'source>>;
}

impl<'hir, 'source> StoryIncludeRequest<'hir, 'source> {
    pub fn passage(&self) -> &'hir HirPassage<'source> {
        self.passage
    }
}

impl<'hir, 'source> StoryNavigationRequest<'hir, 'source> {
    pub fn passage(&self) -> &'hir HirPassage<'source> {
        self.passage
    }
}

impl<'hir, 'source> StoryHistoryEntry<'hir, 'source> {
    pub fn id(&self) -> StoryHistoryId {
        self.id
    }

    pub fn passage(&self) -> &'hir HirPassage<'source> {
        self.passage
    }

    pub fn had_navigation(&self) -> bool {
        self.had_navigation
    }
}

/// 一次仅包含 Story 时间线的内存检查点。
pub struct StorySnapshot<'hir, 'source> {
    pub(super) compiled: &'hir HirStory<'source>,
    pub(super) history: Vec<StoryHistoryEntry<'hir, 'source>>,
    pub(super) position: Option<usize>,
    pub(super) next_history_id: u64,
}

/// Runtime 只提交 Story 请求，不直接修改导航时间线。
pub struct StoryRuntimeRequests<'story, 'hir, 'source> {
    story: &'story Story<'hir, 'source>,
    pending_includes: VecDeque<StoryIncludeRequest<'hir, 'source>>,
    pending_goto: Option<StoryNavigationRequest<'hir, 'source>>,
}

/// 异步等待期间脱离 Story 借用、但仍绑定同一编译结果的 Runtime 请求。
pub struct StoryRuntimePending<'hir, 'source> {
    compiled: &'hir HirStory<'source>,
    pending_includes: VecDeque<StoryIncludeRequest<'hir, 'source>>,
    pending_goto: Option<StoryNavigationRequest<'hir, 'source>>,
}

impl<'story, 'hir, 'source> StoryRuntimeRequests<'story, 'hir, 'source> {
    pub fn new(story: &'story Story<'hir, 'source>) -> Self {
        Self {
            story,
            pending_includes: VecDeque::new(),
            pending_goto: None,
        }
    }

    pub fn pending_include(&self) -> Option<&StoryIncludeRequest<'hir, 'source>> {
        self.pending_includes.front()
    }

    pub fn pending_include_count(&self) -> usize {
        self.pending_includes.len()
    }

    pub fn take_include(&mut self) -> Option<StoryIncludeRequest<'hir, 'source>> {
        self.pending_includes.pop_front()
    }

    pub fn pending_goto(&self) -> Option<&StoryNavigationRequest<'hir, 'source>> {
        self.pending_goto.as_ref()
    }

    pub fn take_goto(&mut self) -> Option<StoryNavigationRequest<'hir, 'source>> {
        self.pending_goto.take()
    }

    /// 结束当前借用并把尚未消费的请求交给 continuation。
    pub fn into_pending(self) -> StoryRuntimePending<'hir, 'source> {
        StoryRuntimePending {
            compiled: self.story.compiled,
            pending_includes: self.pending_includes,
            pending_goto: self.pending_goto,
        }
    }

    /// 将 continuation 保存的请求重新附着到同一 Story。
    pub fn from_pending(
        story: &'story Story<'hir, 'source>,
        pending: StoryRuntimePending<'hir, 'source>,
    ) -> Result<Self, StoryRuntimePendingError<'hir, 'source>> {
        if !std::ptr::eq(story.compiled, pending.compiled) {
            return Err(StoryRuntimePendingError { pending });
        }
        Ok(Self {
            story,
            pending_includes: pending.pending_includes,
            pending_goto: pending.pending_goto,
        })
    }
}

impl StoryRuntimePending<'_, '_> {
    pub fn pending_include_count(&self) -> usize {
        self.pending_includes.len()
    }

    pub fn has_goto(&self) -> bool {
        self.pending_goto.is_some()
    }
}

/// 请求不能附着到目标 Story；原请求所有权保持完整。
pub struct StoryRuntimePendingError<'hir, 'source> {
    pub pending: StoryRuntimePending<'hir, 'source>,
}

impl<'story, 'hir, 'source> MacroStoryAccess for StoryRuntimeRequests<'story, 'hir, 'source> {
    type Error = StoryRuntimeRequestError;

    fn has(&self, name: &str) -> bool {
        self.story.has(name)
    }

    fn include(&mut self, name: &str) -> Result<(), Self::Error> {
        let passage: &'hir HirPassage<'source> = self.story.get(name).ok_or_else(|| {
            StoryRuntimeRequestError::Navigation(StoryNavigationError::MissingPassage(
                name.to_owned(),
            ))
        })?;
        self.pending_includes
            .push_back(StoryIncludeRequest { passage });
        Ok(())
    }

    fn goto(&mut self, name: &str) -> Result<(), Self::Error> {
        if self.pending_goto.is_some() {
            return Err(StoryRuntimeRequestError::GotoAlreadyPending);
        }
        let request: StoryNavigationRequest<'_, '_> = self
            .story
            .request_goto(name)
            .map_err(StoryRuntimeRequestError::Navigation)?;
        self.pending_goto = Some(request);
        Ok(())
    }
}

impl<'story, 'hir, 'source> RuntimeStoryAccess<'hir, 'source>
    for StoryRuntimeRequests<'story, 'hir, 'source>
{
    fn take_include_request(&mut self) -> Option<StoryIncludeRequest<'hir, 'source>> {
        self.take_include()
    }
}

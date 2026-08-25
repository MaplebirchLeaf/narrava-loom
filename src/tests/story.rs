//! Story 控制器的查询与初始导航状态测试。

use std::path::Path;

use crate::{
    hir::{HirPassage, HirStory},
    macro_runtime::MacroStoryAccess,
    source::Source,
    story::{
        Story, StoryHistoryEntry, StoryHistoryError, StoryIncludeRequest, StoryNavigationError,
        StoryNavigationRequest, StoryRuntimeRequestError, StoryRuntimeRequests, StorySnapshot,
        StorySnapshotError,
    },
};

#[test]
fn special_passage_names_have_one_shared_definition() {
    use crate::story::special::{
        BAR_PASSAGE, BAR_STOWED_PASSAGE, FOOTER_PASSAGE, HEADER_PASSAGE, START_PASSAGE,
        STORY_INIT_PASSAGE, is_host_region, is_special,
    };

    assert_eq!(START_PASSAGE, "Start");
    assert_eq!(STORY_INIT_PASSAGE, "StoryInit");
    assert!(is_host_region(HEADER_PASSAGE));
    assert!(is_host_region(FOOTER_PASSAGE));
    assert!(is_host_region(BAR_PASSAGE));
    assert!(is_host_region(BAR_STOWED_PASSAGE));
    assert!(!is_host_region(START_PASSAGE));
    for name in [
        START_PASSAGE,
        STORY_INIT_PASSAGE,
        HEADER_PASSAGE,
        FOOTER_PASSAGE,
        BAR_PASSAGE,
        BAR_STOWED_PASSAGE,
    ] {
        assert!(is_special(name));
    }
    assert!(!is_special("Hall"));
}

// 大型测试集按用例边界拆成物理分片；门面保留共享导入与模块说明。
// include 保持原模块命名、私有辅助项可见性和测试发现路径不变。
include!("story/part_01.rs");
include!("story/part_02.rs");

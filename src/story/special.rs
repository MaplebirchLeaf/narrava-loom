//! Engine 识别的保留 Passage 名称。

/// 新游戏默认起始 Passage 名称。
pub const START_PASSAGE: &str = "Start";
/// 每次新游戏开始时执行的初始化 Passage 名称。
pub const STORY_INIT_PASSAGE: &str = "StoryInit";
/// 每屏顶部区域渲染的 Header Passage 名称。
pub const HEADER_PASSAGE: &str = "Header";
/// 每屏底部区域渲染的 Footer Passage 名称。
pub const FOOTER_PASSAGE: &str = "Footer";
/// 侧栏区域渲染的 Bar Passage 名称。
pub const BAR_PASSAGE: &str = "Bar";
/// 侧栏收起状态下渲染的 BarStowed Passage 名称。
pub const BAR_STOWED_PASSAGE: &str = "BarStowed";

/// 全部保留 Passage 名称，按固定顺序列出。
pub const SPECIAL_PASSAGES: [&str; 6] = [
    START_PASSAGE,
    STORY_INIT_PASSAGE,
    HEADER_PASSAGE,
    FOOTER_PASSAGE,
    BAR_PASSAGE,
    BAR_STOWED_PASSAGE,
];

/// 由 Host 直接渲染、不进入 Story 导航的保留 Passage 名称。
pub const HOST_REGION_PASSAGES: [&str; 4] = [
    HEADER_PASSAGE,
    FOOTER_PASSAGE,
    BAR_PASSAGE,
    BAR_STOWED_PASSAGE,
];

/// 返回名称是否属于 Host 区域 Passage（Header/Footer/Bar/BarStowed）。
pub fn is_host_region(name: &str) -> bool {
    HOST_REGION_PASSAGES.contains(&name)
}

/// 返回名称是否属于禁止携带 Tag 的保留 Passage。
pub fn is_special(name: &str) -> bool {
    SPECIAL_PASSAGES.contains(&name)
}

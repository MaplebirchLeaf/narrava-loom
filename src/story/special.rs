//! Engine 识别的保留 Passage 名称。

pub const START_PASSAGE: &str = "Start";
pub const STORY_INIT_PASSAGE: &str = "StoryInit";
pub const HEADER_PASSAGE: &str = "Header";
pub const FOOTER_PASSAGE: &str = "Footer";
pub const BAR_PASSAGE: &str = "Bar";
pub const BAR_STOWED_PASSAGE: &str = "BarStowed";

pub const HOST_REGION_PASSAGES: [&str; 4] = [
    HEADER_PASSAGE,
    FOOTER_PASSAGE,
    BAR_PASSAGE,
    BAR_STOWED_PASSAGE,
];

pub fn is_host_region(name: &str) -> bool {
    HOST_REGION_PASSAGES.contains(&name)
}

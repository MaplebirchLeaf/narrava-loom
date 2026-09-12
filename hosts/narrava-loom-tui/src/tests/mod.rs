//! `narrava-loom-tui` 的集中测试入口。
//!
//! 本地用例与共享 Host 用例统一由此挂载。

mod debug;
mod render;

#[path = "../../../tests/audio.rs"]
mod audio;

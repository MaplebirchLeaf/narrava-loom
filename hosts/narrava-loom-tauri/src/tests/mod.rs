//! `narrava-loom-tauri` 的集中测试入口。
//!
//! 本地用例与共享 Host 用例统一由此挂载。

mod assets;
mod release;
mod resource_protocol;
mod town_demo;
mod updates;

#[path = "../../../tests/audio.rs"]
mod audio;

#[path = "../../../tests/save_io.rs"]
mod save_io;

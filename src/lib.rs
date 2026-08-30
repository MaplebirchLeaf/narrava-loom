//! Narrava Loom 的宿主无关叙事 Core。
//!
//! 平台程序通过本 crate 驱动 Compiler、Runtime 与 Engine；具体画面、输入和
//! 平台对象由 Host 负责。命令行程序只是使用这套 library API 的一个 Rust Host。

pub mod bytecode;
mod config;
pub mod diagnostic;
pub mod engine;
pub mod events;
pub mod expression;
pub mod hir;
pub mod host;
pub mod i18n;
mod interpolation;
pub mod lir;
pub mod logger;
pub mod macro_runtime;
pub mod mir;
pub mod nar;
pub mod package_zip;
pub mod reaction;
pub mod release;
pub mod resource;
pub mod runtime;
pub mod save;
pub mod script;
pub mod semantic;
mod source;
pub mod state;
pub mod story;
pub mod twee;
pub mod vm;

pub use config::{
    ConfigError, GameCompatibility, GameCompatibilityError, GameConfig, GameIdentity,
    GameIdentityError, ProjectConfig,
};
pub use source::{Source, SourceError, SourceKind, SourceList, SourcePath};

#[cfg(test)]
mod tests;

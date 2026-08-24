//! Narrava Loom 的集中测试入口。
//!
//! 所有 `#[test]` 统一放在本目录或其子目录，并由这里集中挂载。
//! 业务源码不声明测试模块；`lib.rs` 只保留本入口的条件挂载。

mod bytecode;
mod config;
mod diagnostic;
mod engine;
mod events;
mod expression;
mod expression_evaluator;
mod expression_value;
mod hir;
mod host;
mod i18n;
mod interpolation;
mod lir;
mod logger;
mod macro_runtime;
mod mir;
mod nar;
mod presentation;
mod release;
mod resource;
mod runtime;
mod save;
mod script;
mod source;
mod state;
mod story;
mod twee;
mod vm;

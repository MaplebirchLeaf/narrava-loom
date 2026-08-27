//! `narrava-loom-core` 的可选模组加载器附属。
//!
//! 当前只固定独立 crate 与单向依赖边界：本 crate 同时依赖
//! `narrava-loom-protocol`（跨 Host 的 Surface 传输协议）与 `narrava-loom-core`
//! （语义与执行），依赖顺序固定为 `modloader → protocol → core`；
//! 模组加载实现不属于 Core 完成工作。

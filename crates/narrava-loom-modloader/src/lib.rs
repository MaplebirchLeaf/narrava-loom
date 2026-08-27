//! `narrava-loom-core` 的可选模组加载器附属。
//!
//! 当前只固定独立 crate 与单向依赖边界：本 crate 直接依赖
//! `narrava-loom-core` 的游戏身份、源码、资源与模组包契约，不消费表现输出协议；
//! 模组加载实现不属于 Core 完成工作。

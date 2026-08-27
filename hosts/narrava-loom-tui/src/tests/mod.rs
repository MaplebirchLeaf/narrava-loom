//! `narrava-loom-tui` 的集中测试入口。
//!
//! 遵循仓库源码规范：所有 `#[test]` 只放在本目录，业务源码不声明测试模块。

mod render;

/// TUI 完整 Host 必须消费发行包内已经校验的 Bytecode，并与 Tauri 一样投递生命周期事件、
/// 在每次剧情推进后刷新作者侧栏。这些断言保护跨 Host 行为，不绑定终端渲染细节。
#[test]
fn host_runtime_keeps_release_bytecode_events_and_sidebar_in_the_shared_update_path() {
    let source: &str = include_str!("../host.rs");

    assert!(source.contains("package.bytecode().clone()"));
    assert!(source.contains("emit_passage_event(script, phase, context)"));
    assert!(source.contains("finish_update("));
    assert_eq!(
        source.matches("append_sidebar(").count(),
        1,
        "侧栏只能从统一更新收尾调用，启动和推进不应各复制调用"
    );
}

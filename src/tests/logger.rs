//! 内存 Logger 行为测试。

use crate::diagnostic::{Diagnostic, DiagnosticSeverity};

use crate::logger::{
    LogEvent, LogFilter, LogLevel, LogRecord, LogSequence, LogSubscriptionId, Logger,
};

#[test]
fn assigns_monotonic_sequence_to_each_record() {
    let mut logger: Logger = Logger::new();

    logger.log(LogEvent::new(LogLevel::Info, "story", "第一条"));
    logger.log(LogEvent::new(LogLevel::Info, "story", "第二条"));

    let records: &[LogRecord] = logger.get();
    assert_eq!(records[0].sequence, LogSequence(1));
    assert_eq!(records[1].sequence, LogSequence(2));
    assert_eq!(records[0].event.message, "第一条");
}

#[test]
fn subscriptions_receive_only_matching_future_events() {
    let mut logger: Logger = Logger::new();
    logger.log(LogEvent::new(LogLevel::Error, "macro", "订阅前"));
    let subscription: LogSubscriptionId = logger.subscribe(LogFilter {
        minimum_level: Some(LogLevel::Warn),
        target: Some("macro".to_owned()),
    });

    logger.log(LogEvent::new(LogLevel::Info, "macro", "级别过低"));
    logger.log(LogEvent::new(LogLevel::Error, "story", "目标不同"));
    logger.log(LogEvent::new(LogLevel::Warn, "macro", "第一条"));
    logger.log(LogEvent::new(LogLevel::Error, "macro", "第二条"));

    let pending: Vec<LogRecord> = logger.take(subscription).expect("订阅应存在");
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].event.message, "第一条");
    assert_eq!(pending[1].event.message, "第二条");
    assert_eq!(pending[0].sequence, LogSequence(4));
    assert_eq!(pending[1].sequence, LogSequence(5));
    assert_eq!(logger.take(subscription), Some(Vec::new()));
}

#[test]
fn unsubscribe_removes_subscription_and_pending_events() {
    let mut logger: Logger = Logger::new();
    let subscription: LogSubscriptionId = logger.subscribe(LogFilter::default());
    logger.log(LogEvent::new(LogLevel::Info, "story", "待处理"));

    assert!(logger.unsubscribe(subscription));
    assert_eq!(logger.take(subscription), None);
    assert!(!logger.unsubscribe(subscription));
}

#[test]
fn clear_removes_history_and_pending_subscription_events() {
    let mut logger: Logger = Logger::new();
    let subscription: LogSubscriptionId = logger.subscribe(LogFilter::default());
    logger.log(LogEvent::new(LogLevel::Warn, "runtime", "待清理"));

    logger.clear();
    logger.log(LogEvent::new(LogLevel::Info, "runtime", "清理后"));

    assert_eq!(logger.get().len(), 1);
    assert_eq!(logger.get()[0].sequence, LogSequence(2));
    assert_eq!(
        logger.take(subscription).map(|records| records.len()),
        Some(1)
    );
}

#[test]
fn filters_by_minimum_level_and_exact_target() {
    let mut logger: Logger = Logger::new();
    logger.log(LogEvent::new(LogLevel::Debug, "macro", "开始执行"));
    logger.log(LogEvent::new(LogLevel::Warn, "story", "目标较慢"));
    logger.log(LogEvent::new(LogLevel::Error, "macro", "执行失败"));
    let filter: LogFilter = LogFilter {
        minimum_level: Some(LogLevel::Warn),
        target: Some("macro".to_owned()),
    };

    let events: Vec<&LogRecord> = logger.query(&filter);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event.message, "执行失败");
}

#[test]
fn empty_filter_returns_all_events_in_order() {
    let mut logger: Logger = Logger::new();
    logger.log(LogEvent::new(LogLevel::Trace, "vm", "第一条"));
    logger.log(LogEvent::new(LogLevel::Info, "story", "第二条"));

    let events: Vec<&LogRecord> = logger.query(&LogFilter::default());

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event.message, "第一条");
    assert_eq!(events[1].event.message, "第二条");
}

#[test]
fn records_structured_events_in_order() {
    let mut logger: Logger = Logger::new();

    logger.log(LogEvent::new(LogLevel::Info, "story", "进入 Start"));
    logger.log(LogEvent::new(LogLevel::Debug, "macro", "执行 weather"));

    let events: &[LogRecord] = logger.get();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event.target, "story");
    assert_eq!(events[1].event.level, LogLevel::Debug);
}

#[test]
fn keeps_optional_diagnostic_and_clears_events() {
    let mut logger: Logger = Logger::new();
    let diagnostic: Diagnostic = Diagnostic::new(
        "macro.rejected",
        DiagnosticSeverity::Error,
        "异步 Macro 被拒绝",
    );
    let event: LogEvent =
        LogEvent::new(LogLevel::Error, "macro", "Macro 执行失败").with_diagnostic(diagnostic);

    logger.log(event);
    assert_eq!(
        logger.get()[0]
            .event
            .diagnostic
            .as_ref()
            .map(|value| value.code.as_str()),
        Some("macro.rejected")
    );

    logger.clear();
    assert!(logger.get().is_empty());
}

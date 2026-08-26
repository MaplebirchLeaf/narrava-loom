//! 不依赖终端、浏览器或 Tauri 的结构化内存 Logger。

use std::collections::HashMap;

use crate::diagnostic::Diagnostic;

/// Logger 事件的详细程度与严重程度。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Logger 查询条件；未填写的字段不参与筛选。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogFilter {
    pub minimum_level: Option<LogLevel>,
    pub target: Option<String>,
}

impl LogFilter {
    /// 判断事件是否同时满足级别与目标条件。
    fn matches(&self, event: &LogEvent) -> bool {
        let level_matches: bool = self
            .minimum_level
            .is_none_or(|minimum: LogLevel| event.level >= minimum);
        let target_matches: bool = self
            .target
            .as_deref()
            .is_none_or(|target: &str| event.target == target);
        level_matches && target_matches
    }
}

/// Logger 为一次订阅分配的稳定身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LogSubscriptionId(u64);

/// Logger 分配的单调记录序号。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LogSequence(pub(crate) u64);

impl LogSequence {
    /// 返回序号值，供 Debug API 序列化。
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Logger 接收事件后生成的不可变记录。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogRecord {
    pub sequence: LogSequence,
    pub event: LogEvent,
}

/// 一条订阅：过滤条件与尚未取走的待处理记录。
#[derive(Debug)]
struct LogSubscription {
    filter: LogFilter,
    pending: Vec<LogRecord>,
}

/// 一条可供调试 API 查询或转发的日志事件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEvent {
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub diagnostic: Option<Diagnostic>,
}

impl LogEvent {
    /// 建立不携带 Diagnostic 的普通日志事件。
    pub fn new(level: LogLevel, target: &str, message: &str) -> Self {
        Self {
            level,
            target: target.to_owned(),
            message: message.to_owned(),
            diagnostic: None,
        }
    }

    /// 将已有 Diagnostic 附加到事件，而不改变其内容。
    pub fn with_diagnostic(mut self, diagnostic: Diagnostic) -> Self {
        self.diagnostic = Some(diagnostic);
        self
    }
}

/// 按写入顺序保存结构化事件的最小 Logger。
#[derive(Debug)]
pub struct Logger {
    records: Vec<LogRecord>,
    subscriptions: HashMap<LogSubscriptionId, LogSubscription>,
    next_subscription_id: u64,
    next_sequence: u64,
}

impl Logger {
    /// 建立空 Logger。
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            subscriptions: HashMap::new(),
            next_subscription_id: 0,
            next_sequence: 1,
        }
    }

    /// 记录一条已经构造完成的事件。
    pub fn log(&mut self, event: LogEvent) {
        let record: LogRecord = LogRecord {
            sequence: LogSequence(self.next_sequence),
            event,
        };
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .expect("Logger 记录序号已耗尽");
        for subscription in self.subscriptions.values_mut() {
            if subscription.filter.matches(&record.event) {
                subscription.pending.push(record.clone());
            }
        }
        self.records.push(record);
    }

    /// 订阅之后产生且符合条件的事件。
    pub fn subscribe(&mut self, filter: LogFilter) -> LogSubscriptionId {
        let id: LogSubscriptionId = LogSubscriptionId(self.next_subscription_id);
        self.next_subscription_id = self
            .next_subscription_id
            .checked_add(1)
            .expect("Logger 订阅 ID 已耗尽");
        self.subscriptions.insert(
            id,
            LogSubscription {
                filter,
                pending: Vec::new(),
            },
        );
        id
    }

    /// 取走当前待处理事件；未知订阅返回 `None`。
    pub fn take(&mut self, id: LogSubscriptionId) -> Option<Vec<LogRecord>> {
        let subscription: &mut LogSubscription = self.subscriptions.get_mut(&id)?;
        Some(std::mem::take(&mut subscription.pending))
    }

    /// 取消订阅，并释放尚未读取的事件。
    pub fn unsubscribe(&mut self, id: LogSubscriptionId) -> bool {
        self.subscriptions.remove(&id).is_some()
    }

    /// 按写入顺序读取当前全部事件。
    pub fn get(&self) -> &[LogRecord] {
        &self.records
    }

    /// 按最低级别和精确目标筛选，并保持原始写入顺序。
    pub fn query(&self, filter: &LogFilter) -> Vec<&LogRecord> {
        self.records
            .iter()
            .filter(|record: &&LogRecord| filter.matches(&record.event))
            .collect()
    }

    /// 清空历史和订阅待处理事件，但保留订阅本身。
    pub fn clear(&mut self) {
        self.records.clear();
        for subscription in self.subscriptions.values_mut() {
            subscription.pending.clear();
        }
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}

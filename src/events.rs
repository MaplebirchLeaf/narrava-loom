//! Host 无关的结构化运行时事实总线。

use std::{collections::HashMap, collections::HashSet, error::Error, fmt};

use crate::expression::value::Value;

/// 事件在总线中的单调序号。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventSequence(u64);

impl EventSequence {
    /// 原始序号值，供 Host 序列化。
    pub fn get(self) -> u64 {
        self.0
    }
}

/// 事件总线为一次订阅分配的稳定身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventSubscriptionId(u64);

/// 事件订阅的过滤条件；未填写的字段不参与筛选。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventFilter {
    name: Option<String>,
}

impl EventFilter {
    /// 按精确事件名过滤；名称为空时匹配全部事件。
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
        }
    }

    /// 过滤使用的事件名；`None` 表示不过滤名称。
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// 判断事件是否命中过滤条件。
    fn matches(&self, event: &EventRecord) -> bool {
        self.name.as_deref().is_none_or(|name| name == event.name)
    }
}

/// 总线中的一条不可变事件记录。
#[derive(Clone, Debug, PartialEq)]
pub struct EventRecord {
    sequence: EventSequence,
    name: String,
    payload: Value,
}

impl EventRecord {
    /// 事件序号。
    pub fn sequence(&self) -> EventSequence {
        self.sequence
    }

    /// 事件名。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 事件负载。
    pub fn payload(&self) -> &Value {
        &self.payload
    }
}

/// 一条订阅：过滤条件与尚未取走的待处理事件。
#[derive(Debug)]
struct EventSubscription {
    filter: EventFilter,
    pending: Vec<EventRecord>,
}

/// Host 无关的结构化运行时事实总线：事件按序号归档并分发到订阅。
#[derive(Debug)]
pub struct Event {
    records: Vec<EventRecord>,
    subscriptions: HashMap<EventSubscriptionId, EventSubscription>,
    next_subscription_id: u64,
    next_sequence: u64,
}

impl Event {
    /// 建立空总线。
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            subscriptions: HashMap::new(),
            next_subscription_id: 1,
            next_sequence: 1,
        }
    }

    /// 发布一条事件：归档到历史，并投递给匹配的订阅。
    pub fn emit(
        &mut self,
        name: impl Into<String>,
        payload: Value,
    ) -> Result<EventSequence, EventError> {
        let name = name.into();
        validate_name(&name)?;
        validate_payload(&payload)?;
        let sequence = EventSequence(self.next_sequence);
        let record = EventRecord {
            sequence,
            name,
            payload,
        };
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .expect("Event 序号空间不应耗尽");
        for subscription in self.subscriptions.values_mut() {
            if subscription.filter.matches(&record) {
                subscription.pending.push(record.clone());
            }
        }
        self.records.push(record);
        Ok(sequence)
    }

    /// 注册订阅；只投递注册之后发布且匹配的事件。
    pub fn subscribe(&mut self, filter: EventFilter) -> EventSubscriptionId {
        let id = EventSubscriptionId(self.next_subscription_id);
        self.next_subscription_id = self
            .next_subscription_id
            .checked_add(1)
            .expect("Event 订阅 ID 空间不应耗尽");
        self.subscriptions.insert(
            id,
            EventSubscription {
                filter,
                pending: Vec::new(),
            },
        );
        id
    }

    /// 取走订阅的待处理事件；未知订阅返回 `None`。
    pub fn take(&mut self, id: EventSubscriptionId) -> Option<Vec<EventRecord>> {
        let subscription = self.subscriptions.get_mut(&id)?;
        Some(std::mem::take(&mut subscription.pending))
    }

    /// 取消订阅，并释放尚未读取的事件；返回是否真的移除了订阅。
    pub fn unsubscribe(&mut self, id: EventSubscriptionId) -> bool {
        self.subscriptions.remove(&id).is_some()
    }

    /// 按发布顺序读取全部历史事件。
    pub fn records(&self) -> &[EventRecord] {
        &self.records
    }

    /// 清空历史与订阅待处理事件，但保留订阅本身。
    pub fn clear(&mut self) {
        self.records.clear();
        for subscription in self.subscriptions.values_mut() {
            subscription.pending.clear();
        }
    }
}

impl Default for Event {
    fn default() -> Self {
        Self::new()
    }
}

/// 事件名非法或负载包含不可序列化值。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventError {
    InvalidName(String),
    UnsupportedValue(String),
}

impl fmt::Display for EventError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Event 失败: {self:?}")
    }
}

impl Error for EventError {}

/// 事件名不能为空或含空白字符。
fn validate_name(name: &str) -> Result<(), EventError> {
    if name.is_empty() || name.chars().any(char::is_whitespace) {
        Err(EventError::InvalidName(name.to_owned()))
    } else {
        Ok(())
    }
}

/// 校验负载只包含可安全序列化的值。
fn validate_payload(payload: &Value) -> Result<(), EventError> {
    let mut visited = HashSet::new();
    validate_value(payload, "payload", &mut visited)
}

/// 递归校验值；`visited` 防止共享容器被重复遍历。
fn validate_value(
    value: &Value,
    path: &str,
    visited: &mut HashSet<(char, usize)>,
) -> Result<(), EventError> {
    match value {
        Value::Array(array) => {
            if !visited.insert(('a', array.identity())) {
                return Ok(());
            }
            for (index, value) in array.snapshot().iter().enumerate() {
                validate_value(value, &format!("{path}[{index}]"), visited)?;
            }
            Ok(())
        }
        Value::Object(object) => {
            if !visited.insert(('o', object.identity())) {
                return Ok(());
            }
            for (name, value) in object.snapshot() {
                validate_value(&value, &format!("{path}.{name}"), visited)?;
            }
            Ok(())
        }
        Value::Callable(_) | Value::ScriptCallable(_) | Value::Namespace(_) => {
            Err(EventError::UnsupportedValue(path.to_owned()))
        }
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::String(_) => Ok(()),
    }
}

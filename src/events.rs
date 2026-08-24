//! Host 无关的结构化运行时事实总线。

use std::{collections::HashMap, collections::HashSet, error::Error, fmt};

use crate::expression::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventSequence(u64);

impl EventSequence {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventSubscriptionId(u64);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventFilter {
    name: Option<String>,
}

impl EventFilter {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    fn matches(&self, event: &EventRecord) -> bool {
        self.name.as_deref().is_none_or(|name| name == event.name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventRecord {
    sequence: EventSequence,
    name: String,
    payload: Value,
}

impl EventRecord {
    pub fn sequence(&self) -> EventSequence {
        self.sequence
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }
}

#[derive(Debug)]
struct EventSubscription {
    filter: EventFilter,
    pending: Vec<EventRecord>,
}

#[derive(Debug)]
pub struct Event {
    records: Vec<EventRecord>,
    subscriptions: HashMap<EventSubscriptionId, EventSubscription>,
    next_subscription_id: u64,
    next_sequence: u64,
}

impl Event {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            subscriptions: HashMap::new(),
            next_subscription_id: 1,
            next_sequence: 1,
        }
    }

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

    pub fn take(&mut self, id: EventSubscriptionId) -> Option<Vec<EventRecord>> {
        let subscription = self.subscriptions.get_mut(&id)?;
        Some(std::mem::take(&mut subscription.pending))
    }

    pub fn unsubscribe(&mut self, id: EventSubscriptionId) -> bool {
        self.subscriptions.remove(&id).is_some()
    }

    pub fn records(&self) -> &[EventRecord] {
        &self.records
    }

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

fn validate_name(name: &str) -> Result<(), EventError> {
    if name.is_empty() || name.chars().any(char::is_whitespace) {
        Err(EventError::InvalidName(name.to_owned()))
    } else {
        Ok(())
    }
}

fn validate_payload(payload: &Value) -> Result<(), EventError> {
    let mut visited = HashSet::new();
    validate_value(payload, "payload", &mut visited)
}

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

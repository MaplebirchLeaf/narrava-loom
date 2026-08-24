//! Narrava Value 图与可序列化 Save 节点之间的转换。

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::expression::value::{ArrayValue, ObjectValue, TextValue, Value};

use super::SaveError;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SaveValueGraph {
    roots: BTreeMap<String, SaveValue>,
    nodes: BTreeMap<u64, SaveNode>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum SaveValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(u64),
    String(Vec<u16>),
    Array(u64),
    Object(u64),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum SaveNode {
    Array(Vec<SaveValue>),
    Object(Vec<(String, SaveValue)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RuntimeIdentity {
    Array(usize),
    Object(usize),
}

struct Encoder {
    next_id: u64,
    identities: HashMap<RuntimeIdentity, u64>,
    nodes: BTreeMap<u64, SaveNode>,
}

impl SaveValueGraph {
    pub(super) fn encode(roots: &BTreeMap<String, Value>) -> Result<Self, SaveError> {
        let mut encoder: Encoder = Encoder {
            next_id: 1,
            identities: HashMap::new(),
            nodes: BTreeMap::new(),
        };
        let mut encoded: BTreeMap<String, SaveValue> = BTreeMap::new();
        for (name, value) in roots {
            let path: String = format!("$variables.{name}");
            encoded.insert(name.clone(), encoder.encode_value(value, path.as_str())?);
        }
        Ok(Self {
            roots: encoded,
            nodes: encoder.nodes,
        })
    }

    pub(super) fn decode(&self) -> Result<BTreeMap<String, Value>, SaveError> {
        let mut runtime_nodes: BTreeMap<u64, Value> = BTreeMap::new();
        for (id, node) in &self.nodes {
            if *id == 0 {
                return Err(invalid("Value 节点 ID 不能为 0"));
            }
            let value: Value = match node {
                SaveNode::Array(_) => Value::array(Vec::new()),
                SaveNode::Object(_) => Value::object(Vec::new()),
            };
            runtime_nodes.insert(*id, value);
        }

        for (id, node) in &self.nodes {
            let target: Value = runtime_nodes
                .get(id)
                .cloned()
                .ok_or_else(|| invalid("Value 节点缺失"))?;
            match (target, node) {
                (Value::Array(array), SaveNode::Array(items)) => {
                    let values: Result<Vec<Value>, SaveError> = items
                        .iter()
                        .map(|value: &SaveValue| decode_value(value, &runtime_nodes))
                        .collect();
                    let values: Vec<Value> = values?;
                    array.with_mut(|target: &mut Vec<Value>| *target = values);
                }
                (Value::Object(object), SaveNode::Object(properties)) => {
                    let values: Result<Vec<(String, Value)>, SaveError> = properties
                        .iter()
                        .map(|(name, value): &(String, SaveValue)| {
                            Ok((name.clone(), decode_value(value, &runtime_nodes)?))
                        })
                        .collect();
                    let values: Vec<(String, Value)> = values?;
                    object.with_mut(|target: &mut Vec<(String, Value)>| *target = values);
                }
                _ => return Err(invalid("Value 节点类型不一致")),
            }
        }

        self.roots
            .iter()
            .map(|(name, value): (&String, &SaveValue)| {
                Ok((name.clone(), decode_value(value, &runtime_nodes)?))
            })
            .collect()
    }
}

impl Encoder {
    fn encode_value(&mut self, value: &Value, path: &str) -> Result<SaveValue, SaveError> {
        match value {
            Value::Undefined => Ok(SaveValue::Undefined),
            Value::Null => Ok(SaveValue::Null),
            Value::Boolean(value) => Ok(SaveValue::Boolean(*value)),
            Value::Number(value) => Ok(SaveValue::Number(value.to_bits())),
            Value::String(value) => Ok(SaveValue::String(value.as_units().to_vec())),
            Value::Array(array) => self.encode_array(array, path),
            Value::Object(object) => self.encode_object(object, path),
            Value::Callable(_) | Value::ScriptCallable(_) | Value::Namespace(_) => {
                Err(SaveError::UnsupportedValue {
                    path: path.to_owned(),
                })
            }
        }
    }

    fn encode_array(&mut self, array: &ArrayValue, path: &str) -> Result<SaveValue, SaveError> {
        let identity: RuntimeIdentity = RuntimeIdentity::Array(array.identity());
        if let Some(id) = self.identities.get(&identity) {
            return Ok(SaveValue::Array(*id));
        }
        let id: u64 = self.allocate(identity)?;
        let items: Vec<Value> = array.snapshot();
        let values: Result<Vec<SaveValue>, SaveError> = items
            .iter()
            .enumerate()
            .map(|(index, value): (usize, &Value)| {
                self.encode_value(value, format!("{path}[{index}]").as_str())
            })
            .collect();
        self.nodes.insert(id, SaveNode::Array(values?));
        Ok(SaveValue::Array(id))
    }

    fn encode_object(&mut self, object: &ObjectValue, path: &str) -> Result<SaveValue, SaveError> {
        let identity: RuntimeIdentity = RuntimeIdentity::Object(object.identity());
        if let Some(id) = self.identities.get(&identity) {
            return Ok(SaveValue::Object(*id));
        }
        let id: u64 = self.allocate(identity)?;
        let properties: Vec<(String, Value)> = object.snapshot();
        let values: Result<Vec<(String, SaveValue)>, SaveError> = properties
            .iter()
            .map(|(name, value): &(String, Value)| {
                Ok((
                    name.clone(),
                    self.encode_value(value, format!("{path}.{name}").as_str())?,
                ))
            })
            .collect();
        self.nodes.insert(id, SaveNode::Object(values?));
        Ok(SaveValue::Object(id))
    }

    fn allocate(&mut self, identity: RuntimeIdentity) -> Result<u64, SaveError> {
        let id: u64 = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| invalid("Value 节点 ID 已耗尽"))?;
        self.identities.insert(identity, id);
        Ok(id)
    }
}

fn decode_value(value: &SaveValue, nodes: &BTreeMap<u64, Value>) -> Result<Value, SaveError> {
    match value {
        SaveValue::Undefined => Ok(Value::Undefined),
        SaveValue::Null => Ok(Value::Null),
        SaveValue::Boolean(value) => Ok(Value::Boolean(*value)),
        SaveValue::Number(bits) => Ok(Value::Number(f64::from_bits(*bits))),
        SaveValue::String(units) => Ok(Value::String(TextValue::from_units(units.clone()))),
        SaveValue::Array(id) => match nodes.get(id) {
            Some(Value::Array(array)) => Ok(Value::Array(array.clone())),
            Some(_) => Err(invalid("Array 引用指向了其他节点类型")),
            None => Err(invalid("Array 引用了不存在的节点")),
        },
        SaveValue::Object(id) => match nodes.get(id) {
            Some(Value::Object(object)) => Ok(Value::Object(object.clone())),
            Some(_) => Err(invalid("Object 引用指向了其他节点类型")),
            None => Err(invalid("Object 引用了不存在的节点")),
        },
    }
}

fn invalid(message: &str) -> SaveError {
    SaveError::InvalidValueGraph {
        message: message.to_owned(),
    }
}

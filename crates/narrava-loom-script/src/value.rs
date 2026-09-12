//! Core 值与 JSON 的脚本边界转换。

use crate::ScriptError;
use narrava_loom_core::expression::value::Value;

/// JSON → Core 值（供 Input 与宏返回值转换）。
pub fn json_to_value(value: &serde_json::Value) -> Result<Value, ScriptError> {
    match value {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(value) => Ok(Value::Boolean(*value)),
        serde_json::Value::Number(value) => value
            .as_f64()
            .map(Value::Number)
            .ok_or_else(|| ScriptError::new("script.value", "数值超出范围")),
        serde_json::Value::String(value) => Ok(Value::string(value.as_str())),
        serde_json::Value::Array(values) => values
            .iter()
            .map(json_to_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::array),
        serde_json::Value::Object(values) => values
            .iter()
            .map(|(name, value)| Ok((name.clone(), json_to_value(value)?)))
            .collect::<Result<Vec<_>, ScriptError>>()
            .map(Value::object),
    }
}

/// Core 值 → JSON；函数/命名空间不可序列化。
pub(super) fn value_to_json(value: &Value) -> Result<serde_json::Value, ()> {
    match value {
        Value::Undefined | Value::Null => Ok(serde_json::Value::Null),
        Value::Boolean(value) => Ok(serde_json::Value::Bool(*value)),
        Value::Number(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .ok_or(()),
        Value::String(value) => value
            .to_unicode_string()
            .map(serde_json::Value::String)
            .ok_or(()),
        Value::Array(value) => value
            .snapshot()
            .iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        Value::Object(value) => value
            .snapshot()
            .into_iter()
            .map(|(name, value)| Ok((name, value_to_json(&value)?)))
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(serde_json::Value::Object),
        Value::Callable(_) | Value::ScriptCallable(_) | Value::Namespace(_) => Err(()),
    }
}

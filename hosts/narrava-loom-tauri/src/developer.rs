//! 开发者模式专用的 State 检查与修改边界。
//!
//! 这里只转换可安全展示或由 JSON 表达的数据；平台命令仍必须先在 TauriHost 上校验
//! developer 配置，避免把调试能力误当成常规游戏 API。

use narrava_loom_core::{expression::value::Value, state::State};
use serde::Serialize;

use crate::{HostErrorDto, script_runtime};

/// 开发者管理面板中展示的一条 State 条目。
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DeveloperValueDto {
    /// 所属命名空间（`global`/`variables`/`temporary`）。
    pub namespace: String,
    /// 条目名。
    pub name: String,
    /// 值的类型标签（如 `string`/`number`/`array`）。
    pub kind: String,
    /// 可 JSON 表达的当前值；数组/对象只给摘要。
    pub value: serde_json::Value,
}

/// developer 未开启时的统一错误。
pub(crate) fn developer_disabled() -> HostErrorDto {
    HostErrorDto::new("tauri_host.developer_disabled", "开发者模式没有开启")
}

/// 导出全部命名空间条目供管理面板展示。
pub(crate) fn developer_state(state: &State) -> Vec<DeveloperValueDto> {
    state
        .global_entries()
        .map(|(name, value)| developer_value("global", name, value))
        .chain(
            state
                .variables_entries()
                .map(|(name, value)| developer_value("variables", name, value)),
        )
        .chain(
            state
                .temporary_entries()
                .map(|(name, value)| developer_value("temporary", name, value)),
        )
        .collect()
}

/// 单条 State 值转 DTO；数组/对象仅保留长度摘要。
fn developer_value(namespace: &str, name: &str, value: &Value) -> DeveloperValueDto {
    let (kind, value) = match value {
        Value::Undefined => ("undefined", serde_json::Value::Null),
        Value::Null => ("null", serde_json::Value::Null),
        Value::Boolean(value) => ("boolean", serde_json::Value::Bool(*value)),
        Value::Number(value) => (
            "number",
            serde_json::Number::from_f64(*value)
                .map(serde_json::Value::Number)
                .unwrap_or_else(|| serde_json::Value::String(value.to_string())),
        ),
        Value::String(value) => (
            "string",
            serde_json::Value::String(
                value
                    .to_unicode_string()
                    .unwrap_or_else(|| String::from("<非 Unicode 字符串>")),
            ),
        ),
        Value::Array(value) => (
            "array",
            serde_json::Value::String(format!("Array({})", value.len())),
        ),
        Value::Object(value) => (
            "object",
            serde_json::Value::String(format!("Object({})", value.len())),
        ),
        Value::Callable(_) => (
            "callable",
            serde_json::Value::String(String::from("<Core 函数>")),
        ),
        Value::ScriptCallable(_) => (
            "scriptCallable",
            serde_json::Value::String(String::from("<脚本函数>")),
        ),
        Value::Namespace(_) => (
            "namespace",
            serde_json::Value::String(String::from("<命名空间>")),
        ),
    };
    DeveloperValueDto {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
        kind: kind.to_owned(),
        value,
    }
}

/// 按 namespace 把 JSON 值写回 State（仅 `global`/`variables`/`temporary`）。
pub(crate) fn developer_set(
    _script: &script_runtime::EcmaBinding,
    state: &mut State,
    namespace: &str,
    name: &str,
    json: &serde_json::Value,
) -> Result<(), HostErrorDto> {
    let value = script_runtime::json_to_value(json)?;
    match namespace {
        "global" => state.global_set(name, value),
        "variables" => state.variables_set(name, value),
        "temporary" => state.temporary_set(name, value),
        _ => {
            return Err(HostErrorDto::new(
                "tauri_host.developer_namespace",
                "namespace 必须是 global、variables 或 temporary",
            ));
        }
    };
    Ok(())
}

/// 按 namespace 删除 State 条目（仅 `global`/`variables`/`temporary`）。
pub(crate) fn developer_delete(
    _script: &script_runtime::EcmaBinding,
    state: &mut State,
    namespace: &str,
    name: &str,
) -> Result<(), HostErrorDto> {
    match namespace {
        "global" => state.global_del(name),
        "variables" => state.variables_del(name),
        "temporary" => state.temporary_del(name),
        _ => {
            return Err(HostErrorDto::new(
                "tauri_host.developer_namespace",
                "namespace 必须是 global、variables 或 temporary",
            ));
        }
    };
    Ok(())
}

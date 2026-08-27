//! Boa 对活动 Rust `State` 的调用期适配器。
//!
//! `State` 只在执行 JavaScript 的这段同步调用期间移入 Context slot。移动 BTreeMap
//! 不会复制其内容；调用结束后无论成功失败都会移回 Worker 持有的同一个 Rust State。

use std::cell::RefCell;

use boa_engine::{
    Context, Finalize, JsArgs, JsData, JsNativeError, JsResult, JsValue, NativeFunction, Trace,
    js_string,
};
use narrava_loom_core::{expression::value::Value, state::State};

use super::{json_to_value, value_to_json};

#[derive(Trace, Finalize, JsData)]
struct ActiveState {
    #[unsafe_ignore_trace]
    value: RefCell<Option<State>>,
}

/// 安装 `__narravaState*` 原生函数与活动 State 槽位。
pub(super) fn install(context: &mut Context) -> JsResult<()> {
    context.insert_data(ActiveState {
        value: RefCell::new(None),
    });
    register(context, "__narravaStateGet", 2, state_get)?;
    register(context, "__narravaStateHas", 2, state_has)?;
    register(context, "__narravaStateSet", 3, state_set)?;
    register(context, "__narravaStateDel", 2, state_del)?;
    register(context, "__narravaStateSnapshot", 1, state_snapshot)?;
    register(context, "__narravaStateReplace", 2, state_replace)?;
    Ok(())
}

/// 把 Rust `State` 移入 Context 槽位执行一段脚本调用，结束后移回原处。
pub(super) fn with_state<T>(
    context: &mut Context,
    state: &mut State,
    operation: impl FnOnce(&mut Context) -> T,
) -> T {
    let moved = std::mem::take(state);
    let slot = context
        .get_data::<ActiveState>()
        .expect("State bridge 必须在执行脚本前安装");
    let previous = slot.value.borrow_mut().replace(moved);
    assert!(previous.is_none(), "State bridge 不允许重入占用活动 State");

    let result = operation(context);

    let restored = context
        .get_data::<ActiveState>()
        .expect("State bridge 不能在脚本执行期间被移除")
        .value
        .borrow_mut()
        .take()
        .expect("State bridge 必须归还活动 State");
    *state = restored;
    result
}

/// 注册一个原生全局函数。
fn register(
    context: &mut Context,
    name: &'static str,
    length: usize,
    function: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
) -> JsResult<()> {
    context.register_global_builtin_callable(
        js_string!(name),
        length,
        NativeFunction::from_fn_ptr(function),
    )
}

/// `State.*.get` 原生实现。
fn state_get(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let namespace = string_argument(arguments, 0, context)?;
    let name = string_argument(arguments, 1, context)?;
    let value = with_active(context, |state| get(state, &namespace, &name).cloned())?;
    match value {
        Some(value) => core_to_js(&value, context),
        None => Ok(JsValue::undefined()),
    }
}

/// `State.*.has` 原生实现。
fn state_has(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let namespace = string_argument(arguments, 0, context)?;
    let name = string_argument(arguments, 1, context)?;
    with_active(context, |state| has(state, &namespace, &name)).map(JsValue::new)
}

/// `State.*.set` 原生实现（返回旧值）。
fn state_set(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let namespace = string_argument(arguments, 0, context)?;
    let name = string_argument(arguments, 1, context)?;
    let value = js_to_core(arguments.get_or_undefined(2), context)?;
    let old = with_active(context, |state| set(state, &namespace, &name, value))?;
    match old {
        Some(value) => core_to_js(&value, context),
        None => Ok(JsValue::undefined()),
    }
}

/// `State.*.del` 原生实现（返回旧值）。
fn state_del(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let namespace = string_argument(arguments, 0, context)?;
    let name = string_argument(arguments, 1, context)?;
    let old = with_active(context, |state| del(state, &namespace, &name))?;
    match old {
        Some(value) => core_to_js(&value, context),
        None => Ok(JsValue::undefined()),
    }
}

/// 导出某命名空间全部条目为 JSON 对象（用于 `Save.capture`）。
fn state_snapshot(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let namespace = string_argument(arguments, 0, context)?;
    let values = with_active(context, |state| entries(state, &namespace))?;
    let json = values
        .into_iter()
        .map(|(name, value)| {
            value_to_json(&value)
                .map(|value| (name, value))
                .map_err(|()| type_error("State namespace 含脚本不可序列化值"))
        })
        .collect::<JsResult<serde_json::Map<_, _>>>()?;
    JsValue::from_json(&serde_json::Value::Object(json), context)
}

/// 用 JSON 对象整体替换某命名空间（用于 `Save.restore`）。
fn state_replace(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let namespace = string_argument(arguments, 0, context)?;
    let json = arguments
        .get_or_undefined(1)
        .to_json(context)?
        .ok_or_else(|| type_error("State replacement 必须是 object"))?;
    let serde_json::Value::Object(values) = json else {
        return Err(type_error("State replacement 必须是 object"));
    };
    let values = values
        .iter()
        .map(|(name, value)| Ok((name.clone(), json_to_value(value).map_err(host_error)?)))
        .collect::<JsResult<Vec<_>>>()?;
    with_active(context, |state| replace(state, &namespace, values))?;
    Ok(JsValue::undefined())
}

/// 在活动 State 上执行操作；未在调用期内访问则报错。
fn with_active<T>(context: &Context, operation: impl FnOnce(&mut State) -> T) -> JsResult<T> {
    let slot = context
        .get_data::<ActiveState>()
        .ok_or_else(|| type_error("State bridge 未安装"))?;
    let mut active = slot.value.borrow_mut();
    let state = active
        .as_mut()
        .ok_or_else(|| type_error("State API 只能在 Narrava 调用期使用"))?;
    Ok(operation(state))
}

/// 按命名空间读值。
fn get<'state>(state: &'state State, namespace: &str, name: &str) -> Option<&'state Value> {
    match namespace {
        "global" => state.global_get(name),
        "variables" => state.variables_get(name),
        "temporary" => state.temporary_get(name),
        "setup" => Some(state.setup_get()),
        _ => None,
    }
}

/// 按命名空间查询存在性。
fn has(state: &State, namespace: &str, name: &str) -> bool {
    match namespace {
        "global" => state.global_has(name),
        "variables" => state.variables_has(name),
        "temporary" => state.temporary_has(name),
        "setup" => true,
        _ => false,
    }
}

/// 按命名空间写值，返回旧值。
fn set(state: &mut State, namespace: &str, name: &str, value: Value) -> Option<Value> {
    match namespace {
        "global" => state.global_set(name, value),
        "variables" => state.variables_set(name, value),
        "temporary" => state.temporary_set(name, value),
        "setup" => Some(state.setup_set(value)),
        _ => None,
    }
}

/// 按命名空间删除，返回旧值。
fn del(state: &mut State, namespace: &str, name: &str) -> Option<Value> {
    match namespace {
        "global" => state.global_del(name),
        "variables" => state.variables_del(name),
        "temporary" => state.temporary_del(name),
        _ => None,
    }
}

fn entries(state: &State, namespace: &str) -> Vec<(String, Value)> {
    match namespace {
        "global" => state
            .global_entries()
            .map(|(name, value)| (name.to_owned(), value.clone()))
            .collect(),
        "variables" => state
            .variables_entries()
            .map(|(name, value)| (name.to_owned(), value.clone()))
            .collect(),
        "temporary" => state
            .temporary_entries()
            .map(|(name, value)| (name.to_owned(), value.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

fn replace(state: &mut State, namespace: &str, values: Vec<(String, Value)>) {
    let names: Vec<String> = entries(state, namespace)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    for name in names {
        let _ = del(state, namespace, &name);
    }
    for (name, value) in values {
        let _ = set(state, namespace, &name, value);
    }
}

/// Core 值 → JsValue；脚本函数转为 `__narravaCallable` 标记对象。
fn core_to_js(value: &Value, context: &mut Context) -> JsResult<JsValue> {
    match value {
        Value::Undefined => Ok(JsValue::undefined()),
        Value::ScriptCallable(callable) => JsValue::from_json(
            &serde_json::json!({
                "__narravaCallable": callable.id(),
                "name": callable.name(),
            }),
            context,
        ),
        _ => JsValue::from_json(
            &value_to_json(value).map_err(|()| type_error("State value 不能转换为 ECMAScript"))?,
            context,
        ),
    }
}

/// JsValue → Core 值；`__narravaCallable` 标记对象还原为脚本函数。
fn js_to_core(value: &JsValue, context: &mut Context) -> JsResult<Value> {
    let Some(json) = value.to_json(context)? else {
        return Ok(Value::Undefined);
    };
    if let serde_json::Value::Object(object) = &json
        && let Some(id) = object
            .get("__narravaCallable")
            .and_then(serde_json::Value::as_u64)
    {
        let name = object
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<anonymous>");
        return Ok(Value::ScriptCallable(
            narrava_loom_core::expression::value::ScriptCallable::new(id, name.to_owned()),
        ));
    }
    json_to_value(&json).map_err(host_error)
}

/// 取指定位置的字符串参数。
fn string_argument(arguments: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
    arguments
        .get_or_undefined(index)
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
}

/// Host 错误 → JS 类型错误。
fn host_error(error: narrava_loom_protocol::HostErrorDto) -> boa_engine::JsError {
    type_error(error.message)
}

/// 构造 JS TypeError。
fn type_error(message: impl Into<String>) -> boa_engine::JsError {
    JsNativeError::typ().with_message(message.into()).into()
}

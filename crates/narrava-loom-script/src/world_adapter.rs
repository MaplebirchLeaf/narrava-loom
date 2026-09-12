//! Boa 对 Core 地点定义及当前世界位置的适配器。

use std::cell::{Cell, Ref, RefCell, RefMut};

use boa_engine::{
    Context, Finalize, JsArgs, JsData, JsError, JsNativeError, JsResult, JsValue, NativeFunction,
    Trace, js_string,
};
use narrava_loom_core::{
    state::State,
    world::{MAX_COORDINATE, Place, Point, WorldError, WorldPosition, WorldState},
};
use serde::{Deserialize, Serialize};

use crate::{EcmaBinding, EcmaRuntime, state_adapter::with_active};

/// 这里只保存 Rust 值，不持有需要 Boa GC 追踪的脚本对象。
#[derive(Trace, Finalize, JsData)]
struct WorldBridge {
    #[unsafe_ignore_trace]
    sealed: Cell<bool>,
    #[unsafe_ignore_trace]
    refresh: RefCell<Option<WorldRefresh>>,
}

struct WorldRefresh {
    state: WorldState,
    // Reaction Phase 每次进入只调用一次；后续调用意味着已发生新的导航。
    entered: bool,
}

/// JS number 先按浮点读取；安全整数校验后才转换成 Core 坐标。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceDefinition {
    id: String,
    name: Option<String>,
    parent: Option<String>,
    bounds: Vec<[f64; 2]>,
    entry: Option<[f64; 2]>,
}

pub(super) fn install(context: &mut Context) -> JsResult<()> {
    context.insert_data(WorldBridge {
        sealed: Cell::new(false),
        refresh: RefCell::new(None),
    });
    for (name, length, function) in [
        ("__narravaWorldAdd", 1, add as NativeCall),
        ("__narravaWorldGet", 1, get as NativeCall),
        ("__narravaWorldPlaces", 0, places as NativeCall),
        ("__narravaWorldLocate", 1, locate as NativeCall),
        ("__narravaWorldCurrent", 0, current as NativeCall),
        ("__narravaWorldMove", 1, move_to as NativeCall),
    ] {
        context.register_global_builtin_callable(
            js_string!(name),
            length,
            NativeFunction::from_fn_ptr(function),
        )?;
    }
    Ok(())
}

type NativeCall = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;

/// 初始脚本执行完毕后禁止更改地点定义；父地点验证由 Runtime 装载入口完成。
pub(super) fn seal(context: &Context) {
    bridge(context).sealed.set(true);
}

impl EcmaBinding {
    /// 当前页重绘只读取既有世界位置；该视图跨 Pending 保留到命令结束。
    pub(crate) fn set_world_refresh(&self, state: Option<WorldState>) {
        let runtime: Ref<'_, EcmaRuntime> = self.runtime.borrow();
        *bridge(&runtime.context).refresh.borrow_mut() =
            state.map(|state: WorldState| WorldRefresh {
                state,
                entered: false,
            });
    }

    /// 首次进入被重绘页时恢复保存位置；再次进入生命周期意味着新导航，解除只读。
    pub(crate) fn enter_world_refresh(&self) -> Option<WorldState> {
        let runtime: Ref<'_, EcmaRuntime> = self.runtime.borrow();
        let mut refresh: RefMut<'_, Option<WorldRefresh>> =
            bridge(&runtime.context).refresh.borrow_mut();
        if let Some(view) = refresh.as_mut() {
            if !view.entered {
                view.entered = true;
                return Some(view.state.clone());
            }
            *refresh = None;
        }
        None
    }
}

fn add(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if bridge(context).sealed.get() {
        return Err(type_error("World.add 只能在初始脚本装载期间调用"));
    }
    let json: serde_json::Value = argument_json(arguments, context)?;
    let definition: PlaceDefinition = serde_json::from_value(json)
        .map_err(|error: serde_json::Error| type_error(error.to_string()))?;
    let place: Place = Place {
        id: definition.id,
        name: definition.name,
        parent: definition.parent,
        bounds: definition
            .bounds
            .into_iter()
            .map(point)
            .collect::<JsResult<Vec<Point>>>()?,
        entry: definition.entry.map(point).transpose()?,
    };
    with_active(context, |state: &mut State| state.world_mut().add(place))?.map_err(world_error)?;
    Ok(JsValue::undefined())
}

fn get(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let id: String = arguments
        .get_or_undefined(0)
        .as_string()
        .ok_or_else(|| type_error("World.get id 必须是字符串"))?
        .to_std_string_escaped();
    let place: Option<Place> =
        with_active(context, |state: &mut State| state.world().get(&id).cloned())?;
    match place {
        Some(place) => snapshot(&place, context),
        None => Ok(JsValue::undefined()),
    }
}

fn places(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let places: Vec<Place> = with_active(context, |state: &mut State| {
        state.world().places().cloned().collect()
    })?;
    snapshot(&places, context)
}

fn locate(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let point: Point = point_argument(arguments, context)?;
    let places: Vec<Place> = with_active(context, |state: &mut State| {
        state.world().locate(point).into_iter().cloned().collect()
    })?;
    snapshot(&places, context)
}

fn current(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let position: Option<WorldPosition> = with_active(context, |state: &mut State| {
        state.world_state().position.clone()
    })?;
    snapshot(&position, context)
}

fn move_to(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let point: Point = point_argument(arguments, context)?;
    let refreshing: bool = bridge(context).refresh.borrow().is_some();
    let position: WorldPosition = with_active(context, |state: &mut State| {
        let mut next: WorldState = state.world_state().clone();
        state.world().move_to(&mut next, point)?;
        // 重绘仍校验请求，但只有普通命令提交位移；两者都返回权威位置的副本。
        if !refreshing {
            *state.world_state_mut() = next;
        }
        let position: WorldPosition = state
            .world_state()
            .position
            .clone()
            .expect("成功移动必须有当前地点");
        Ok::<WorldPosition, WorldError>(position)
    })?
    .map_err(world_error)?;
    snapshot(&position, context)
}

fn bridge(context: &Context) -> &WorldBridge {
    context
        .get_data::<WorldBridge>()
        .expect("World bridge 必须在执行脚本前安装")
}

fn argument_json(arguments: &[JsValue], context: &mut Context) -> JsResult<serde_json::Value> {
    arguments
        .get_or_undefined(0)
        .to_json(context)?
        .ok_or_else(|| type_error("World 参数必须是可序列化的数据"))
}

fn point_argument(arguments: &[JsValue], context: &mut Context) -> JsResult<Point> {
    let json: serde_json::Value = argument_json(arguments, context)?;
    let coordinates: [f64; 2] = serde_json::from_value(json)
        .map_err(|error: serde_json::Error| type_error(error.to_string()))?;
    point(coordinates)
}

fn point(coordinates: [f64; 2]) -> JsResult<Point> {
    if coordinates.into_iter().any(|value: f64| {
        !value.is_finite() || value.fract() != 0.0 || value.abs() > MAX_COORDINATE as f64
    }) {
        return Err(type_error("World 坐标必须是 JavaScript 安全整数"));
    }
    Ok([coordinates[0] as i64, coordinates[1] as i64])
}

/// 查询结果始终是独立脚本对象，作者修改快照不会绕过 State 的写入规则。
fn snapshot(value: &impl Serialize, context: &mut Context) -> JsResult<JsValue> {
    let json: serde_json::Value = serde_json::to_value(value)
        .map_err(|error: serde_json::Error| type_error(error.to_string()))?;
    JsValue::from_json(&json, context)
}

fn type_error(message: impl Into<String>) -> JsError {
    JsNativeError::typ().with_message(message.into()).into()
}

fn world_error(error: WorldError) -> JsError {
    JsNativeError::error()
        .with_message(format!("{}: {error}", error.code()))
        .into()
}

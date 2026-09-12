//! 脚本随机数与 Core 共用同一条可保存序列，不使用 Boa 自己的随机源。

use boa_engine::{Context, JsArgs, JsNativeError, JsResult, JsValue, NativeFunction, js_string};
use narrava_loom_core::{random::RandomState, state::State};

use crate::state_adapter::with_active;

pub(super) fn install(context: &mut Context) -> JsResult<()> {
    for (name, length, call) in [
        ("__narravaRandomNext", 0, next as NativeCall),
        ("__narravaRandomSeed", 1, seed as NativeCall),
        ("__narravaRandomCurrent", 0, current as NativeCall),
    ] {
        context.register_global_builtin_callable(
            js_string!(name),
            length,
            NativeFunction::from_fn_ptr(call),
        )?;
    }
    Ok(())
}

type NativeCall = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;

fn next(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    with_active(context, |state: &mut State| state.random()).map(JsValue::new)
}

fn seed(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let seed: f64 = arguments
        .get_or_undefined(0)
        .as_number()
        .ok_or_else(|| JsNativeError::typ().with_message("Random.seed 必须是非负安全整数"))?;
    if !seed.is_finite() || !(0.0..=9_007_199_254_740_991.0).contains(&seed) || seed.fract() != 0.0
    {
        return Err(JsNativeError::range()
            .with_message("Random.seed 必须在 0 到 2^53-1 之间")
            .into());
    }
    with_active(context, |state: &mut State| state.seed_random(seed as u64))?;
    Ok(JsValue::undefined())
}

fn current(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let random: RandomState = with_active(context, |state: &mut State| state.random_state())?;
    // 64 位内部状态以字符串越过 JS 边界，保留全部位数。
    JsValue::from_json(
        &serde_json::json!({
            "seed": random.seed().to_string(),
            "state": random.state().to_string(),
        }),
        context,
    )
}

//! Boa 对只读 Core Resource 目录的按需适配器。

use boa_engine::{
    Context, Finalize, JsArgs, JsData, JsNativeError, JsResult, JsValue, NativeFunction, Trace,
    js_string,
};
use narrava_loom_core::resource::ResourceCatalog;

#[derive(Trace, Finalize, JsData)]
struct Resources {
    #[unsafe_ignore_trace]
    catalog: ResourceCatalog,
}

pub(super) fn install(context: &mut Context, resources: ResourceCatalog) -> JsResult<()> {
    context.insert_data(Resources { catalog: resources });
    register(context, "__narravaResourcePaths", 0, paths)?;
    register(context, "__narravaResourceHas", 1, has)?;
    register(context, "__narravaResourceInfo", 1, info)?;
    register(context, "__narravaResourceRead", 1, read)?;
    register(context, "__narravaResourceText", 1, text)?;
    Ok(())
}

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

fn paths(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let paths = resources(context)
        .paths()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    JsValue::from_json(&serde_json::json!(paths), context)
}

fn has(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let path = path_argument(arguments, context)?;
    Ok(JsValue::new(resources(context).has(&path)))
}

fn info(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let path = path_argument(arguments, context)?;
    let Some(info) = resources(context).info(&path) else {
        return Ok(JsValue::undefined());
    };
    JsValue::from_json(
        &serde_json::json!({
            "path": info.path(),
            "mediaType": info.media_type(),
            "size": info.size(),
        }),
        context,
    )
}

fn read(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let path = path_argument(arguments, context)?;
    let Some(bytes) = resources(context).read(&path).map_err(resource_error)? else {
        return Ok(JsValue::undefined());
    };
    let bytes = bytes.to_vec();
    JsValue::from_json(&serde_json::json!(bytes), context)
}

fn text(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let path = path_argument(arguments, context)?;
    let value = resources(context)
        .text(&path)
        .map_err(resource_error)?
        .map(str::to_owned);
    match value {
        Some(value) => Ok(JsValue::from(js_string!(value))),
        None => Ok(JsValue::undefined()),
    }
}

fn resource_error(error: impl std::fmt::Display) -> boa_engine::JsError {
    JsNativeError::error()
        .with_message(error.to_string())
        .into()
}

fn resources(context: &Context) -> &ResourceCatalog {
    &context
        .get_data::<Resources>()
        .expect("Resource bridge 必须在执行脚本前安装")
        .catalog
}

fn path_argument(arguments: &[JsValue], context: &mut Context) -> JsResult<String> {
    arguments
        .get_or_undefined(0)
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
}

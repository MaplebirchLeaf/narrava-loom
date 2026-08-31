//! Boa `Reaction` API 到 Core Reaction Registry 的边界转换。

use std::{
    cell::{Ref, RefCell},
    rc::Rc,
};

use boa_engine::{
    Context, Finalize, JsArgs, JsData, JsNativeError, JsResult, JsString, JsValue, NativeFunction,
    Trace, js_string,
};
use narrava_loom_core::{
    expression::value::{ScriptCallable, Value},
    reaction::{
        PassageMatcher, PassageSelector, PassageTagSelector, ReactionCallbacks, ReactionDefinition,
        ReactionEffect, ReactionEvent, ReactionId, ReactionRegistry, ReactionTrigger, StatePath,
    },
};
use serde::Deserialize;

use super::json_to_value;

#[derive(Trace, Finalize, JsData)]
pub(super) struct ActiveReactions {
    #[unsafe_ignore_trace]
    pub(super) registry: Rc<RefCell<ReactionRegistry<ScriptCallable>>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReactionDefinitionDto {
    id: String,
    event: Option<String>,
    state: Option<String>,
    #[serde(default)]
    lifecycle: bool,
    passage: Option<PassageDto>,
    cond: Option<ScriptCallableDto>,
    widget: Option<String>,
    include: Option<String>,
    replace: Option<String>,
    goto: Option<String>,
    emit: Option<ReactionEmitDto>,
    #[serde(default)]
    exit: bool,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    once: bool,
    limit: Option<u64>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct ScriptCallableDto {
    #[serde(rename = "__narravaCallable")]
    id: u64,
    name: String,
}

#[derive(Deserialize)]
struct ReactionEmitDto {
    name: String,
    #[serde(default)]
    payload: EmitPayloadDto,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum EmitPayloadDto {
    Callable(ScriptCallableDto),
    Static(serde_json::Value),
}

impl Default for EmitPayloadDto {
    fn default() -> Self {
        Self::Static(serde_json::Value::Null)
    }
}

#[derive(Deserialize)]
struct PassageDto {
    #[serde(default, rename = "match")]
    matches: Vec<MatcherDto>,
    #[serde(default, rename = "exclude")]
    excludes: Vec<MatcherDto>,
    #[serde(default)]
    tags: PassageTagsDto,
}

#[derive(Default, Deserialize)]
struct PassageTagsDto {
    #[serde(default)]
    any: Vec<String>,
    #[serde(default)]
    all: Vec<String>,
    #[serde(default)]
    none: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum MatcherDto {
    Exact { exact: String },
    Regex { regex: String, flags: String },
}

pub(super) fn install(
    context: &mut Context,
) -> JsResult<Rc<RefCell<ReactionRegistry<ScriptCallable>>>> {
    let registry: Rc<RefCell<ReactionRegistry<ScriptCallable>>> =
        Rc::new(RefCell::new(ReactionRegistry::new()));
    context.insert_data(ActiveReactions {
        registry: registry.clone(),
    });
    register_bridge_function(context, "__narravaReactionAdd", reaction_add)?;
    register_bridge_function(context, "__narravaReactionGet", reaction_get)?;
    register_bridge_function(context, "__narravaReactionEnable", reaction_enable)?;
    register_bridge_function(context, "__narravaReactionDisable", reaction_disable)?;
    register_bridge_function(context, "__narravaReactionReset", reaction_reset)?;
    Ok(registry)
}

fn register_bridge_function(
    context: &mut Context,
    name: &'static str,
    function: fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
) -> JsResult<()> {
    context.register_global_builtin_callable(
        js_string!(name),
        1,
        NativeFunction::from_fn_ptr(function),
    )
}

fn reaction_add(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let dto: ReactionDefinitionDto =
        serde_json::from_str(&string_argument(arguments, context)?).map_err(bridge_error)?;
    let (definition, callbacks): (ReactionDefinition, ReactionCallbacks<ScriptCallable>) =
        decode_definition(dto)?;
    let id: String = definition.id.as_str().to_owned();
    active_registry(context)?
        .borrow_mut()
        .add(definition, callbacks)
        .map_err(bridge_error)?;
    Ok(JsValue::new(JsString::from(
        serialize_status(context, &id)?.unwrap(),
    )))
}

fn reaction_get(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let id: String = string_argument(arguments, context)?;
    Ok(serialize_status(context, &id)?
        .map(|value: String| JsValue::new(JsString::from(value)))
        .unwrap_or_else(JsValue::undefined))
}

fn reaction_enable(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    mutate_reaction(arguments, context, ReactionRegistry::enable)
}

fn reaction_disable(
    _: &JsValue,
    arguments: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    mutate_reaction(arguments, context, ReactionRegistry::disable)
}

fn reaction_reset(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    mutate_reaction(arguments, context, ReactionRegistry::reset)
}

fn mutate_reaction(
    arguments: &[JsValue],
    context: &mut Context,
    operation: fn(
        &mut ReactionRegistry<ScriptCallable>,
        &str,
    ) -> Result<bool, narrava_loom_core::reaction::ReactionError>,
) -> JsResult<JsValue> {
    let id: String = string_argument(arguments, context)?;
    operation(&mut active_registry(context)?.borrow_mut(), &id)
        .map(JsValue::new)
        .map_err(bridge_error)
}

fn decode_definition(
    dto: ReactionDefinitionDto,
) -> JsResult<(ReactionDefinition, ReactionCallbacks<ScriptCallable>)> {
    let mut triggers: Vec<ReactionTrigger> = Vec::new();
    if let Some(event) = dto.event {
        triggers.push(ReactionTrigger::Event(event));
    }
    if let Some(state) = dto.state {
        triggers.push(ReactionTrigger::State(
            StatePath::parse(state).map_err(bridge_error)?,
        ));
    }
    if dto.lifecycle {
        triggers.push(ReactionTrigger::Lifecycle);
    }
    if triggers.len() != 1 {
        return Err(bridge_error(
            "Reaction 必须且只能声明 event、state、lifecycle 之一",
        ));
    }
    let condition: Option<ScriptCallable> = dto
        .cond
        .map(|callable: ScriptCallableDto| ScriptCallable::new(callable.id, callable.name));
    let (emit, emit_payload): (Option<ReactionEvent>, Option<ScriptCallable>) = match dto.emit {
        None => (None, None),
        Some(ReactionEmitDto {
            name,
            payload: EmitPayloadDto::Static(payload),
        }) => (
            Some(ReactionEvent {
                name,
                payload: json_to_value(&payload).map_err(|error| bridge_error(error.message))?,
            }),
            None,
        ),
        Some(ReactionEmitDto {
            name,
            payload: EmitPayloadDto::Callable(callable),
        }) => (
            Some(ReactionEvent {
                name,
                payload: Value::Null,
            }),
            Some(ScriptCallable::new(callable.id, callable.name)),
        ),
    };
    Ok((
        ReactionDefinition {
            id: ReactionId::parse(dto.id).map_err(bridge_error)?,
            trigger: triggers.pop().unwrap(),
            passage: dto.passage.map(decode_passage_selector).transpose()?,
            effect: ReactionEffect {
                widget: dto.widget,
                include: dto.include,
                replace: dto.replace,
                goto: dto.goto,
                emit,
                exit: dto.exit,
            },
            enabled: dto.enabled,
            once: dto.once,
            limit: dto.limit,
            tags: dto.tags,
        },
        ReactionCallbacks {
            condition,
            emit_payload,
        },
    ))
}

fn decode_passage_selector(dto: PassageDto) -> JsResult<PassageSelector> {
    Ok(PassageSelector {
        matches: dto
            .matches
            .into_iter()
            .map(decode_passage_matcher)
            .collect::<JsResult<Vec<PassageMatcher>>>()?,
        excludes: dto
            .excludes
            .into_iter()
            .map(decode_passage_matcher)
            .collect::<JsResult<Vec<PassageMatcher>>>()?,
        tags: PassageTagSelector {
            any: dto.tags.any,
            all: dto.tags.all,
            none: dto.tags.none,
        },
    })
}

fn decode_passage_matcher(dto: MatcherDto) -> JsResult<PassageMatcher> {
    match dto {
        MatcherDto::Exact { exact } => PassageMatcher::exact(exact).map_err(bridge_error),
        MatcherDto::Regex { regex, flags } => {
            PassageMatcher::regex(regex, flags).map_err(bridge_error)
        }
    }
}

fn serialize_status(context: &Context, id: &str) -> JsResult<Option<String>> {
    let reactions: Ref<'_, ReactionRegistry<ScriptCallable>> = active_registry(context)?.borrow();
    let Some(entry) = reactions.get(id) else {
        return Ok(None);
    };
    serde_json::to_string(&serde_json::json!({
        "id": entry.definition().id.as_str(),
        "enabled": entry.enabled(),
        "triggered": entry.triggered(),
        "tags": entry.definition().tags,
    }))
    .map(Some)
    .map_err(bridge_error)
}

fn active_registry(context: &Context) -> JsResult<&Rc<RefCell<ReactionRegistry<ScriptCallable>>>> {
    context
        .get_data::<ActiveReactions>()
        .map(|active: &ActiveReactions| &active.registry)
        .ok_or_else(|| bridge_error("Reaction bridge 未安装"))
}

fn string_argument(arguments: &[JsValue], context: &mut Context) -> JsResult<String> {
    arguments
        .get_or_undefined(0)
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
}

fn default_true() -> bool {
    true
}

fn bridge_error(error: impl std::fmt::Display) -> boa_engine::JsError {
    JsNativeError::typ().with_message(error.to_string()).into()
}

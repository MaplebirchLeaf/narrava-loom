//! Boa `Reaction` API 到 Core Reaction Registry 的边界转换。

use std::cell::{Ref, RefCell};

use boa_engine::{
    Context, Finalize, JsArgs, JsData, JsNativeError, JsResult, JsString, JsValue, NativeFunction,
    Trace, js_string,
};
use narrava_loom_core::{
    expression::value::{ScriptCallable, Value},
    reaction::{
        PassageMatcher, PassageSelector, PassageTagSelector, ReactionDefinition, ReactionEffect,
        ReactionId, ReactionRegistry, ReactionTrigger, StatePath,
    },
};
use serde::Deserialize;

use super::json_to_value;

#[derive(Trace, Finalize, JsData)]
pub(super) struct ActiveReactions {
    #[unsafe_ignore_trace]
    pub(super) registry: RefCell<ReactionRegistry<ScriptCallable>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DefinitionDto {
    id: String,
    event: Option<String>,
    state: Option<String>,
    #[serde(default)]
    lifecycle: bool,
    passage: Option<PassageDto>,
    cond: Option<CallableDto>,
    widget: Option<String>,
    include: Option<String>,
    replace: Option<String>,
    goto: Option<String>,
    emit: Option<EmitDto>,
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
struct CallableDto {
    #[serde(rename = "__narravaCallable")]
    id: u64,
    name: String,
}

#[derive(Deserialize)]
struct EmitDto {
    name: String,
    #[serde(default)]
    payload: serde_json::Value,
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
    Regex { regex: String },
}

pub(super) fn install(context: &mut Context) -> JsResult<()> {
    context.insert_data(ActiveReactions {
        registry: RefCell::new(ReactionRegistry::new()),
    });
    register(context, "__narravaReactionAdd", reaction_add)?;
    register(context, "__narravaReactionGet", reaction_get)?;
    register(context, "__narravaReactionEnable", reaction_enable)?;
    register(context, "__narravaReactionDisable", reaction_disable)?;
    register(context, "__narravaReactionReset", reaction_reset)
}

pub(super) fn get(context: &Context) -> JsResult<Ref<'_, ReactionRegistry<ScriptCallable>>> {
    Ok(slot(context)?.borrow())
}

fn register(
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
    let dto: DefinitionDto = serde_json::from_str(&argument(arguments, context)?).map_err(fail)?;
    let (definition, condition): (ReactionDefinition, Option<ScriptCallable>) = convert(dto)?;
    let id: String = definition.id.as_str().to_owned();
    slot(context)?
        .borrow_mut()
        .add(definition, condition)
        .map_err(fail)?;
    Ok(JsValue::new(JsString::from(status(context, &id)?.unwrap())))
}

fn reaction_get(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let id: String = argument(arguments, context)?;
    Ok(status(context, &id)?
        .map(|value: String| JsValue::new(JsString::from(value)))
        .unwrap_or_else(JsValue::undefined))
}

fn reaction_enable(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    mutate(arguments, context, ReactionRegistry::enable)
}

fn reaction_disable(
    _: &JsValue,
    arguments: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    mutate(arguments, context, ReactionRegistry::disable)
}

fn reaction_reset(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    mutate(arguments, context, ReactionRegistry::reset)
}

fn mutate(
    arguments: &[JsValue],
    context: &mut Context,
    operation: fn(
        &mut ReactionRegistry<ScriptCallable>,
        &str,
    ) -> Result<bool, narrava_loom_core::reaction::ReactionError>,
) -> JsResult<JsValue> {
    let id: String = argument(arguments, context)?;
    operation(&mut slot(context)?.borrow_mut(), &id)
        .map(JsValue::new)
        .map_err(fail)
}

fn convert(dto: DefinitionDto) -> JsResult<(ReactionDefinition, Option<ScriptCallable>)> {
    let mut triggers: Vec<ReactionTrigger> = Vec::new();
    if let Some(event) = dto.event {
        triggers.push(ReactionTrigger::Event(event));
    }
    if let Some(state) = dto.state {
        triggers.push(ReactionTrigger::State(
            StatePath::parse(state).map_err(fail)?,
        ));
    }
    if dto.lifecycle {
        triggers.push(ReactionTrigger::Lifecycle);
    }
    if triggers.len() != 1 {
        return Err(fail("Reaction 必须且只能声明 event、state、lifecycle 之一"));
    }
    let condition: Option<ScriptCallable> = dto
        .cond
        .map(|callable: CallableDto| ScriptCallable::new(callable.id, callable.name));
    let emit: Option<(String, Value)> = dto
        .emit
        .map(|emit: EmitDto| {
            json_to_value(&emit.payload)
                .map(|payload: Value| (emit.name, payload))
                .map_err(|error| fail(error.message))
        })
        .transpose()?;
    Ok((
        ReactionDefinition {
            id: ReactionId::parse(dto.id).map_err(fail)?,
            trigger: triggers.pop().unwrap(),
            passage: dto.passage.map(convert_passage).transpose()?,
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
        condition,
    ))
}

fn convert_passage(dto: PassageDto) -> JsResult<PassageSelector> {
    Ok(PassageSelector {
        matches: dto
            .matches
            .into_iter()
            .map(convert_matcher)
            .collect::<JsResult<Vec<PassageMatcher>>>()?,
        excludes: dto
            .excludes
            .into_iter()
            .map(convert_matcher)
            .collect::<JsResult<Vec<PassageMatcher>>>()?,
        tags: PassageTagSelector {
            any: dto.tags.any,
            all: dto.tags.all,
            none: dto.tags.none,
        },
    })
}

fn convert_matcher(dto: MatcherDto) -> JsResult<PassageMatcher> {
    match dto {
        MatcherDto::Exact { exact } => PassageMatcher::exact(exact).map_err(fail),
        MatcherDto::Regex { regex } => PassageMatcher::regex(regex).map_err(fail),
    }
}

fn status(context: &Context, id: &str) -> JsResult<Option<String>> {
    let reactions: Ref<'_, ReactionRegistry<ScriptCallable>> = get(context)?;
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
    .map_err(fail)
}

fn slot(context: &Context) -> JsResult<&RefCell<ReactionRegistry<ScriptCallable>>> {
    context
        .get_data::<ActiveReactions>()
        .map(|active: &ActiveReactions| &active.registry)
        .ok_or_else(|| fail("Reaction bridge 未安装"))
}

fn argument(arguments: &[JsValue], context: &mut Context) -> JsResult<String> {
    arguments
        .get_or_undefined(0)
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
}

fn default_true() -> bool {
    true
}

fn fail(error: impl std::fmt::Display) -> boa_engine::JsError {
    JsNativeError::typ().with_message(error.to_string()).into()
}

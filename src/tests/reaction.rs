use crate::expression::value::Value;
use crate::reaction::{
    PassageMatcher, PassageSelector, ReactionDefinition, ReactionEffect, ReactionId,
    ReactionRegistry, ReactionResolveError, ReactionSuccess, ReactionTrigger, StatePath,
    resolve_event_queue, resolve_lifecycle_reactions, resolve_state_changes,
};
use crate::state::State;
use crate::{hir::HirPassage, source::SourcePath};

fn event_definition(id: &str, event: &str) -> ReactionDefinition {
    ReactionDefinition {
        id: ReactionId::parse(id).unwrap(),
        trigger: ReactionTrigger::Event(event.to_owned()),
        passage: None,
        effect: ReactionEffect {
            emit: Some((
                String::from("reaction:fired"),
                crate::expression::value::Value::Null,
            )),
            ..ReactionEffect::default()
        },
        enabled: true,
        once: false,
        limit: None,
        tags: Vec::new(),
    }
}

#[test]
fn event_resolver_preserves_order_payload_conditions_and_success_counts() {
    let mut registry: ReactionRegistry<&'static str> = ReactionRegistry::new();
    let first: ReactionDefinition = event_definition("first", "quest:completed");
    let mut second: ReactionDefinition = event_definition("second", "quest:completed");
    second.limit = Some(1);
    registry.add(first, Some("accept")).unwrap();
    registry.add(second, Some("reject")).unwrap();
    let mut effects: Vec<String> = Vec::new();

    let result = resolve_event_queue(
        &mut registry,
        [(String::from("quest:completed"), Value::Number(7.0))],
        8,
        |condition, payload| {
            Ok::<bool, ()>(condition == Some(&"accept") && payload == &Value::Number(7.0))
        },
        |id, _effect| {
            effects.push(id.as_str().to_owned());
            Ok::<(), ()>(())
        },
    )
    .unwrap();

    assert_eq!(effects, ["first"]);
    assert_eq!(result.triggered, [ReactionId::parse("first").unwrap()]);
    assert_eq!(registry.get("first").unwrap().triggered(), 1);
    assert_eq!(registry.get("second").unwrap().triggered(), 0);
}

#[test]
fn event_resolver_queues_emitted_events_and_rejects_a_descendant_cycle() {
    let mut registry: ReactionRegistry<()> = ReactionRegistry::new();
    let mut first: ReactionDefinition = event_definition("first", "event:a");
    first.effect.emit = Some((String::from("event:b"), Value::Null));
    let mut second: ReactionDefinition = event_definition("second", "event:b");
    second.effect.emit = Some((String::from("event:a"), Value::Null));
    registry.add(first, None).unwrap();
    registry.add(second, None).unwrap();

    let error = resolve_event_queue(
        &mut registry,
        [(String::from("event:a"), Value::Null)],
        8,
        |_condition, _payload| Ok::<bool, ()>(true),
        |_id, _effect| Ok::<(), ()>(()),
    )
    .unwrap_err();

    assert_eq!(
        error,
        ReactionResolveError::EventCycle {
            event: String::from("event:a"),
            reaction: ReactionId::parse("first").unwrap(),
        }
    );
}

#[test]
fn event_resolver_does_not_count_a_failed_effect() {
    let mut registry: ReactionRegistry<()> = ReactionRegistry::new();
    registry
        .add(event_definition("first", "event:a"), None)
        .unwrap();

    let error = resolve_event_queue(
        &mut registry,
        [(String::from("event:a"), Value::Null)],
        8,
        |_condition, _payload| Ok::<bool, &str>(true),
        |_id, _effect| Err::<(), &str>("effect failed"),
    )
    .unwrap_err();

    assert_eq!(error, ReactionResolveError::Operation("effect failed"));
    assert_eq!(registry.get("first").unwrap().triggered(), 0);
}

#[test]
fn registry_keeps_state_paths_in_first_registration_order() {
    let mut registry: ReactionRegistry<()> = ReactionRegistry::new();
    for (id, path) in [
        ("gold-first", "$inventory.gold"),
        ("stage", "$quest.stage"),
        ("gold-second", "$inventory.gold"),
    ] {
        let mut definition = event_definition(id, "unused");
        definition.trigger = ReactionTrigger::State(StatePath::parse(path).unwrap());
        registry.add(definition, None).unwrap();
    }

    assert_eq!(
        registry
            .state_paths()
            .iter()
            .map(StatePath::as_str)
            .collect::<Vec<_>>(),
        ["$inventory.gold", "$quest.stage"]
    );
}

#[test]
fn state_resolver_passes_before_after_and_continues_emitted_event_chain() {
    let mut registry: ReactionRegistry<&'static str> = ReactionRegistry::new();
    let mut state_rule = event_definition("friendship", "unused");
    state_rule.trigger = ReactionTrigger::State(StatePath::parse("$alice.affection").unwrap());
    state_rule.effect.emit = Some((String::from("alice:friendship"), Value::Null));
    registry.add(state_rule, Some("crossed")).unwrap();
    registry
        .add(event_definition("notice", "alice:friendship"), None)
        .unwrap();
    let mut state: State = State::new();
    state.variables_set(
        "alice",
        Value::object(vec![(String::from("affection"), Value::Number(40.0))]),
    );
    let before = state.snapshot();
    state.variables_set(
        "alice",
        Value::object(vec![(String::from("affection"), Value::Number(50.0))]),
    );
    let after = state.snapshot();
    let mut arguments: Vec<Value> = Vec::new();

    let resolution = resolve_state_changes(
        &mut registry,
        &before,
        &after,
        8,
        |condition, argument| {
            if condition.is_some() {
                arguments.push(argument.clone());
            }
            Ok::<bool, ()>(true)
        },
        |_id, _effect| Ok::<(), ()>(()),
    )
    .unwrap();

    assert_eq!(
        resolution
            .triggered
            .iter()
            .map(ReactionId::as_str)
            .collect::<Vec<_>>(),
        ["friendship", "notice"]
    );
    let Value::Object(argument) = &arguments[0] else {
        unreachable!("State cond 参数应为 Object")
    };
    assert_eq!(argument.get("before"), Some(Value::Number(40.0)));
    assert_eq!(argument.get("after"), Some(Value::Number(50.0)));
}

#[test]
fn lifecycle_resolver_applies_passage_selector_before_dynamic_condition() {
    let mut registry: ReactionRegistry<()> = ReactionRegistry::new();
    let mut definition = event_definition("tavern", "unused");
    definition.trigger = ReactionTrigger::Lifecycle;
    definition.passage = Some(PassageSelector {
        matches: vec![PassageMatcher::exact("Tavern").unwrap()],
        ..PassageSelector::default()
    });
    registry.add(definition, None).unwrap();
    let source = SourcePath::fragment();
    let passage = HirPassage {
        source: &source,
        name: "Tavern",
        tags: vec!["indoor"],
        body: Vec::new(),
    };

    let resolution = resolve_lifecycle_reactions(
        &mut registry,
        &passage,
        8,
        |_condition, _argument| Ok::<bool, ()>(true),
        |_id, _effect| Ok::<(), ()>(()),
    )
    .unwrap();

    assert_eq!(resolution.triggered, [ReactionId::parse("tavern").unwrap()]);
}

#[test]
fn registry_indexes_candidates_in_registration_order_and_honors_enabled_state() {
    let mut registry: ReactionRegistry<()> = ReactionRegistry::new();
    registry
        .add(event_definition("first", "quest:completed"), None)
        .unwrap();
    registry
        .add(event_definition("second", "quest:completed"), None)
        .unwrap();

    assert_eq!(
        registry.event_candidates("quest:completed"),
        vec![
            ReactionId::parse("first").unwrap(),
            ReactionId::parse("second").unwrap()
        ]
    );
    assert!(registry.disable("first").unwrap());
    assert_eq!(
        registry.event_candidates("quest:completed"),
        vec![ReactionId::parse("second").unwrap()]
    );
    assert!(!registry.disable("first").unwrap());
    assert!(registry.enable("first").unwrap());
}

#[test]
fn once_destroys_definition_while_limit_disables_and_reset_reactivates_it() {
    let mut registry: ReactionRegistry<()> = ReactionRegistry::new();
    let mut once: ReactionDefinition = event_definition("once", "talk");
    once.once = true;
    registry.add(once, None).unwrap();
    assert_eq!(
        registry.record_success("once").unwrap(),
        ReactionSuccess::Destroyed
    );
    assert!(registry.get("once").is_none());

    let mut limited: ReactionDefinition = event_definition("limited", "talk");
    limited.limit = Some(2);
    registry.add(limited, None).unwrap();
    assert_eq!(
        registry.record_success("limited").unwrap(),
        ReactionSuccess::Active
    );
    assert_eq!(
        registry.record_success("limited").unwrap(),
        ReactionSuccess::Disabled
    );
    assert!(!registry.get("limited").unwrap().enabled());
    assert!(registry.reset("limited").unwrap());
    assert!(registry.get("limited").unwrap().enabled());
    assert_eq!(registry.get("limited").unwrap().triggered(), 0);
}

#[test]
fn reset_reports_restoring_a_rule_to_its_initial_disabled_state() {
    let mut registry: ReactionRegistry<()> = ReactionRegistry::new();
    let mut definition: ReactionDefinition = event_definition("manual", "talk");
    definition.enabled = false;
    registry.add(definition, None).unwrap();

    assert!(registry.enable("manual").unwrap());
    assert!(registry.reset("manual").unwrap());
    assert!(!registry.get("manual").unwrap().enabled());
    assert!(!registry.reset("manual").unwrap());
}

#[test]
fn runtime_state_restores_once_tombstones_and_limit_counters() {
    let mut source: ReactionRegistry<()> = ReactionRegistry::new();
    let mut once = event_definition("once", "talk");
    once.once = true;
    source.add(once.clone(), None).unwrap();
    let mut limited = event_definition("limited", "talk");
    limited.limit = Some(2);
    source.add(limited.clone(), None).unwrap();
    source.record_success("once").unwrap();
    source.record_success("limited").unwrap();
    let saved = source.runtime_state();

    let mut restored: ReactionRegistry<()> = ReactionRegistry::new();
    restored.add(once, None).unwrap();
    restored.add(limited, None).unwrap();
    restored.restore_runtime_state(&saved).unwrap();

    assert!(restored.get("once").is_none());
    assert_eq!(restored.get("limited").unwrap().triggered(), 1);
    assert!(restored.get("limited").unwrap().enabled());
}

#[test]
fn registration_rejects_ambiguous_or_context_invalid_contracts() {
    let mut registry: ReactionRegistry<()> = ReactionRegistry::new();
    let mut exit: ReactionDefinition = event_definition("bad.exit", "talk");
    exit.effect.exit = true;
    assert!(registry.add(exit, None).is_err());

    let mut replace: ReactionDefinition = event_definition("bad.replace", "talk");
    replace.effect.replace = Some(String::from("panel"));
    assert!(registry.add(replace, None).is_err());

    let mut include: ReactionDefinition = event_definition("bad.include", "talk");
    include.effect.emit = None;
    include.effect.include = Some(String::from("Notice"));
    assert_eq!(
        registry.add(include, None),
        Err(crate::reaction::ReactionError::IncludeWithoutReplace)
    );

    let mut limit: ReactionDefinition = event_definition("bad.limit", "talk");
    limit.limit = Some(0);
    assert!(registry.add(limit, None).is_err());

    assert!(StatePath::parse("alice.affection").is_err());
    assert!(StatePath::parse("$alice..affection").is_err());
    assert_eq!(
        StatePath::parse("$alice.affection").unwrap().as_str(),
        "$alice.affection"
    );
}

use crate::reaction::{
    ReactionDefinition, ReactionEffect, ReactionId, ReactionRegistry, ReactionSuccess,
    ReactionTrigger, StatePath,
};

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
fn registration_rejects_ambiguous_or_context_invalid_contracts() {
    let mut registry: ReactionRegistry<()> = ReactionRegistry::new();
    let mut exit: ReactionDefinition = event_definition("bad.exit", "talk");
    exit.effect.exit = true;
    assert!(registry.add(exit, None).is_err());

    let mut replace: ReactionDefinition = event_definition("bad.replace", "talk");
    replace.effect.replace = Some(String::from("panel"));
    assert!(registry.add(replace, None).is_err());

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

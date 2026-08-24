use crate::{
    events::{Event, EventError, EventFilter},
    expression::value::{ScriptCallable, Value},
};

#[test]
fn events_keep_order_and_deliver_only_to_existing_matching_subscriptions() {
    let mut events = Event::new();
    events
        .emit("story:start", Value::Number(1.0))
        .expect("事件应可发布");
    let all = events.subscribe(EventFilter::default());
    let story = events.subscribe(EventFilter::named("story:start"));

    events
        .emit("story:start", Value::Number(2.0))
        .expect("事件应可发布");
    events
        .emit("save:complete", Value::Null)
        .expect("事件应可发布");

    let all_pending = events.take(all).expect("订阅应存在");
    assert_eq!(all_pending.len(), 2);
    assert_eq!(all_pending[0].sequence().get(), 2);
    assert_eq!(all_pending[1].sequence().get(), 3);
    let story_pending = events.take(story).expect("订阅应存在");
    assert_eq!(story_pending.len(), 1);
    assert_eq!(story_pending[0].name(), "story:start");
    assert_eq!(events.records().len(), 3);
}

#[test]
fn events_take_unsubscribe_and_clear_have_explicit_ownership() {
    let mut events = Event::new();
    let subscription = events.subscribe(EventFilter::default());
    events.emit("one", Value::Null).unwrap();
    assert_eq!(events.take(subscription).unwrap().len(), 1);
    assert!(events.take(subscription).unwrap().is_empty());

    events.emit("two", Value::Null).unwrap();
    events.clear();
    assert!(events.records().is_empty());
    assert!(events.take(subscription).unwrap().is_empty());
    events.emit("three", Value::Null).unwrap();
    assert_eq!(events.records()[0].sequence().get(), 3);

    assert!(events.unsubscribe(subscription));
    assert!(!events.unsubscribe(subscription));
    assert!(events.take(subscription).is_none());
}

#[test]
fn events_reject_invalid_names_and_platform_callable_payloads() {
    let mut events = Event::new();
    assert_eq!(
        events.emit("", Value::Null).unwrap_err(),
        EventError::InvalidName(String::new())
    );
    assert_eq!(
        events
            .emit("bad name", Value::Null)
            .expect_err("事件名称不能含空白"),
        EventError::InvalidName(String::from("bad name"))
    );

    let payload = Value::array(vec![Value::ScriptCallable(ScriptCallable::new(
        1, "handler",
    ))]);
    assert_eq!(
        events.emit("runtime:value", payload).unwrap_err(),
        EventError::UnsupportedValue(String::from("payload[0]"))
    );
    assert!(events.records().is_empty());
}

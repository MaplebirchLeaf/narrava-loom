//! 真实 Boa Logger 桥接、容量和独立于 State 回滚的日志。

use crate::EcmaBinding;
use boa_engine::{JsValue, Source};
use narrava_loom_core::{
    SourceList,
    diagnostic::{Diagnostic, DiagnosticLocator, DiagnosticSeverity},
    expression::value::Value,
    i18n::I18nCatalog,
    resource::ResourceCatalog,
    state::State,
};
use narrava_loom_protocol::{DiagnosticSeverityDto, HostLogLevelDto};
use std::rc::Rc;

fn binding() -> Rc<EcmaBinding> {
    EcmaBinding::load(
        &SourceList { items: Vec::new() },
        &ResourceCatalog::default(),
        &I18nCatalog::default(),
        "en",
        &mut State::new(),
    )
    .unwrap()
}

fn evaluate(binding: &EcmaBinding, source: &str) -> serde_json::Value {
    let mut runtime = binding.runtime.borrow_mut();
    let value: JsValue = runtime.context.eval(Source::from_bytes(source)).unwrap();
    value
        .to_json(&mut runtime.context)
        .unwrap()
        .unwrap_or(serde_json::Value::Null)
}

#[test]
fn native_logger_filters_drains_and_removes_real_subscriptions() {
    let binding: Rc<EcmaBinding> = binding();
    let value = evaluate(
        &binding,
        r#"
        Logger.info('game', 'before');
        const selected = Logger.subscribe({minimumLevel:'warn', target:'game'});
        Logger.debug('game', 'quiet');
        Logger.error('other', 'different');
        Logger.warn('game', 'first');
        Logger.error('game', 'second');
        const records = Logger.take(selected);
        records[0].message = 'local copy';
        ({records, empty:Logger.take(selected), removed:Logger.unsubscribe(selected),
          missing:Logger.take(selected) === undefined, removedAgain:Logger.unsubscribe(selected)})
    "#,
    );
    assert_eq!(value["records"][0]["sequence"], 4);
    assert_eq!(value["records"][1]["message"], "second");
    assert_eq!(value["empty"], serde_json::json!([]));
    assert_eq!(value["removed"], true);
    assert_eq!(value["missing"], true);
    assert_eq!(value["removedAgain"], false);
    let records = binding.log_records();
    assert_eq!(records.len(), 5);
    assert_eq!(records[3].message, "first");
    assert_eq!(records[3].level, HostLogLevelDto::Warn);
}

#[test]
fn native_logger_bounds_history_and_each_unread_queue() {
    let binding: Rc<EcmaBinding> = binding();
    let value = evaluate(
        &binding,
        r#"
        const all = Logger.subscribe();
        for(let i=0;i<1030;i++) Logger.info('game', String(i));
        const records = Logger.take(all);
        ({length:records.length, first:records[0].sequence, last:records.at(-1).sequence})
    "#,
    );
    assert_eq!(
        value,
        serde_json::json!({"length":1024,"first":7,"last":1030})
    );
    let records = binding.log_records();
    assert_eq!(records.len(), 1024);
    assert_eq!(records[0].sequence, 7);
}

#[test]
fn native_logger_rejects_invalid_filters_and_handles() {
    let binding: Rc<EcmaBinding> = binding();
    for source in [
        "Logger.subscribe({minimumLevel:'fatal'})",
        "Logger.subscribe({extra:true})",
        "Logger.subscribe({target:3})",
        "Logger.info({}, 'message')",
        "Logger.warn('game', 3)",
        "Logger.take(-1)",
        "Logger.take(1.5)",
        "Logger.unsubscribe(2**53)",
    ] {
        assert!(
            binding
                .runtime
                .borrow_mut()
                .context
                .eval(Source::from_bytes(source))
                .is_err(),
            "{source}"
        );
    }
    assert!(binding.log_records().is_empty());
}

#[test]
fn failed_script_logs_survive_state_rollback_and_share_host_diagnostics() {
    let binding: Rc<EcmaBinding> = binding();
    evaluate(
        &binding,
        "Macro.add('fail', {handler: () => { V.coins=9; Logger.error('quest', 'before failure'); throw Error('quest rejected'); }})",
    );
    let mut state: State = State::new();
    state.variables_set("coins", Value::Number(1.0));
    let checkpoint = state.checkpoint();
    let error = binding.call_macro("fail", "", &mut state).unwrap_err();
    assert!(error.message.contains("quest rejected"));
    state.restore_checkpoint(checkpoint);
    let location = DiagnosticLocator::new("story/main.twee", "<<fail>>")
        .locate(0, 0, 8)
        .unwrap();
    binding.record_diagnostic(
        &Diagnostic::new(
            "quest.rejected",
            DiagnosticSeverity::Error,
            "quest rejected",
        )
        .with_location(location),
    );
    assert_eq!(state.variables_get("coins"), Some(&Value::Number(1.0)));
    let records = binding.log_records();
    assert_eq!(records[0].target, "quest");
    assert_eq!(
        records[1].diagnostic.as_ref().unwrap().severity,
        Some(DiagnosticSeverityDto::Error)
    );
    let location = records[1]
        .diagnostic
        .as_ref()
        .unwrap()
        .location
        .as_ref()
        .unwrap();
    assert_eq!(location.source, "story/main.twee");
    assert_eq!(location.line, Some(1));
    assert!(!location.generated);
}

//! 原始源码位置与生成脚本位置必须保持区别。

use crate::{
    EcmaBinding, ScriptError,
    protocol_adapter::{core_diagnostic, diagnostic},
    transpile,
};
use narrava_loom_core::{
    SourceList,
    diagnostic::{Diagnostic, DiagnosticLocator, DiagnosticSeverity},
    i18n::I18nCatalog,
    resource::ResourceCatalog,
    state::State,
};
use narrava_loom_protocol::{DiagnosticSeverityDto, HostErrorDto};
use std::{
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

fn load(path: &str, source: &str) -> Result<Rc<EcmaBinding>, ScriptError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root: PathBuf = PathBuf::from(format!(
        "target/test-projects/script-diagnostic-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let script: PathBuf = root.join("contents").join(path);
    std::fs::create_dir_all(script.parent().unwrap()).unwrap();
    std::fs::write(&script, source).unwrap();
    let sources: SourceList = SourceList::discover(&root).unwrap();
    let result = EcmaBinding::load(
        &sources,
        &ResourceCatalog::default(),
        &I18nCatalog::default(),
        "en",
        &mut State::new(),
    );
    std::fs::remove_dir_all(root).unwrap();
    result
}

#[test]
fn original_diagnostic_location_survives_host_and_core_round_trip() {
    let location = DiagnosticLocator::new("story/main.twee", "第一行\n<<bad>>")
        .locate(0, 10, 17)
        .unwrap();
    let original = Diagnostic::new("macro.bad", DiagnosticSeverity::Warning, "bad macro")
        .with_location(location);
    let host: HostErrorDto = diagnostic(original.clone());
    assert_eq!(host.severity, Some(DiagnosticSeverityDto::Warning));
    assert_eq!(host.location.as_ref().unwrap().line, Some(2));
    assert!(!host.location.as_ref().unwrap().generated);
    assert_eq!(core_diagnostic(&host), original);
    let legacy: HostErrorDto =
        serde_json::from_str(r#"{"code":"legacy","message":"old error"}"#).unwrap();
    assert!(legacy.location.is_none());
    assert!(legacy.severity.is_none());
}

#[test]
fn typescript_parser_error_retains_original_file_and_line() {
    let error: ScriptError = transpile(
        "scripts/broken.ts",
        "// original line\nconst count: number = ;\n",
    )
    .unwrap_err();
    let location = error.location.expect("Oxc 原始标签应提供位置");
    assert_eq!(location.source, "scripts/broken.ts");
    assert_eq!(location.line, Some(2));
    assert!(location.start.is_some());
    assert!(!location.generated);
}

#[test]
fn loop_limit_diagnostic_does_not_attempt_to_create_a_javascript_error_object() {
    let mut context = crate::ecma::runtime_context(3);
    let error = context
        .eval(boa_engine::Source::from_bytes("while(true) {}"))
        .unwrap_err();
    let diagnostic = crate::binding::diagnostics::js_error(
        &mut context,
        "script.load",
        error,
        Some("scripts/loop.js"),
    );
    assert!(diagnostic.message.contains("loop"), "{diagnostic:?}");
    assert_eq!(diagnostic.location.unwrap().source, "scripts/loop.js");
}

#[test]
fn load_error_reports_the_actual_script_path_and_exception() {
    let result = load(
        "scripts/broken.ts",
        "type Removed = number;\nthrow new Error('author explosion');",
    );
    let error: ScriptError = match result {
        Ok(_) => panic!("脚本应失败"),
        Err(error) => error,
    };
    assert!(
        error.message.contains("author explosion"),
        "{}",
        error.message
    );
    let location = error.location.expect("装载入口知道当前文件");
    assert_eq!(location.source, "scripts/broken.ts");
    assert!(location.generated);
    assert!(location.start.is_none());
    assert!(location.end.is_none());
}

#[test]
fn later_macro_exception_preserves_origin_without_claiming_original_ts_coordinates() {
    let binding = load("scripts/quest.ts", "type Removed = number;\nMacro.add('broken', {handler: () => { throw new Error('late explosion'); }});").unwrap();
    let error = binding
        .call_macro("broken", "", &mut State::new())
        .unwrap_err();
    assert!(error.message.contains("late explosion"));
    let location = error.location.expect("Boa 执行栈应保留定义文件");
    assert_eq!(location.source, "scripts/quest.ts");
    assert!(location.generated);
    assert!(location.start.is_none());
}

#[test]
fn twee_call_preserves_script_exception_and_source() {
    super::support::with_runtime(
        ":: Start\n<<print explode()>>",
        "State.global.set('explode', () => { throw new Error('function explosion'); });",
        |runtime| {
            let error = runtime
                .execute(narrava_loom_protocol::RuntimeCommand::Start)
                .unwrap_err();
            assert!(error.message.contains("function explosion"), "{error:?}");
            assert_eq!(error.location.as_ref().unwrap().source, "scripts/main.js");
        },
    );
}

#[test]
fn reaction_condition_retains_script_exception_and_source() {
    let binding = load("scripts/reaction.ts", "Reaction.add({id:'broken',event:'quest',cond:()=>{throw new Error('condition explosion')},widget:'broken'}); Event.emit('quest');").unwrap();
    let error = binding
        .resolve_queued_event_reactions(None, &mut State::new())
        .unwrap_err();
    assert!(error.message.contains("condition explosion"), "{error:?}");
    assert_eq!(
        error.location.as_ref().unwrap().source,
        "scripts/reaction.ts"
    );
}

#[test]
fn session_macro_errors_keep_origin_across_initial_and_pending_dispatch() {
    use super::support::{pending, with_runtime};
    use narrava_loom_protocol::RuntimeCommand;
    for delayed in [false, true] {
        let body: &str = if delayed {
            "<<pause>><<broken>>"
        } else {
            "<<broken>>"
        };
        with_runtime(
            &format!(":: Start\n{body}"),
            "Macro.add('pause', {handler:async()=>{await Host.delay(1)}}); Macro.add('broken', {handler:()=>{throw Error('dispatch explosion')}});",
            |runtime| {
                let result = runtime.execute(RuntimeCommand::Start);
                let error: HostErrorDto = if delayed {
                    let operation: u64 = pending(result.unwrap()).id();
                    runtime
                        .execute(RuntimeCommand::Resume {
                            operation,
                            result: None,
                        })
                        .unwrap_err()
                } else {
                    result.unwrap_err()
                };
                assert!(error.message.contains("dispatch explosion"), "{error:?}");
                assert_eq!(error.location.as_ref().unwrap().source, "scripts/main.js");
                let logs = runtime.log_records();
                assert_eq!(logs.last().unwrap().diagnostic.as_ref(), Some(&error));
            },
        );
    }
}

#[test]
fn session_action_macro_failure_keeps_origin_and_old_frame() {
    use super::support::{action, ready, with_runtime};
    use narrava_loom_protocol::RuntimeCommand;
    with_runtime(
        ":: Start\n<<link 'break'>><<broken>><</link>>",
        "Macro.add('broken', {handler:()=>{throw Error('action explosion')}});",
        |runtime| {
            let (_, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
            let interaction: String = action(&start, "break");
            let error: HostErrorDto = runtime
                .execute(RuntimeCommand::Activate {
                    interaction: interaction.clone(),
                })
                .unwrap_err();
            assert!(error.message.contains("action explosion"), "{error:?}");
            assert_eq!(error.location.as_ref().unwrap().source, "scripts/main.js");
            assert_eq!(
                runtime.debug_snapshot().unwrap().current.as_deref(),
                Some("Start")
            );
            assert_eq!(
                runtime
                    .execute(RuntimeCommand::Activate { interaction })
                    .unwrap_err(),
                error,
                "失败回滚后原 action 身份必须仍然可用"
            );
        },
    );
}

#[test]
fn session_async_macro_rejection_retains_origin() {
    use super::support::{action, pending, ready, with_runtime};
    use narrava_loom_protocol::RuntimeCommand;
    for action_body in [false, true] {
        let story: &str = if action_body {
            ":: Start\n<<link 'break'>><<broken>><</link>>"
        } else {
            ":: Start\n<<broken>>"
        };
        with_runtime(
            story,
            "Macro.add('broken', {handler:async()=>{await Host.delay(1); throw Error('async explosion')}});",
            |runtime| {
                let update = runtime.execute(RuntimeCommand::Start).unwrap();
                let operation: u64 = if action_body {
                    let (_, frame) = ready(update);
                    let interaction: String = action(&frame, "break");
                    pending(
                        runtime
                            .execute(RuntimeCommand::Activate { interaction })
                            .unwrap(),
                    )
                    .id()
                } else {
                    pending(update).id()
                };
                let error = runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: None,
                    })
                    .unwrap_err();
                assert!(error.message.contains("async explosion"), "{error:?}");
                assert_eq!(error.location.as_ref().unwrap().source, "scripts/main.js");
            },
        );
    }
}

//! Engine 根种子统一驱动脚本与 Twee，执行进度随游戏事务恢复。
use super::support::{
    action, export_save, import_save, navigate, pending, ready, text, with_runtime,
    with_seeded_runtime,
};
use crate::RuntimeSession;
use narrava_loom_core::engine::Engine;
use narrava_loom_protocol::{HostDebugSnapshotDto, RuntimeCommand};
use narrava_loom_protocol::{HostUpdateDto, PendingOperation, PendingResult, RuntimeUpdate};

#[test]
fn ordinary_random_works_in_script_and_twee_without_seed_api() {
    with_runtime(
        ":: Start\n<<set $choice = either(10, 20)>><<set $unit = random()>>\n",
        "V.roll = Math.random();",
        |runtime| {
            ready(runtime.execute(RuntimeCommand::Start).unwrap());
            let before: HostDebugSnapshotDto = runtime.debug_snapshot().unwrap();
            for key in ["roll", "unit"] {
                let unit: f64 = before.state["variables"][key].as_f64().unwrap();
                assert!((0.0..1.0).contains(&unit));
            }
            let choice: f64 = before.state["variables"]["choice"].as_f64().unwrap();
            assert!(choice == 10.0 || choice == 20.0);
            runtime
                .execute(RuntimeCommand::DebugScript {
                    source: "typeof Random".into(),
                })
                .unwrap();
            assert_eq!(
                runtime
                    .debug_snapshot()
                    .unwrap()
                    .evaluation
                    .unwrap()
                    .value
                    .preview,
                "\"undefined\""
            );
            assert!(runtime.console_completions("Random").unwrap().is_empty());
            let bytes: Vec<u8> = export_save(runtime);
            runtime
                .execute(RuntimeCommand::DebugScript {
                    source: "V.roll = -1".into(),
                })
                .unwrap();
            import_save(runtime, bytes);
            assert_eq!(
                runtime.debug_snapshot().unwrap().state["variables"]["roll"],
                before.state["variables"]["roll"]
            );
        },
    );
}

#[test]
fn engine_root_seed_is_available_before_scripts_and_drives_twee() {
    super::support::with_seeded_runtime(
        ":: Start\n<<set $third = random()>><<set $choice = either(10,20)>>\n",
        "V.seed = Engine.seed; V.first = Math.random(); V.second = Math.random();",
        u64::MAX,
        |runtime| {
            ready(runtime.execute(RuntimeCommand::Start).unwrap());
            let values = runtime.debug_snapshot().unwrap().state["variables"].clone();
            let expected = narrava_loom_core::engine::Engine::new(u64::MAX);
            assert_eq!(values["seed"], u64::MAX.to_string());
            for name in ["first", "second", "third"] {
                assert_eq!(values[name].as_f64(), Some(expected.next_random()));
            }
            assert_eq!(
                values["choice"].as_f64(),
                Some(if expected.next_random() < 0.5 {
                    10.0
                } else {
                    20.0
                })
            );
            runtime
                .execute(RuntimeCommand::DebugScript {
                    source:
                        "Reflect.set(Engine, 'seed', '1'); Reflect.set(Math, 'random', () => 0)"
                            .into(),
                })
                .unwrap();
            runtime
                .execute(RuntimeCommand::DebugScript {
                    source: "V.seedAfter = Engine.seed; V.next = Math.random()".into(),
                })
                .unwrap();
            let values = runtime.debug_snapshot().unwrap().state["variables"].clone();
            assert_eq!(values["seedAfter"], u64::MAX.to_string());
            assert_eq!(values["next"].as_f64(), Some(expected.next_random()));
        },
    );
}

const DRAW: &str = r#"
Macro.add('draw', {handler: () => `DRAW=${Math.random()};`});
Macro.add('headerRandom', {handler: () => { Math.random(); return ''; }});
"#;

fn samples(frame: &HostUpdateDto) -> Vec<f64> {
    text(frame)
        .split("DRAW=")
        .skip(1)
        .map(|part: &str| {
            part.split_once(';')
                .expect("抽样标记应闭合")
                .0
                .parse()
                .unwrap()
        })
        .collect()
}

fn activate(
    runtime: &mut RuntimeSession<'_, '_>,
    frame: &HostUpdateDto,
    label: &str,
) -> HostUpdateDto {
    ready(
        runtime
            .execute(RuntimeCommand::Activate {
                interaction: action(frame, label),
            })
            .unwrap(),
    )
    .1
}

fn language_refresh(runtime: &mut RuntimeSession<'_, '_>) -> RuntimeUpdate {
    let PendingOperation::SelectLanguage { operation, .. } = pending(
        runtime
            .execute(RuntimeCommand::SelectLanguage {
                locale: "en".into(),
            })
            .unwrap(),
    ) else {
        panic!("语言切换应等待宿主")
    };
    runtime
        .execute(RuntimeCommand::Resume {
            operation,
            result: Some(PendingResult::SelectLanguage),
        })
        .unwrap()
}

#[test]
fn random_history_replays_body_and_include_while_header_is_isolated() {
    let story: &str = r#":: Start
<<link [[进入|Page]]>><</link>>
:: Page
<<draw>><<include "Details">>
<<link [[下一页|Next]]>><</link>>
:: Details
<<draw>>
:: Next
<<draw>>
:: Header
<<headerRandom>>
"#;
    with_seeded_runtime(story, DRAW, 42, |runtime| {
        let expected: Engine = Engine::new(42);
        let pair: Vec<f64> = vec![expected.next_random(), expected.next_random()];
        let third: f64 = expected.next_random();
        let start: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
        let page: HostUpdateDto = navigate(runtime, &start, "Page").1;
        assert_eq!(samples(&page), pair);
        let next: HostUpdateDto = navigate(runtime, &page, "Next").1;
        assert_eq!(samples(&next), vec![third]);
        let previous: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Back).unwrap()).1;
        assert_eq!(samples(&previous), pair);
        let next: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Forward).unwrap()).1;
        assert_eq!(samples(&next), vec![third]);
    });
}

#[test]
fn random_refresh_replays_visible_draws_and_preserves_action_tail_across_save() {
    let story: &str = r#":: Start
<<link [[进入|Page]]>><</link>>
:: Page
<<draw>>
<<link "抽样">><<draw>><</link>>
<<link [[下一页|Next]]>><</link>>
:: Next
<<draw>>
"#;
    with_seeded_runtime(story, DRAW, 42, |runtime| {
        let expected: Engine = Engine::new(42);
        let first: f64 = expected.next_random();
        let second: f64 = expected.next_random();
        let third: f64 = expected.next_random();
        let start: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
        let page: HostUpdateDto = navigate(runtime, &start, "Page").1;
        assert_eq!(samples(&page), vec![first]);
        let action_frame: HostUpdateDto = activate(runtime, &page, "抽样");
        assert_eq!(samples(&action_frame).last(), Some(&second));
        let document: Vec<u8> = export_save(runtime);
        let refreshed: HostUpdateDto = ready(language_refresh(runtime)).1;
        assert_eq!(samples(&refreshed), vec![first]);
        let next: HostUpdateDto = navigate(runtime, &refreshed, "Next").1;
        assert_eq!(samples(&next), vec![third]);
        let restored: HostUpdateDto = import_save(runtime, document).1;
        assert_eq!(samples(&restored), vec![first]);
        let drawn: HostUpdateDto = activate(runtime, &restored, "抽样");
        assert_eq!(samples(&drawn).last(), Some(&third));
    });
}

#[test]
fn random_refresh_pending_resume_cancel_and_error_release_temporary_sequence() {
    for outcome in ["resume", "cancel", "error"] {
        let story: &str = r#":: Start
<<link [[进入|Page]]>><</link>>
:: Page
<<set $phase = 'body'>><<draw>><<pause>><<draw>>
<<link "准备刷新">><<arm>><</link>>
<<link "抽样">><<draw>><</link>>
"#;
        let script: String = format!(
            r#"{DRAW}
let armed = false;
Macro.add('arm', {{handler: () => {{ armed = true; State.variables.set('phase', 'committed'); return `DRAW=${{Math.random()}};`; }} }});
Macro.add('pause', {{handler: async () => {{
    if (!armed) return '';
    armed = false;
    await Host.delay(1);
    if ('{outcome}' === 'error') throw new Error('random refresh failed');
    return '';
}} }});
"#
        );
        with_seeded_runtime(story, &script, 42, |runtime| {
            let expected: Engine = Engine::new(42);
            let body: Vec<f64> = vec![expected.next_random(), expected.next_random()];
            let third: f64 = expected.next_random();
            let fourth: f64 = expected.next_random();
            let start: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
            let page: HostUpdateDto = navigate(runtime, &start, "Page").1;
            assert_eq!(samples(&page), body);
            let armed: HostUpdateDto = activate(runtime, &page, "准备刷新");
            assert_eq!(samples(&armed).last(), Some(&third));
            let PendingOperation::Delay { operation, .. } = pending(language_refresh(runtime))
            else {
                panic!("重绘应在正文挂起")
            };
            let wrong = runtime
                .execute(RuntimeCommand::Resume {
                    operation: operation + 1,
                    result: None,
                })
                .unwrap_err();
            assert_eq!(wrong.code, "runtime_session.operation_mismatch");
            let visible: HostUpdateDto = match outcome {
                "cancel" => {
                    assert!(matches!(
                        runtime
                            .execute(RuntimeCommand::Cancel { operation })
                            .unwrap(),
                        RuntimeUpdate::Applied
                    ));
                    armed
                }
                "error" => {
                    assert!(
                        runtime
                            .execute(RuntimeCommand::Resume {
                                operation,
                                result: None
                            })
                            .is_err()
                    );
                    armed
                }
                _ => {
                    let refreshed: HostUpdateDto = ready(
                        runtime
                            .execute(RuntimeCommand::Resume {
                                operation,
                                result: None,
                            })
                            .unwrap(),
                    )
                    .1;
                    assert_eq!(samples(&refreshed), body);
                    refreshed
                }
            };
            let drawn: HostUpdateDto = activate(runtime, &visible, "抽样");
            assert_eq!(samples(&drawn).last(), Some(&fourth), "{outcome}");
            assert_eq!(
                runtime.debug_snapshot().unwrap().state["variables"]["phase"],
                "committed",
                "{outcome}"
            );
        });
    }
}

#[test]
fn engine_random_rolls_back_failed_commands_and_restarts_from_startup() {
    with_seeded_runtime(
        ":: Start\n<<set $body = random()>>\n",
        "V.startup = Math.random();",
        42,
        |runtime| {
            ready(runtime.execute(RuntimeCommand::Start).unwrap());
            let before = runtime.engine().snapshot();
            let values = runtime.debug_snapshot().unwrap().state["variables"].clone();
            assert!(
                runtime
                    .execute(RuntimeCommand::DebugScript {
                        source: "Math.random(); throw new Error('fail')".into()
                    })
                    .is_err()
            );
            assert_eq!(runtime.engine().snapshot(), before);
            let operation = pending(
                runtime
                    .execute(RuntimeCommand::DebugScript {
                        source: "Math.random(); await Host.delay(1); Math.random()".into(),
                    })
                    .unwrap(),
            )
            .id();
            runtime
                .execute(RuntimeCommand::Cancel { operation })
                .unwrap();
            assert_eq!(runtime.engine().snapshot(), before);
            runtime
                .execute(RuntimeCommand::DebugScript {
                    source: "Math.random()".into(),
                })
                .unwrap();
            runtime
                .execute(RuntimeCommand::DebugScript {
                    source: "Engine.restart()".into(),
                })
                .unwrap();
            assert_eq!(runtime.engine().snapshot(), before);
            assert_eq!(runtime.debug_snapshot().unwrap().state["variables"], values);
        },
    );
}

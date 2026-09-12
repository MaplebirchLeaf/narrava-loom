//! 随机数经真实脚本、VM、历史和宿主事务共用一条可重放序列。

use super::support::{
    action, export_save, import_save, navigate, pending, ready, text, with_runtime,
};
use crate::RuntimeSession;
use narrava_loom_core::random::RandomState;
use narrava_loom_protocol::{
    HostUpdateDto, PendingOperation, PendingResult, RuntimeCommand, RuntimeUpdate,
};

const DRAW: &str = r#"
Random.seed(42);
Macro.add('draw', {handler: () => `DRAW=${Random.next()};`});
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
fn random_math_script_and_twee_draw_from_the_same_sequence() {
    let story: &str =
        ":: Start\n<<first>><<set $third = random()>><<set $fourth = either(10,20)>><<probe>>\n";
    let script: &str = r#"
Random.seed(42);
Macro.add('first', {handler: () => {
    State.variables.set('first', Math.random());
    State.variables.set('second', Random.next());
    return '';
}});
Macro.add('probe', {handler: () => JSON.stringify({
    values: ['first','second','third','fourth'].map(key => State.variables.get(key)),
    current: Random.current(),
})});
"#;
    with_runtime(story, script, |runtime| {
        let frame: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
        let observed: serde_json::Value = serde_json::from_str(&text(&frame)).unwrap();
        let mut expected: RandomState = RandomState::new(42);
        let values: [f64; 4] = [
            expected.next_unit(),
            expected.next_unit(),
            expected.next_unit(),
            if expected.next_unit() < 0.5 {
                10.0
            } else {
                20.0
            },
        ];
        let observed_values: Vec<f64> = observed["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value: &serde_json::Value| value.as_f64().unwrap())
            .collect();
        assert_eq!(observed_values, values);
        assert_eq!(
            observed["current"],
            serde_json::json!({"seed":"42","state":expected.state().to_string()})
        );
    });
}

#[test]
fn random_seed_validates_without_coercion_and_current_is_lossless_and_detached() {
    let script: &str = r#"
Macro.add('probe', {handler: () => {
    Random.seed(Number.MAX_SAFE_INTEGER);
    const before = Random.current();
    const rejected = [];
    for (const invalid of [-1, 0.5, NaN, Infinity, 2 ** 53, '42', null, undefined]) {
        try { Random.seed(invalid); rejected.push(false); }
        catch (error) { rejected.push(error instanceof TypeError || error instanceof RangeError); }
    }
    const after = Random.current();
    before.seed = 'changed'; before.state = 'changed';
    const first = Math.random();
    const result = {rejected, after, current: Random.current(), first};
    Random.seed(0);
    result.zero = Random.current();
    return JSON.stringify(result);
}});
"#;
    with_runtime(":: Start\n<<probe>>\n", script, |runtime| {
        let frame: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
        let observed: serde_json::Value = serde_json::from_str(&text(&frame)).unwrap();
        let mut expected: RandomState = RandomState::new(9_007_199_254_740_991);
        assert_eq!(observed["rejected"], serde_json::json!(vec![true; 8]));
        assert_eq!(
            observed["after"],
            serde_json::json!({"seed":"9007199254740991","state":"9007199254740991"})
        );
        assert_eq!(observed["first"], expected.next_unit());
        assert_eq!(observed["current"]["state"], expected.state().to_string());
        assert_eq!(
            observed["zero"],
            serde_json::json!({"seed":"0","state":"0"})
        );
    });
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
    with_runtime(story, DRAW, |runtime| {
        let mut expected: RandomState = RandomState::new(42);
        let pair: Vec<f64> = vec![expected.next_unit(), expected.next_unit()];
        let third: f64 = expected.next_unit();
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
    with_runtime(story, DRAW, |runtime| {
        let mut expected: RandomState = RandomState::new(42);
        let first: f64 = expected.next_unit();
        let second: f64 = expected.next_unit();
        let third: f64 = expected.next_unit();
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
Macro.add('arm', {{handler: () => {{ armed = true; State.variables.set('phase', 'committed'); return `DRAW=${{Random.next()}};`; }} }});
Macro.add('pause', {{handler: async () => {{
    if (!armed) return '';
    armed = false;
    await Host.delay(1);
    if ('{outcome}' === 'error') throw new Error('random refresh failed');
    return '';
}} }});
"#
        );
        with_runtime(story, &script, |runtime| {
            let mut expected: RandomState = RandomState::new(42);
            let body: Vec<f64> = vec![expected.next_unit(), expected.next_unit()];
            let third: f64 = expected.next_unit();
            let fourth: f64 = expected.next_unit();
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
fn random_refresh_same_passage_goto_resumes_the_authoritative_tail() {
    let story: &str = r#":: Start
<<link [[进入|Page]]>><</link>>
:: Page
<<draw>><<redirect>>
<<link "准备刷新">><<arm>><</link>>
<<link "抽样">><<draw>><</link>>
"#;
    let script: String = format!(
        r#"{DRAW}
let armed = false;
Macro.add('arm', {{handler: () => {{ armed = true; return `DRAW=${{Random.next()}};`; }} }});
Macro.add('redirect', {{handler: () => {{
    armed = false;
    return '';
}} }});
Reaction.add({{id:'redirect', lifecycle:true, cond:() => armed && Random.next() >= 0, goto:'Page', limit:1}});
"#
    );
    with_runtime(story, &script, |runtime| {
        let mut expected: RandomState = RandomState::new(42);
        let first: f64 = expected.next_unit();
        let second: f64 = expected.next_unit();
        let third: f64 = expected.next_unit();
        let fourth: f64 = expected.next_unit();
        let start: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
        let page: HostUpdateDto = navigate(runtime, &start, "Page").1;
        assert_eq!(samples(&page), vec![first]);
        let armed: HostUpdateDto = activate(runtime, &page, "准备刷新");
        assert_eq!(samples(&armed).last(), Some(&second));
        let redirected: HostUpdateDto = ready(language_refresh(runtime)).1;
        assert_eq!(samples(&redirected), vec![third]);
        let drawn: HostUpdateDto = activate(runtime, &redirected, "抽样");
        assert_eq!(samples(&drawn).last(), Some(&fourth));
    });
}

#[test]
fn random_save_after_uses_active_state_for_success_and_failure() {
    for succeeds in [true, false] {
        let script: String = format!(
            r#"{DRAW}
Save.after('export', () => {{ State.variables.set('after', Math.random()); }});
Macro.add('save', {{handler: () => {{ Save.export('quick'); return ''; }} }});
"#
        );
        with_runtime(
            ":: Start\n<<draw>><<link \"保存\">><<save>><</link>>\n",
            &script,
            |runtime| {
                let frame: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
                let operation: u64 = pending(
                    runtime
                        .execute(RuntimeCommand::Activate {
                            interaction: action(&frame, "保存"),
                        })
                        .unwrap(),
                )
                .id();
                let result: PendingResult = if succeeds {
                    PendingResult::Save { document: None }
                } else {
                    PendingResult::Failed {
                        error: narrava_loom_protocol::HostErrorDto::new("test.io", "save failed"),
                    }
                };
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: Some(result),
                    })
                    .expect("Save.after 应获得活动 State");
                let mut expected: RandomState = RandomState::new(42);
                let _first: f64 = expected.next_unit();
                let second: f64 = expected.next_unit();
                let observed: narrava_loom_protocol::HostDebugSnapshotDto =
                    runtime.debug_snapshot().unwrap();
                assert_eq!(observed.state["variables"]["after"].as_f64(), Some(second));
                assert_eq!(observed.random["state"], expected.state().to_string());
            },
        );
    }
}

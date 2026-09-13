//! 输入引发的导航和脚本保存属于同一事务，失败必须恢复全部运行边界。

use super::support::{action, export_save, pending, ready, with_runtime};
use narrava_loom_protocol::{
    HostErrorDto, HostNodeDto, HostUpdateDto, PendingOperation, PendingResult, RuntimeCommand,
    RuntimeUpdate,
};

#[test]
fn input_navigation_save_failure_restores_history_interactions_and_reactions() {
    for asynchronous in [false, true] {
        for encoding_fails in [false, true] {
            let story: &str = r#":: Start
<<textbox "$name" "before">>
<<link "旧操作">><<legacy>><</link>>
:: Next
<<prepare>>
"#;
            let script: String = format!(
                r#"
State.variables.set('name', 'before');
Reaction.add({{id:'navigate', state:'$name', cond:({{after}}) => after === 'changed', goto:'Next', once:true}});
Macro.add('legacy', {{handler: () => 'old interaction works'}});
Macro.add('prepare', {{async handler() {{
    Math.random();
    if ({asynchronous}) await Host.delay(1);
    if ({encoding_fails}) State.variables.set('unsupported', () => {{}});
    Save.export('quick');
    return '';
}} }});
"#
            );
            with_runtime(story, &script, |runtime| {
                let start: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
                let input: String = start
                    .nodes
                    .iter()
                    .find_map(|node: &HostNodeDto| match node {
                        HostNodeDto::Textbox { id, .. } => Some(id.clone()),
                        _ => None,
                    })
                    .unwrap();
                let old_action: String = action(&start, "旧操作");
                let before: Vec<u8> = export_save(runtime);
                let mut result: Result<RuntimeUpdate, HostErrorDto> =
                    runtime.execute(RuntimeCommand::Input {
                        interaction: input.clone(),
                        value: "changed".into(),
                    });
                if asynchronous {
                    let PendingOperation::Delay { operation, .. } = pending(result.unwrap()) else {
                        panic!("输入导航应先等待正文宏")
                    };
                    result = runtime.execute(RuntimeCommand::Resume {
                        operation,
                        result: None,
                    });
                }
                let error: HostErrorDto = if encoding_fails {
                    result.expect_err("保存编码失败必须回滚输入导航")
                } else {
                    let PendingOperation::Save { operation, .. } = pending(result.unwrap()) else {
                        panic!("输入导航应等待存档IO")
                    };
                    runtime
                        .execute(RuntimeCommand::Resume {
                            operation,
                            result: Some(PendingResult::Failed {
                                error: HostErrorDto::new("test.io", "save failed"),
                            }),
                        })
                        .expect_err("保存IO失败必须回滚输入导航")
                };
                assert_eq!(
                    error.code,
                    if encoding_fails {
                        "runtime_session.save"
                    } else {
                        "test.io"
                    }
                );
                assert_eq!(
                    runtime.debug_snapshot().unwrap().current.as_deref(),
                    Some("Start"),
                    "async={asynchronous}, encoding={encoding_fails}"
                );
                assert_eq!(
                    export_save(runtime),
                    before,
                    "State、历史和Reaction计数都应回滚"
                );
                let old_frame: HostUpdateDto = ready(
                    runtime
                        .execute(RuntimeCommand::Activate {
                            interaction: old_action,
                        })
                        .expect("旧页交互必须仍有效"),
                )
                .1;
                assert_eq!(old_frame.current, "Start");
                assert!(!old_frame.can_back);

                // once Reaction 的运行状态必须一并回滚，再次提交才能重新触发同一导航。
                let next: Result<RuntimeUpdate, HostErrorDto> =
                    runtime.execute(RuntimeCommand::Input {
                        interaction: input,
                        value: "changed".into(),
                    });
                if encoding_fails && !asynchronous {
                    assert!(next.is_err());
                } else {
                    assert!(matches!(next, Ok(RuntimeUpdate::Pending { .. })));
                }
            });
        }
    }
}

#[test]
fn input_navigation_save_cancellation_and_callback_failure_restore_the_original_transaction() {
    for outcome in ["cancel", "callback-failure", "success"] {
        let script: String = format!(
            r#"
State.variables.set('name', 'before');
Reaction.add({{id:'navigate', state:'$name', cond:({{after}}) => after === 'changed', goto:'Next', once:true}});
Macro.add('save', {{handler: () => {{ Save.export('quick'); return ''; }} }});
Save.after('export', () => {{
    if ('{outcome}' === 'callback-failure') throw new Error('save callback failed');
}});
"#
        );
        with_runtime(
            ":: Start\n<<textbox \"$name\" \"before\">>\n:: Next\n<<save>>\n",
            &script,
            |runtime| {
                let start: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
                let input: String = start
                    .nodes
                    .iter()
                    .find_map(|node: &HostNodeDto| match node {
                        HostNodeDto::Textbox { id, .. } => Some(id.clone()),
                        _ => None,
                    })
                    .unwrap();
                let before: Vec<u8> = export_save(runtime);
                let operation: u64 = pending(
                    runtime
                        .execute(RuntimeCommand::Input {
                            interaction: input,
                            value: "changed".into(),
                        })
                        .unwrap(),
                )
                .id();
                if outcome == "cancel" {
                    assert_eq!(
                        runtime
                            .execute(RuntimeCommand::Cancel { operation })
                            .unwrap(),
                        RuntimeUpdate::Applied
                    );
                } else {
                    let result: Result<RuntimeUpdate, HostErrorDto> =
                        runtime.execute(RuntimeCommand::Resume {
                            operation,
                            result: Some(PendingResult::Save { document: None }),
                        });
                    if outcome == "callback-failure" {
                        assert_eq!(result.unwrap_err().code, "script.save_after");
                    } else {
                        let committed: HostUpdateDto = ready(result.unwrap()).1;
                        assert_eq!(committed.current, "Next");
                        assert_eq!(
                            runtime.debug_snapshot().unwrap().current.as_deref(),
                            Some("Next")
                        );
                        assert_eq!(
                            runtime.debug_snapshot().unwrap().state["variables"]["name"],
                            "changed"
                        );
                        return;
                    }
                }
                assert_eq!(
                    runtime.debug_snapshot().unwrap().current.as_deref(),
                    Some("Start")
                );
                assert_eq!(export_save(runtime), before, "{outcome}");
            },
        );
    }
}

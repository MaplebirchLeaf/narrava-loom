use super::support::{
    action, export_save, navigate, navigation, pending, ready, resume, with_runtime,
};
use narrava_loom_protocol::{
    HostNodeDto, PendingOperation, PendingResult, RuntimeCommand, RuntimeUpdate, SaveOperation,
};

#[test]
fn dialog_action_opens_four_pages_without_navigation_and_can_reopen() {
    with_runtime(
        r#":: Start
<<set $opened to 0>>
<<link "查看角色">>
<<set $opened to $opened + 1>>
<<dialog "装备">>
<<page "属性">>
<<print $opened>>
<<meter "体力" 72 0 100>>
<<page "装备">>
<<image "tree.png" "树">>
<<page "经历">>
<<scriptText>>
<<page "其他">>
只有文字。
<</dialog>>
<</link>>
"#,
        "Macro.add('scriptText', {handler: () => '脚本正文'});",
        |runtime| {
            let (_, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
            assert!(
                !start
                    .nodes
                    .iter()
                    .any(|node| matches!(node, HostNodeDto::Dialog { .. }))
            );
            let id: String = action(&start, "查看角色");
            let mut previous_key: Option<String> = None;
            for count in 1..=2 {
                let (_, update) = ready(
                    runtime
                        .execute(RuntimeCommand::Activate {
                            interaction: id.clone(),
                        })
                        .unwrap(),
                );
                assert_eq!(update.current, "Start");
                assert!(!update.can_back);
                let dialogs: Vec<&HostNodeDto> = update
                    .nodes
                    .iter()
                    .filter(|node| matches!(node, HostNodeDto::Dialog { .. }))
                    .collect();
                assert_eq!(dialogs.len(), 1);
                let HostNodeDto::Dialog {
                    key,
                    initial,
                    pages,
                } = dialogs[0]
                else {
                    unreachable!()
                };
                assert_eq!(initial, "装备");
                assert_eq!(
                    pages
                        .iter()
                        .map(|page| page.title.as_str())
                        .collect::<Vec<_>>(),
                    ["属性", "装备", "经历", "其他"]
                );
                assert!(pages[0].nodes.iter().any(|node| matches!(node, HostNodeDto::Text { text, .. } if text == &count.to_string())));
                assert!(pages[0].nodes.iter().any(|node| matches!(node, HostNodeDto::Component { capability, .. } if capability == "meter")));
                assert!(
                    matches!(&pages[1].nodes.iter().find(|node| matches!(node, HostNodeDto::Image { .. })).unwrap(), HostNodeDto::Image { resource, alt, .. } if resource == "img/tree.png" && alt == "树")
                );
                assert!(pages[2].nodes.iter().any(
                    |node| matches!(node, HostNodeDto::Text { text, .. } if text == "脚本正文")
                ));
                assert_ne!(previous_key.as_ref(), Some(key));
                previous_key = Some(key.clone());
            }
        },
    );
}

#[test]
fn dialog_action_pending_cancel_and_invalid_default_roll_back() {
    for initial in ["页", "不存在"] {
        with_runtime(
            &format!(
                r#":: Start
<<set $opened to 0>>
<<link "打开">>
<<set $opened to $opened + 1>>
<<dialog "{initial}">>
<<page "页">>
<<wait>>
<<print $opened>>
<</dialog>>
<</link>>
"#
            ),
            "Macro.add('wait', {handler: async () => { await Host.delay(1); return '完成'; }});",
            |runtime| {
                let (_, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
                let id = action(&start, "打开");
                let operation = pending(
                    runtime
                        .execute(RuntimeCommand::Activate {
                            interaction: id.clone(),
                        })
                        .unwrap(),
                )
                .id();
                assert!(
                    runtime
                        .execute(RuntimeCommand::Cancel {
                            operation: operation + 1
                        })
                        .is_err()
                );
                runtime
                    .execute(RuntimeCommand::Cancel { operation })
                    .unwrap();
                let operation = pending(
                    runtime
                        .execute(RuntimeCommand::Activate {
                            interaction: id.clone(),
                        })
                        .unwrap(),
                )
                .id();
                let result = runtime.execute(RuntimeCommand::Resume {
                    operation,
                    result: None,
                });
                if initial == "页" {
                    let (_, update) = ready(result.unwrap());
                    let HostNodeDto::Dialog { pages, .. } = update
                        .nodes
                        .iter()
                        .find(|node| matches!(node, HostNodeDto::Dialog { .. }))
                        .unwrap()
                    else {
                        unreachable!()
                    };
                    assert!(
                        pages[0].nodes.iter().any(
                            |node| matches!(node, HostNodeDto::Text { text, .. } if text == "1")
                        )
                    );
                    assert!(!update.can_back);
                } else {
                    assert!(result.is_err());
                    assert!(matches!(
                        runtime
                            .execute(RuntimeCommand::Activate { interaction: id })
                            .unwrap(),
                        RuntimeUpdate::Pending { .. }
                    ));
                }
            },
        );
    }
}

#[test]
fn audio_macros_and_script_share_declarations_without_surface_nodes() {
    with_runtime(
        r#":: Start
<<audio "forest.ogg" "bgm">>
<<scriptAudio>>
正文
"#,
        r#"Macro.add('scriptAudio', { handler: () => Audio.play('bell.wav', {channel:'voice', loop:false, volume:0.4}) });"#,
        |runtime| {
            let (effects, update) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
            assert_eq!(
                serde_json::to_value(effects).unwrap(),
                serde_json::json!([
                    {"type":"play","resource":"audio/forest.ogg","channel":"bgm","loop":true,"volume":1.0},
                    {"type":"play","resource":"audio/bell.wav","channel":"voice","loop":false,"volume":0.4}
                ])
            );
            assert!(
                !serde_json::to_string(&update.nodes)
                    .unwrap()
                    .contains("audio/")
            );
            assert!(!update.can_back);
        },
    );
}

#[test]
fn audio_tags_headers_continuity_override_and_history_use_current_passage() {
    with_runtime(
        r#":: Start
<<link [[进入森林|Forest]]>><</link>>
:: Header
页眉
<<audio "forest.ogg" "ambience" "forest">>
:: Footer
页脚
:: Forest [forest rain]
<<audio "birds.ogg" "voice">>
<<link [[继续|ForestNext]]>><</link>>
:: ForestNext [forest]
<<audio "audio/forest.ogg" "ambience">>
<<link [[洞穴|Cave]]>><</link>>
:: Cave [forest]
<<audio "cave.ogg" "ambience">>
<<link [[城镇|Town]]>><</link>>
:: Town
结束
"#,
        r#"Audio.play('rain.ogg', { channel:'rain', tags:['rain'], volume:0.2 });"#,
        |runtime| {
            let (effects, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
            assert!(effects.is_empty());
            assert!(serde_json::to_string(&start).unwrap().contains("页眉"));
            assert!(serde_json::to_string(&start).unwrap().contains("页脚"));
            let (effects, forest) = navigate(runtime, &start, "Forest");
            assert_eq!(effects.len(), 3);
            let (effects, _next) = navigate(runtime, &forest, "ForestNext");
            assert!(
                effects.iter().all(|effect| matches!(
                    effect,
                    narrava_loom_protocol::AudioEffect::Stop { .. }
                ))
            );
            assert_eq!(
                effects.len(),
                2,
                "forest ambience must continue without another play"
            );
            let operation = pending(
                runtime
                    .execute(RuntimeCommand::SelectLanguage {
                        locale: "en".into(),
                    })
                    .unwrap(),
            )
            .id();
            let (effects, next) = ready(
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: Some(PendingResult::SelectLanguage),
                    })
                    .unwrap(),
            );
            assert!(
                effects.is_empty(),
                "language refresh must not restart matching audio"
            );
            let save = pending(
                runtime
                    .execute(RuntimeCommand::Save {
                        operation: SaveOperation::Export,
                        target: "audio-test".into(),
                    })
                    .unwrap(),
            );
            let PendingOperation::Save {
                operation,
                document: Some(document),
                ..
            } = save
            else {
                panic!("export document expected");
            };
            assert!(matches!(
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: Some(PendingResult::Save { document: None })
                    })
                    .unwrap(),
                RuntimeUpdate::Applied
            ));
            let operation = pending(
                runtime
                    .execute(RuntimeCommand::Save {
                        operation: SaveOperation::Import,
                        target: "audio-test".into(),
                    })
                    .unwrap(),
            )
            .id();
            let (effects, restored) = ready(
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: Some(PendingResult::Save {
                            document: Some(document),
                        }),
                    })
                    .unwrap(),
            );
            assert!(
                effects.is_empty(),
                "restoring the same scene must not restart matching audio"
            );
            assert_eq!(restored.current, next.current);
            let (effects, cave) = navigate(runtime, &restored, "Cave");
            assert!(
                matches!(&effects[..], [narrava_loom_protocol::AudioEffect::Play {resource, ..}] if resource == "audio/cave.ogg")
            );
            let (effects, _) = navigate(runtime, &cave, "Town");
            assert!(
                matches!(&effects[..], [narrava_loom_protocol::AudioEffect::Stop {channel}] if channel == "ambience")
            );
            let (effects, _) = ready(runtime.execute(RuntimeCommand::Back).unwrap());
            assert!(
                matches!(&effects[..], [narrava_loom_protocol::AudioEffect::Play {resource, ..}] if resource == "audio/cave.ogg")
            );
        },
    );
}

#[test]
fn audio_waits_for_commit_and_cancel_or_failure_discards_it() {
    for cancel in [false, true] {
        with_runtime(
            ":: Start\n<<audio \"audio/a.wav\">><<wait>>",
            "Macro.add('wait', { handler: async () => { await Host.delay(1); Audio.stop('sfx'); } });",
            |runtime| {
                let operation = pending(runtime.execute(RuntimeCommand::Start).unwrap()).id();
                assert!(
                    runtime
                        .execute(RuntimeCommand::Cancel {
                            operation: operation + 1
                        })
                        .is_err()
                );
                let result = runtime
                    .execute(if cancel {
                        RuntimeCommand::Cancel { operation }
                    } else {
                        RuntimeCommand::Resume {
                            operation,
                            result: None,
                        }
                    })
                    .unwrap();
                if cancel {
                    assert!(matches!(result, RuntimeUpdate::Applied));
                    let operation = pending(runtime.execute(RuntimeCommand::Start).unwrap()).id();
                    let RuntimeUpdate::Audio { effects, .. } = runtime
                        .execute(RuntimeCommand::Resume {
                            operation,
                            result: None,
                        })
                        .unwrap()
                    else {
                        panic!("new execution must complete");
                    };
                    assert_eq!(
                        effects.len(),
                        1,
                        "cancelled audio must not leak into next execution"
                    );
                } else {
                    let RuntimeUpdate::Audio { effects, .. } = result else {
                        panic!("effects must follow successful resume");
                    };
                    assert_eq!(effects.len(), 1);
                }
                assert!(runtime.execute(RuntimeCommand::Back).is_err());
            },
        );
    }
    with_runtime(
        r#":: Start
<<audio "audio/a.wav">><<failOnce>>"#,
        "let attempts = 0; Macro.add('failOnce', { handler: () => { if (attempts++ === 0) Audio.play('../bad.wav'); } });",
        |runtime| {
            assert!(runtime.execute(RuntimeCommand::Start).is_err());
            let RuntimeUpdate::Audio { effects, .. } =
                runtime.execute(RuntimeCommand::Start).unwrap()
            else {
                panic!("retry must succeed");
            };
            assert_eq!(
                effects.len(),
                1,
                "failed execution must discard queued effects"
            );
        },
    );
}

#[test]
fn direct_dialog_uses_vm_and_rejects_duplicate_titles() {
    with_runtime(
        ":: Start\n<<dialog \"页\">>\n<<page \"页\">>\n正文\n<</dialog>>",
        "",
        |runtime| {
            let (_, update) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
            assert!(
                update.nodes.iter().any(
                    |node| matches!(node, HostNodeDto::Dialog { pages, .. } if pages.len() == 1)
                )
            );
        },
    );
    for source in [
        ":: Start\n<<dialog \"页\">>\n<<page \"页\">>\nA\n<<page \"页\">>\nB\n<</dialog>>",
        ":: Start\n<<dialog 1>>\n<<page 1>>\n正文\n<</dialog>>",
    ] {
        with_runtime(source, "", |runtime| {
            assert!(runtime.execute(RuntimeCommand::Start).is_err());
        });
    }
}

#[test]
fn pending_commands_preserve_identity_and_resume_through_special_regions() {
    with_runtime(
        ":: Start\n<<wait>>Main\n:: Bar\n<<wait>>Bar\n",
        "Macro.add('wait', {handler: async () => { await Host.delay(1); await Host.delay(2); return 'ready'; }});",
        |runtime| {
            for command in [
                RuntimeCommand::Activate {
                    interaction: "missing".into(),
                },
                RuntimeCommand::Input {
                    interaction: "missing".into(),
                    value: serde_json::Value::Null,
                },
            ] {
                assert_eq!(
                    runtime.execute(command).unwrap_err().code,
                    "runtime_session.not_started"
                );
            }
            let first = pending(runtime.execute(RuntimeCommand::Start).unwrap()).id();
            assert_eq!(
                runtime.execute(RuntimeCommand::Back).unwrap_err().code,
                "runtime_session.pending"
            );
            assert_eq!(
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation: first + 100,
                        result: None
                    })
                    .unwrap_err()
                    .code,
                "runtime_session.operation_mismatch"
            );
            let second = pending(resume(runtime, first, None)).id();
            assert_ne!(first, second);
            let third = pending(resume(runtime, second, None)).id();
            let fourth = pending(resume(runtime, third, None)).id();
            let RuntimeUpdate::Ready { update } = resume(runtime, fourth, None) else {
                panic!("ready")
            };
            assert_eq!(update.current, "Start");
            assert!(
                update.nodes.iter().any(
                    |node| matches!(node, HostNodeDto::Region { region, .. } if region == "bar")
                )
            );
            assert_eq!(
                runtime.execute(RuntimeCommand::Start).unwrap_err().code,
                "runtime_session.already_started"
            );
            // A rejected command must not discard the last presented frame.
            assert_eq!(
                runtime.execute(RuntimeCommand::Start).unwrap_err().code,
                "runtime_session.already_started"
            );
        },
    );
}

#[test]
fn cancel_restores_the_command_state_and_consumes_the_operation() {
    with_runtime(
        ":: Start\n<<set $score to 7>><<wait>>Main\n",
        "State.variables.set('score', 1); Macro.add('wait', {handler: async () => { await Host.delay(1); }});",
        |runtime| {
            let before = export_save(runtime);
            let operation = pending(runtime.execute(RuntimeCommand::Start).unwrap()).id();
            assert_eq!(
                runtime
                    .execute(RuntimeCommand::Cancel { operation })
                    .unwrap(),
                RuntimeUpdate::Applied
            );
            assert_eq!(
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: None
                    })
                    .unwrap_err()
                    .code,
                "runtime_session.unknown_operation"
            );
            assert_eq!(export_save(runtime), before);
        },
    );
}

#[test]
fn save_round_trip_and_host_failures_preserve_presented_history() {
    with_runtime(
        ":: Start\n<<set $score to 1>><<link [[Next|Next]]>><</link>>\n:: Next\n<<set $score to 2>>Done\n",
        "",
        |runtime| {
            let RuntimeUpdate::Ready { update } = runtime.execute(RuntimeCommand::Start).unwrap()
            else {
                panic!("start")
            };
            let id = navigation(&update.nodes)
                .unwrap_or_else(|| panic!("missing navigation: {update:?}"));
            let saved = export_save(runtime);
            runtime
                .execute(RuntimeCommand::Activate { interaction: id })
                .unwrap();
            let RuntimeUpdate::Ready { update } = runtime.execute(RuntimeCommand::Back).unwrap()
            else {
                panic!("back")
            };
            assert_eq!(update.current, "Start");
            assert!(update.can_forward);
            let RuntimeUpdate::Ready { update } = runtime.execute(RuntimeCommand::Forward).unwrap()
            else {
                panic!("forward")
            };
            assert_eq!(update.current, "Next");
            let operation = pending(
                runtime
                    .execute(RuntimeCommand::Save {
                        operation: SaveOperation::Import,
                        target: "quick".into(),
                    })
                    .unwrap(),
            )
            .id();
            let RuntimeUpdate::Ready { update } = resume(
                runtime,
                operation,
                Some(PendingResult::Save {
                    document: Some(saved.clone()),
                }),
            ) else {
                panic!("restored")
            };
            assert_eq!(update.current, "Start");
            assert_eq!(export_save(runtime), saved);
            let operation = pending(
                runtime
                    .execute(RuntimeCommand::SelectLanguage {
                        locale: "en".into(),
                    })
                    .unwrap(),
            )
            .id();
            assert_eq!(
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: Some(PendingResult::Save { document: None })
                    })
                    .unwrap_err()
                    .code,
                "runtime_session.platform_result_mismatch"
            );
            let operation = pending(
                runtime
                    .execute(RuntimeCommand::Save {
                        operation: SaveOperation::Import,
                        target: "quick".into(),
                    })
                    .unwrap(),
            )
            .id();
            assert!(
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: Some(PendingResult::Save {
                            document: Some(vec![0])
                        })
                    })
                    .is_err()
            );
            assert_eq!(export_save(runtime), saved);
            let operation = pending(
                runtime
                    .execute(RuntimeCommand::SelectLanguage {
                        locale: "en".into(),
                    })
                    .unwrap(),
            )
            .id();
            let RuntimeUpdate::Ready { update } =
                resume(runtime, operation, Some(PendingResult::SelectLanguage))
            else {
                panic!("refresh")
            };
            assert_eq!(update.current, "Start");
            assert!(!update.can_back);
        },
    );
}

#[test]
fn failed_reactions_restore_state_history_and_the_same_interaction() {
    with_runtime(
        ":: Start\n<<link [[Next|Next]]>><</link>>\n:: Next\n<<set $score to 2>>Done\n",
        "State.variables.set('score', 1); Reaction.add({id: 'broken', state: '$score', once: true, include: 'Missing'});",
        |runtime| {
            let RuntimeUpdate::Ready { update } = runtime.execute(RuntimeCommand::Start).unwrap()
            else {
                panic!("ready")
            };
            let id: String = navigation(&update.nodes).unwrap();
            let before: Vec<u8> = export_save(runtime);
            for _attempt in 0..2 {
                let error = runtime
                    .execute(RuntimeCommand::Activate {
                        interaction: id.clone(),
                    })
                    .unwrap_err();
                assert_eq!(error.code, "reaction.include");
                assert_eq!(export_save(runtime), before);
            }
        },
    );
}

#[test]
fn input_save_failure_restores_the_input_checkpoint_after_host_resume() {
    with_runtime(
        ":: Start\n<<textbox \"$name\" \"before\">>\n",
        "State.variables.set('name', 'before'); Reaction.add({id: 'save-input', state: '$name', cond: () => { Save.export('quick'); return false; }, include: 'Start'});",
        |runtime| {
            let RuntimeUpdate::Ready { update } = runtime.execute(RuntimeCommand::Start).unwrap()
            else {
                panic!("ready")
            };
            let id: String = update
                .nodes
                .iter()
                .find_map(|node| match node {
                    HostNodeDto::Textbox { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .unwrap();
            let before: Vec<u8> = export_save(runtime);
            let operation: u64 = pending(
                runtime
                    .execute(RuntimeCommand::Input {
                        interaction: id,
                        value: serde_json::json!("after"),
                    })
                    .unwrap(),
            )
            .id();
            let error = runtime
                .execute(RuntimeCommand::Resume {
                    operation,
                    result: Some(PendingResult::Failed {
                        error: narrava_loom_protocol::HostErrorDto::new("test.io", "write failed"),
                    }),
                })
                .unwrap_err();
            assert_eq!(error.code, "test.io");
            assert_eq!(export_save(runtime), before);
        },
    );
}

#[test]
fn script_language_selection_refreshes_via_the_same_host_operation() {
    with_runtime(":: Start\nMain\n", "I18n.select('en');", |runtime| {
        let operation = pending(runtime.execute(RuntimeCommand::Start).unwrap()).id();
        let RuntimeUpdate::Ready { update } =
            resume(runtime, operation, Some(PendingResult::SelectLanguage))
        else {
            panic!("refresh")
        };
        assert_eq!(update.current, "Start");
        assert!(!update.can_back);
    });
}

use super::support::{pending, ready, with_runtime};
use narrava_loom_protocol::{HostDebugSnapshotDto, RuntimeCommand};

#[test]
fn inspection_is_detached_and_does_not_change_state_or_drain_logs() {
    with_runtime(
        ":: Start\n<<set $count to 7>>\n",
        "Logger.info('author', 'ready');",
        |runtime| {
            ready(runtime.execute(RuntimeCommand::Start).unwrap());
            let first: HostDebugSnapshotDto = runtime.debug_snapshot().unwrap();
            let mut detached: HostDebugSnapshotDto = first.clone();
            detached.state["variables"]["count"] = 100.into();
            assert_eq!(runtime.debug_snapshot().unwrap(), first);
            assert_eq!(first.state["variables"]["count"].as_f64(), Some(7.0));
            assert!(first.logs.iter().any(|record| record.message == "ready"));
            assert!(first.location["current"].is_null());
            assert_eq!(first.current.as_deref(), Some("Start"));
        },
    );
}

#[test]
fn inspection_rejects_pending_and_failed_command_logs_survive_rollback() {
    with_runtime(
        ":: Start\n<<wait>>\n",
        "Macro.add('wait', {async handler() { Logger.warn('author', 'waiting'); await Host.delay(1); }});",
        |runtime| {
            let operation = pending(runtime.execute(RuntimeCommand::Start).unwrap());
            assert_eq!(
                runtime.debug_snapshot().unwrap_err().code,
                "runtime_session.pending"
            );
            runtime
                .execute(RuntimeCommand::Cancel {
                    operation: operation.id(),
                })
                .unwrap();
            assert!(
                runtime
                    .debug_snapshot()
                    .unwrap()
                    .logs
                    .iter()
                    .any(|record| record.message == "waiting")
            );
            let error = runtime
                .execute(RuntimeCommand::Activate {
                    interaction: "missing".into(),
                })
                .unwrap_err();
            assert!(runtime.log_records().iter().any(|record| {
                record
                    .diagnostic
                    .as_ref()
                    .is_some_and(|diagnostic| diagnostic.code == error.code)
            }));
        },
    );
}

#[test]
fn inspection_limits_location_names_and_log_previews_without_truncating_author_data() {
    with_runtime(
        ":: Start\n",
        "Location.add({id:'town',name:'x'.repeat(10000),bounds:[[0,0],[1,0],[0,1]]}); Logger.info('author', 'x'.repeat(10000));",
        |runtime| {
            ready(runtime.execute(RuntimeCommand::Start).unwrap());
            let preview: HostDebugSnapshotDto = runtime.debug_snapshot().unwrap();
            assert!(preview.truncated);
            assert!(
                preview.location["places"][0]["name"]
                    .as_str()
                    .unwrap()
                    .len()
                    < 3000
            );
            assert!(preview.logs[0].message.len() < 3000);
            assert_eq!(runtime.log_records()[0].message.len(), 10000);
        },
    );
}

#[test]
fn console_invalid_operation_preserves_pending_jobs_and_cancel_restores_state() {
    with_runtime(":: Start\n", "V.count = 1;", |runtime| {
        ready(runtime.execute(RuntimeCommand::Start).unwrap());
        let operation = pending(
            runtime
                .execute(RuntimeCommand::DebugScript {
                    source: "await Host.delay(1); V.count = 2".into(),
                })
                .unwrap(),
        );
        assert!(
            runtime
                .execute(RuntimeCommand::Cancel {
                    operation: operation.id() + 1
                })
                .is_err()
        );
        runtime
            .execute(RuntimeCommand::Resume {
                operation: operation.id(),
                result: None,
            })
            .unwrap();
        assert_eq!(
            runtime.debug_snapshot().unwrap().state["variables"]["count"].as_f64(),
            Some(2.0)
        );
        let operation = pending(
            runtime
                .execute(RuntimeCommand::DebugScript {
                    source: "V.count = 3; Math.random(); await Host.delay(1000); V.count = 4"
                        .into(),
                })
                .unwrap(),
        );
        runtime
            .execute(RuntimeCommand::Cancel {
                operation: operation.id(),
            })
            .unwrap();
        let after: HostDebugSnapshotDto = runtime.debug_snapshot().unwrap();
        assert_eq!(after.state["variables"]["count"].as_f64(), Some(2.0));
        runtime
            .execute(RuntimeCommand::DebugScript {
                source: "V.count".into(),
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
            "2"
        );
    });
}

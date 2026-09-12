//! World 刷新视图跨挂起保留，并在恢复或取消后释放。

use super::support::{action, navigate, ready, text, with_runtime};
use narrava_loom_protocol::{
    HostErrorDto, HostUpdateDto, PendingOperation, PendingResult, RuntimeCommand, RuntimeUpdate,
};

const SCRIPT: &str = r#"
World.add({id:'town',bounds:[[0,0],[10,0],[10,10],[0,10]],entry:[2,3]});
let armed = false;
Macro.add('bodyStep', {handler: () => { World.move([4,5]); return ''; }});
Macro.add('headerStep', {handler: () => {
    if (World.current() !== null) World.move([1,1]);
    return '';
}});
Macro.add('arm', {handler: () => {
    const before = World.current().point;
    World.move([6,7]);
    armed = true;
    return `ARM_BEFORE=${JSON.stringify(before)};`;
}});
Macro.add('pause', {handler: async () => {
    if (!armed) return '';
    armed = false;
    await Host.delay(1);
    World.move([3,3]);
    return '';
}});
Macro.add('step', {handler: () => {
    return `MOVE=${JSON.stringify(World.move([7,8]).point)};`;
}});
Macro.add('where', {handler: () => `CURRENT=${JSON.stringify(World.current().point)};`});
"#;

#[test]
fn world_refresh_pending_keeps_readonly_state_and_releases_it_after_resume_or_cancel() {
    for pause_in_header in [false, true] {
        for cancel in [false, true] {
            let body_pause: &str = if pause_in_header { "" } else { "<<pause>>" };
            let header_pause: &str = if pause_in_header { "<<pause>>" } else { "" };
            let story: String = format!(
                r#":: Start
<<link [[进入|Town]]>><</link>>
:: Town [town outside]
<<bodyStep>>{body_pause}<<where>>
<<link "准备刷新">><<arm>><</link>>
<<link "移动">><<step>><</link>>
<<link "观察">><<where>><</link>>
:: Header
<<headerStep>>{header_pause}
"#
            );
            with_runtime(&story, SCRIPT, |runtime| {
                let start: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
                let town: HostUpdateDto = navigate(runtime, &start, "Town").1;
                let armed: HostUpdateDto = ready(
                    runtime
                        .execute(RuntimeCommand::Activate {
                            interaction: action(&town, "准备刷新"),
                        })
                        .unwrap(),
                )
                .1;
                // Header 的正常执行只能修改自己的视图，主世界仍是正文设置的位置。
                assert!(text(&armed).contains("ARM_BEFORE=[4,5];"));
                let RuntimeUpdate::Pending {
                    operation: PendingOperation::SelectLanguage { operation, .. },
                } = runtime
                    .execute(RuntimeCommand::SelectLanguage {
                        locale: String::from("en"),
                    })
                    .unwrap()
                else {
                    panic!("语言切换应等待 Host")
                };
                let RuntimeUpdate::Pending {
                    operation: PendingOperation::Delay { operation, .. },
                } = runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: Some(PendingResult::SelectLanguage),
                    })
                    .unwrap()
                else {
                    panic!("当前页刷新应挂起在正文或 Header 的 Host.delay")
                };
                let error: HostErrorDto = runtime
                    .execute(RuntimeCommand::Resume {
                        operation: operation + 1,
                        result: None,
                    })
                    .unwrap_err();
                assert_eq!(error.code, "runtime_session.operation_mismatch");

                let visible: HostUpdateDto = if cancel {
                    assert!(matches!(
                        runtime
                            .execute(RuntimeCommand::Cancel { operation })
                            .unwrap(),
                        RuntimeUpdate::Applied
                    ));
                    armed
                } else {
                    let refreshed: HostUpdateDto = ready(
                        runtime
                            .execute(RuntimeCommand::Resume {
                                operation,
                                result: None,
                            })
                            .unwrap(),
                    )
                    .1;
                    assert!(text(&refreshed).contains("CURRENT=[6,7];"));
                    assert!(!text(&refreshed).contains("CURRENT=[3,3];"));
                    refreshed
                };
                let observed: HostUpdateDto = ready(
                    runtime
                        .execute(RuntimeCommand::Activate {
                            interaction: action(&visible, "观察"),
                        })
                        .unwrap(),
                )
                .1;
                assert!(text(&observed).contains("CURRENT=[6,7];"));
                let moved: HostUpdateDto = ready(
                    runtime
                        .execute(RuntimeCommand::Activate {
                            interaction: action(&observed, "移动"),
                        })
                        .unwrap(),
                )
                .1;
                assert_eq!(moved.current, "Town");
                assert!(text(&moved).contains("MOVE=[7,8];"));
                let verified: HostUpdateDto = ready(
                    runtime
                        .execute(RuntimeCommand::Activate {
                            interaction: action(&moved, "观察"),
                        })
                        .unwrap(),
                )
                .1;
                assert!(text(&verified).contains("CURRENT=[7,8];"));
                assert_eq!(verified.can_back, town.can_back);
                assert_eq!(verified.can_forward, town.can_forward);
            });
        }
    }
}

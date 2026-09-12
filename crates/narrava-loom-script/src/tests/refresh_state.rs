//! 当前页重放重建交互，但不得覆盖已经提交或刚读入的 State。

use super::support::{action, export_save, import_save, pending, ready, with_runtime};
use narrava_loom_protocol::{HostNodeDto, HostUpdateDto, PendingResult, RuntimeCommand};

fn textbox(frame: &HostUpdateDto) -> (String, String) {
    frame
        .nodes
        .iter()
        .find_map(|node: &HostNodeDto| match node {
            HostNodeDto::Textbox { id, value, .. } => Some((id.clone(), value.clone())),
            _ => None,
        })
        .expect("重绘后仍应有有效输入")
}

#[test]
fn refresh_preserves_saved_input_and_synchronizes_the_rebuilt_control() {
    with_runtime(
        ":: Start\n<<set $name = 'before'>><<textbox \"$name\" \"before\">>\n",
        "",
        |runtime| {
            let start: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
            let saved: HostUpdateDto = ready(
                runtime
                    .execute(RuntimeCommand::Input {
                        interaction: textbox(&start).0,
                        value: "saved".into(),
                    })
                    .unwrap(),
            )
            .1;
            let document: Vec<u8> = export_save(runtime);
            runtime
                .execute(RuntimeCommand::Input {
                    interaction: textbox(&saved).0,
                    value: "changed".into(),
                })
                .unwrap();
            let restored: HostUpdateDto = import_save(runtime, document).1;
            assert_eq!(
                runtime.debug_snapshot().unwrap().state["variables"]["name"],
                "saved"
            );
            assert_eq!(textbox(&restored).1, "saved");

            let operation: u64 = pending(
                runtime
                    .execute(RuntimeCommand::SelectLanguage {
                        locale: "en".into(),
                    })
                    .unwrap(),
            )
            .id();
            let refreshed: HostUpdateDto = ready(
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: Some(PendingResult::SelectLanguage),
                    })
                    .unwrap(),
            )
            .1;
            assert_eq!(textbox(&refreshed).1, "saved");
            assert_eq!(
                runtime.debug_snapshot().unwrap().state["variables"]["name"],
                "saved"
            );
        },
    );
}

#[test]
fn refresh_restores_all_committed_namespaces_before_rendering_specials() {
    let story: &str =
        ":: Start\n<<reset>><<link \"修改\">><<change>><</link>>\n:: Header\n<<inspect>>\n";
    let script: &str = r#"
function setAll(value) {
    State.variables.set('profile', {name:value});
    State.temporary.set('turn', value);
    State.setup.set({label:value});
    State.global.set('label', value);
}
Macro.add('reset', {handler: () => { setAll('before'); return ''; }});
Macro.add('change', {handler: () => { setAll('committed'); return ''; }});
Macro.add('inspect', {handler: () => State.variables.get('profile').name});
"#;
    with_runtime(story, script, |runtime| {
        let start: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
        runtime
            .execute(RuntimeCommand::Activate {
                interaction: action(&start, "修改"),
            })
            .unwrap();
        let before: serde_json::Value = runtime.debug_snapshot().unwrap().state;
        let operation: u64 = pending(
            runtime
                .execute(RuntimeCommand::SelectLanguage {
                    locale: "en".into(),
                })
                .unwrap(),
        )
        .id();
        let refreshed: HostUpdateDto = ready(
            runtime
                .execute(RuntimeCommand::Resume {
                    operation,
                    result: Some(PendingResult::SelectLanguage),
                })
                .unwrap(),
        )
        .1;
        assert_eq!(runtime.debug_snapshot().unwrap().state, before);
        let header: &Vec<HostNodeDto> = refreshed
            .nodes
            .iter()
            .find_map(|node: &HostNodeDto| match node {
                HostNodeDto::Region { region, nodes, .. } if region == "header" => Some(nodes),
                _ => None,
            })
            .expect("应渲染Header");
        assert!(
            header
                .iter()
                .any(|node| matches!(node, HostNodeDto::Text { text, .. } if text == "committed"))
        );
    });
}

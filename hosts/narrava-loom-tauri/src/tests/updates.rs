//! 真实 Worker 的可选呈现帧必须穿过 Host facade，不能在输入或平台操作后丢失。

use std::{fs, future::Future, path::PathBuf};

use serde::Serialize;

use crate::{
    HostDebugSnapshotDto, HostErrorDto, HostNodeDto, HostReplaceTargetDto, HostUpdateDto, TauriHost,
};

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root: PathBuf = PathBuf::from(format!(
            "target/test-projects/host-updates-{name}-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("contents/scripts")).unwrap();
        fs::create_dir_all(root.join("contents/story")).unwrap();
        fs::write(
            root.join("config.toml"),
            "[game]\nid = 'test.host-updates'\nname = 'Host updates'\nversion = '1.0.0'\ndefault_locale = 'zh-CN'\n",
        )
        .unwrap();
        fs::write(
            root.join("contents/story/main.twee"),
            r#":: Start
<<slot "status">>before<</slot>>
<<textbox "$name" "before">>
<<checkbox "$go" false true>>

:: Notice
name: <<print $name>>

:: Target
arrived: <<print $name>>
"#,
        )
        .unwrap();
        fs::write(
            root.join("contents/scripts/main.js"),
            r#"V.name = "before";
V.go = false;
Reaction.add({id: "name.changed", state: "$name", include: "Notice", replace: "status"});
Reaction.add({id: "go.changed", state: "$go", cond: ({after}) => after === true, goto: "Target"});
"#,
        )
        .unwrap();
        Self(root)
    }

    fn host(&self) -> TauriHost {
        TauriHost::spawn(self.0.to_str().unwrap()).unwrap()
    }

    fn enable_developer(&self) {
        let path: PathBuf = self.0.join("config.toml");
        let mut config: String = fs::read_to_string(&path).unwrap();
        config.push_str("\n[host.tauri]\ndeveloper = true\n");
        fs::write(path, config).unwrap();
    }

    fn set_contents(&self, story: &str, script: &str) {
        fs::write(self.0.join("contents/story/main.twee"), story).unwrap();
        fs::write(self.0.join("contents/scripts/main.js"), script).unwrap();
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

/// 同时检查 IPC 序列化形态：有更新是对象，无更新才能是 null。
fn expect_update(value: impl Serialize) -> HostUpdateDto {
    let json: serde_json::Value = serde_json::to_value(value).unwrap();
    assert!(json.is_object(), "Host 应返回更新帧，实际为 {json}");
    serde_json::from_value(json).unwrap()
}

fn input_id(update: &HostUpdateDto, textbox: bool) -> String {
    update
        .nodes
        .iter()
        .find_map(|node: &HostNodeDto| match node {
            HostNodeDto::Textbox { id, .. } if textbox => Some(id.clone()),
            HostNodeDto::Checkbox { id, .. } if !textbox => Some(id.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn input_returns_reaction_replacement_frame() {
    let project: Project = Project::new("replace");
    let host: TauriHost = project.host();
    let initial: HostUpdateDto = block_on(host.start()).unwrap();
    let update: HostUpdateDto = expect_update(
        block_on(host.input(input_id(&initial, true), serde_json::json!("after"))).unwrap(),
    );
    assert_eq!(update.current, "Start");
    assert!(
        update.nodes.iter().any(|node: &HostNodeDto| matches!(
            node, HostNodeDto::Textbox { value, .. } if value == "after"
        )),
        "完整更新帧中的输入值必须与权威 State 一致"
    );
    assert!(update.nodes.iter().any(|node: &HostNodeDto| matches!(
        node,
        HostNodeDto::Replace { target: HostReplaceTargetDto::Key(key), nodes, .. }
            if key == "status" && nodes.iter().any(|node: &HostNodeDto| matches!(
                node, HostNodeDto::Text { text, .. } if text.contains("after")
            ))
    )));
}

#[test]
fn input_returns_reaction_navigation_and_remembers_new_passage() {
    let project: Project = Project::new("goto");
    let host: TauriHost = project.host();
    let initial: HostUpdateDto = block_on(host.start()).unwrap();
    let update: HostUpdateDto = expect_update(
        block_on(host.input(input_id(&initial, false), serde_json::json!(true))).unwrap(),
    );
    assert_eq!(update.current, "Target");
    assert_eq!(host.current_passage().unwrap().as_deref(), Some("Target"));
    assert!(!update.nodes.iter().any(|node: &HostNodeDto| matches!(
        node,
        HostNodeDto::Checkbox { .. } | HostNodeDto::Textbox { .. }
    )));
}

#[test]
fn unchanged_input_has_no_frame_and_invalid_input_is_rejected() {
    let project: Project = Project::new("unchanged");
    let host: TauriHost = project.host();
    let initial: HostUpdateDto = block_on(host.start()).unwrap();
    let id: String = input_id(&initial, false);
    let result: Option<HostUpdateDto> =
        block_on(host.input(id.clone(), serde_json::json!(false))).unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::Value::Null
    );
    assert!(block_on(host.input(id.clone(), serde_json::json!("invalid"))).is_err());
    let update: HostUpdateDto =
        expect_update(block_on(host.input(id, serde_json::json!(true))).unwrap());
    assert_eq!(update.current, "Target");
}

#[test]
fn direct_save_import_returns_the_restored_frame() {
    let project: Project = Project::new("save");
    let host: TauriHost = project.host();
    let initial: HostUpdateDto = block_on(host.start()).unwrap();
    let id: String = input_id(&initial, true);
    block_on(host.input(id.clone(), serde_json::json!("saved"))).unwrap();
    let exported: Option<HostUpdateDto> =
        block_on(host.save(String::from("export"), String::from("quick"))).unwrap();
    assert_eq!(
        serde_json::to_value(exported).unwrap(),
        serde_json::Value::Null
    );
    block_on(host.input(id, serde_json::json!("changed"))).unwrap();
    let update: HostUpdateDto =
        expect_update(block_on(host.save(String::from("import"), String::from("quick"))).unwrap());
    assert_eq!(update.current, "Start");
    assert!(update.nodes.iter().any(|node: &HostNodeDto| matches!(
        node, HostNodeDto::Textbox { value, .. } if value == "saved"
    )));
}

#[test]
fn direct_language_selection_returns_frame_only_after_start() {
    let project: Project = Project::new("language");
    let host: TauriHost = project.host();
    let selected: Option<HostUpdateDto> =
        block_on(host.select_language(String::from("zh-CN"))).unwrap();
    assert_eq!(
        serde_json::to_value(selected).unwrap(),
        serde_json::Value::Null
    );
    block_on(host.start()).unwrap();
    let update: HostUpdateDto =
        expect_update(block_on(host.select_language(String::from("zh-CN"))).unwrap());
    assert_eq!(update.current, "Start");
    assert!(update.nodes.iter().any(|node: &HostNodeDto| matches!(
        node, HostNodeDto::Textbox { value, .. } if value == "before"
    )));
}

#[test]
fn input_frame_uses_reaction_normalized_value() {
    let project: Project = Project::new("normalize");
    project.set_contents(
        ":: Start\n<<textbox \"$name\" \"before\">>\n<<slot \"status\">>before<</slot>>\n:: Notice\n<<print $name>>\n",
        r#"V.name = "before";
Reaction.add({id: "normalize", state: "$name", cond: ({after}) => {
  if (after !== "raw") return false;
  V.name = "normalized";
  return true;
}, include: "Notice", replace: "status"});"#,
    );
    let host: TauriHost = project.host();
    let initial: HostUpdateDto = block_on(host.start()).unwrap();
    let update: HostUpdateDto = expect_update(
        block_on(host.input(input_id(&initial, true), serde_json::json!("raw"))).unwrap(),
    );
    assert!(update.nodes.iter().any(|node: &HostNodeDto| matches!(
        node, HostNodeDto::Textbox { value, .. } if value == "normalized"
    )));
}

#[test]
fn changed_input_synchronizes_related_controls_without_reactions() {
    let project: Project = Project::new("related-inputs");
    project.set_contents(
        ":: Start\n<<radiobutton \"$choice\" \"a\">>\n<<radiobutton \"$choice\" \"b\">>\n<<checkbox \"$choice\" \"a\" \"b\">>\n:: Header\n<<textbox \"$names[$index + 1]\" \"before\">>\n",
        "V.choice = 'a'; V.names = ['unused', 'before']; V.index = 0;",
    );
    let host: TauriHost = project.host();
    let initial: HostUpdateDto = block_on(host.start()).unwrap();
    let checkbox: String = input_id(&initial, false);
    let update: HostUpdateDto =
        expect_update(block_on(host.input(checkbox, serde_json::json!("b"))).unwrap());
    let selected: Vec<bool> = update
        .nodes
        .iter()
        .filter_map(|node: &HostNodeDto| match node {
            HostNodeDto::Radiobutton { selected, .. } | HostNodeDto::Checkbox { selected, .. } => {
                Some(*selected)
            }
            _ => None,
        })
        .collect();
    assert_eq!(selected, [false, true, true]);
    let nested: String = update
        .nodes
        .iter()
        .find_map(|node: &HostNodeDto| match node {
            HostNodeDto::Region { nodes, .. } => {
                nodes.iter().find_map(|node: &HostNodeDto| match node {
                    HostNodeDto::Textbox { id, .. } => Some(id.clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .unwrap();
    let update: HostUpdateDto =
        expect_update(block_on(host.input(nested, serde_json::json!("nested after"))).unwrap());
    assert!(update.nodes.iter().any(|node: &HostNodeDto| matches!(
        node, HostNodeDto::Region { nodes, .. } if nodes.iter().any(|node: &HostNodeDto| matches!(
            node, HostNodeDto::Textbox { value, .. } if value == "nested after"
        ))
    )));
}

#[test]
fn invalid_input_receivers_fail_without_callbacks_or_state_changes() {
    for (index, receiver) in [
        "$names[setup.pick()]",
        "$names[random(0, 1)]",
        "$names[$index++]",
        "$names[$index = 0]",
    ]
    .iter()
    .enumerate()
    {
        let project: Project = Project::new(&format!("receiver-rejected-{index}"));
        project.enable_developer();
        project.set_contents(
            &format!(":: Start\n<<textbox \"{receiver}\" \"before\">>\n"),
            r#"V.names = ["before", "other"]; V.index = 0; setup.pick = () => { Logger.info("test", "receiver callback ran"); return Math.random(); };"#,
        );
        let host: TauriHost = project.host();
        let before: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
        let error: HostErrorDto = block_on(host.start()).unwrap_err();
        assert_eq!(
            error.code, "macro.input.invalid_receiver",
            "{receiver}: {error:?}"
        );
        let after: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
        assert_eq!(after.state, before.state);
        assert!(
            !after
                .logs
                .iter()
                .any(|record| record.message.contains("receiver callback ran"))
        );
    }
}

#[test]
fn failed_reaction_restores_the_presented_input_before_next_command() {
    let project: Project = Project::new("input-rollback");
    project.set_contents(
        ":: Start\n<<textbox \"$name\" \"before\">>\n<<checkbox \"$go\" false true>>\n",
        "V.name = 'before'; V.go = false; Reaction.add({id: 'broken', state: '$name', include: 'Missing'});",
    );
    let host: TauriHost = project.host();
    let initial: HostUpdateDto = block_on(host.start()).unwrap();
    assert!(block_on(host.input(input_id(&initial, true), serde_json::json!("rejected"))).is_err());
    let update: HostUpdateDto = expect_update(
        block_on(host.input(input_id(&initial, false), serde_json::json!(true))).unwrap(),
    );
    assert!(update.nodes.iter().any(|node: &HostNodeDto| matches!(
        node, HostNodeDto::Textbox { value, .. } if value == "before"
    )));
}

#[test]
fn failed_input_save_restores_state_and_presented_controls() {
    let project: Project = Project::new("input-save-rollback");
    project.set_contents(
        ":: Start\n<<textbox \"$name\" \"before\">>\n<<checkbox \"$go\" false true>>\n",
        "V.name = 'before'; V.go = false; Reaction.add({id: 'save-input', state: '$name', cond: () => { Save.export('quick'); return false; }, include: 'Start'});",
    );
    // 由真实文件 IO 拒绝导出，覆盖 Pending/Resume 后才发生的事务恢复。
    fs::write(project.0.join("save"), "blocked directory").unwrap();
    let host: TauriHost = project.host();
    let initial: HostUpdateDto = block_on(host.start()).unwrap();
    let error =
        block_on(host.input(input_id(&initial, true), serde_json::json!("rejected"))).unwrap_err();
    assert_eq!(error.code, "tauri_host.save");
    let update: HostUpdateDto = expect_update(
        block_on(host.input(input_id(&initial, false), serde_json::json!(true))).unwrap(),
    );
    assert!(update.nodes.iter().any(|node: &HostNodeDto| matches!(
        node, HostNodeDto::Textbox { value, .. } if value == "before"
    )));
}

#[test]
fn only_player_navigation_writes_autosave() {
    let project: Project = Project::new("navigation-autosave");
    let host: TauriHost = project.host();
    block_on(host.start()).unwrap();
    let autosave: PathBuf = project.0.join("save/autosave.nsave");
    assert!(!autosave.exists());
    let refreshed: HostUpdateDto =
        expect_update(block_on(host.select_language(String::from("zh-CN"))).unwrap());
    assert!(!autosave.exists());
    block_on(host.input(input_id(&refreshed, true), serde_json::json!("edited"))).unwrap();
    assert!(!autosave.exists());
    block_on(host.input(input_id(&refreshed, false), serde_json::json!(true))).unwrap();
    assert!(autosave.exists());
    // 写入可辨认哨兵，验证后续刷新、读档和历史操作不会再次自动导出。
    let marker: &[u8] = b"autosave sentinel";
    fs::write(&autosave, marker).unwrap();
    block_on(host.save(String::from("export"), String::from("quick"))).unwrap();
    block_on(host.select_language(String::from("zh-CN"))).unwrap();
    assert_eq!(fs::read(&autosave).unwrap(), marker);
    block_on(host.save(String::from("import"), String::from("quick"))).unwrap();
    assert_eq!(fs::read(&autosave).unwrap(), marker);
    block_on(host.history(true)).unwrap();
    assert_eq!(fs::read(&autosave).unwrap(), marker);
    block_on(host.history(false)).unwrap();
    assert_eq!(fs::read(&autosave).unwrap(), marker);
}

#[test]
fn developer_snapshot_requires_explicit_host_configuration() {
    let project: Project = Project::new("debug-disabled");
    let host: TauriHost = project.host();
    let error: HostErrorDto = block_on(host.debug_snapshot()).unwrap_err();
    assert_eq!(error.code, "tauri_host.developer_disabled");
    let error: HostErrorDto =
        block_on(host.debug_execute("V.name = 'forbidden'".into())).unwrap_err();
    assert_eq!(error.code, "tauri_host.developer_disabled");
}

#[test]
fn developer_snapshot_is_read_only_and_shares_runtime_logs() {
    let project: Project = Project::new("debug-readonly");
    project.enable_developer();
    project.set_contents(
        ":: Start\n<<link [[小镇|Town]]>><</link>>\n:: Town [town outside]\n<<textbox \"$name\" \"before\">>\n",
        "V.name = 'before'; Location.add({id:'town',name:'小镇',bounds:[[0,0],[4,0],[4,4],[0,4]]}); Logger.info('test', 'ready for inspection');",
    );
    let host: TauriHost = project.host();
    let initial: HostUpdateDto = block_on(host.start()).unwrap();
    let interaction: String = initial
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation { id, .. } => Some(id.clone()),
            _ => None,
        })
        .unwrap();
    block_on(host.activate(&interaction)).unwrap();
    assert!(project.0.join("save/autosave.nsave").exists());
    let snapshot: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
    assert_eq!(snapshot.current.as_deref(), Some("Town"));
    assert_eq!(snapshot.state["variables"]["name"], "before");
    assert_eq!(snapshot.location["current"]["place"], "town");
    assert_eq!(
        snapshot.location["current"]["point"],
        serde_json::json!([0, 0])
    );
    assert_eq!(block_on(host.debug_snapshot()).unwrap(), snapshot);
    let error: HostErrorDto =
        block_on(host.input(String::from("missing"), serde_json::json!("bad"))).unwrap_err();
    let after: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
    assert_eq!(after.state, snapshot.state);
    assert_eq!(after.location, snapshot.location);
    assert_eq!(after.logs, block_on(host.logs()).unwrap());
    assert!(
        after
            .logs
            .iter()
            .filter_map(|record| record.diagnostic.as_ref())
            .any(|diagnostic| diagnostic.code == error.code
                && diagnostic.message == error.message
                && diagnostic.location == error.location)
    );
}

#[test]
fn developer_snapshot_retains_startup_script_diagnostics() {
    let project: Project = Project::new("debug-startup-error");
    project.enable_developer();
    project.set_contents(":: Start\nHello\n", "throw new Error('startup failed');");
    let host: TauriHost = project.host();
    let error: HostErrorDto = block_on(host.debug_snapshot()).unwrap_err();
    assert!(error.message.contains("startup failed"));
    assert_eq!(error.location.as_ref().unwrap().source, "scripts/main.js");
    assert_eq!(block_on(host.start()).unwrap_err(), error);
    assert_eq!(
        block_on(host.logs()).unwrap()[0].diagnostic.as_ref(),
        Some(&error)
    );
}

#[test]
fn option_inputs_compare_structural_values_without_following_state_cycles() {
    let project: Project = Project::new("input-cycle");
    project.set_contents(
        ":: Start\n<<checkbox \"$cyclic\" null [ [1] ]>>\n<<radiobutton \"$cyclic\" [ [1] ]>>\n<<checkbox \"$object\" null {label: \"a\", values: [1, 2]}>>\n<<checkbox \"$go\" false true>>\n",
        "V.cyclic = [[1]]; V.object = {values:[1,2],label:'a'}; V.go = false; Reaction.add({id:'cycle',state:'$go',cond:()=>{ V.cyclic = []; V.cyclic.push(V.cyclic); return false; },include:'Start'});",
    );
    let host: TauriHost = project.host();
    let initial: HostUpdateDto = block_on(host.start()).unwrap();
    let selected = |update: &HostUpdateDto| -> Vec<bool> {
        update
            .nodes
            .iter()
            .filter_map(|node: &HostNodeDto| match node {
                HostNodeDto::Checkbox { selected, .. }
                | HostNodeDto::Radiobutton { selected, .. } => Some(*selected),
                _ => None,
            })
            .collect()
    };
    assert_eq!(selected(&initial), [true, true, true, false]);
    let trigger: String = initial
        .nodes
        .iter()
        .find_map(|node: &HostNodeDto| match node {
            HostNodeDto::Checkbox { id, checked, .. } if *checked == serde_json::json!(true) => {
                Some(id.clone())
            }
            _ => None,
        })
        .unwrap();
    let update: HostUpdateDto =
        expect_update(block_on(host.input(trigger, serde_json::json!(true))).unwrap());
    assert_eq!(selected(&update), [false, false, true, true]);
}

#[test]
fn developer_console_executes_in_author_realm_and_settles_reactions() {
    let project: Project = Project::new("console-reaction");
    project.enable_developer();
    let host: TauriHost = project.host();
    block_on(host.start()).unwrap();
    let before: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
    assert!(
        block_on(host.debug_execute("V.name".into()))
            .unwrap()
            .is_none()
    );
    let queried: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
    assert_eq!(queried.state, before.state);
    assert_eq!(queried.evaluation.unwrap().value.preview, "\"before\"");
    let frame: HostUpdateDto = block_on(host.debug_execute("V.name = 'console edit'".into()))
        .unwrap()
        .unwrap();
    assert!(format!("{frame:?}").contains("console edit"));
    assert!(
        frame.nodes.iter().any(
            |node| matches!(node, HostNodeDto::Textbox { value, .. } if value == "console edit")
        )
    );
    let frame: HostUpdateDto = block_on(host.debug_execute("V.go = true".into()))
        .unwrap()
        .unwrap();
    assert_eq!(frame.current, "Target");
    assert!(
        !project.0.join("save/autosave.nsave").exists(),
        "开发命令不覆盖自动存档"
    );
}

#[test]
fn developer_console_rolls_back_sync_and_async_failures_without_leaking_jobs() {
    let project: Project = Project::new("console-rollback");
    project.enable_developer();
    let host: TauriHost = project.host();
    block_on(host.start()).unwrap();
    let before: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
    for source in [
        "V.name = 'bad'; Math.random(); throw new Error('rollback')",
        "V.name = 'bad'; Math.random(); Promise.resolve().then(() => { throw new Error('rejected') })",
        "V.name = 'bad'; Host.delay(1); 3",
        "V.name = 'bad'; Engine.goto(123)",
        "V.name = 'bad'; while (true) {}",
        "V.name = ",
        "Save.export('leaked'); V.name = 'bad'; throw new Error('discard save')",
    ] {
        let error: HostErrorDto = block_on(host.debug_execute(source.into())).unwrap_err();
        assert!(error.code.starts_with("console."), "{error:?}");
        let after: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
        assert_eq!(after.state, before.state, "{source}");
        assert_eq!(after.current, before.current, "{source}");
        block_on(host.debug_execute("V.name".into())).unwrap();
        assert_eq!(
            block_on(host.debug_snapshot())
                .unwrap()
                .evaluation
                .unwrap()
                .value
                .preview,
            "\"before\""
        );
    }
    // 真实游戏命令仍可运行并排空正常任务队列；先前 Promise 不能在这里偷偷改状态。
    let frame: HostUpdateDto = block_on(host.debug_execute("V.go = true".into()))
        .unwrap()
        .unwrap();
    assert_eq!(frame.current, "Target");
    assert_eq!(
        block_on(host.debug_snapshot()).unwrap().state["variables"]["name"],
        "before"
    );
    assert!(!project.0.join("save/leaked.nsave").exists());
}

#[test]
fn developer_console_bounds_results_and_persists_state() {
    let project: Project = Project::new("console-save");
    project.enable_developer();
    let host: TauriHost = project.host();
    block_on(host.start()).unwrap();
    block_on(host.debug_execute("(() => { const a = {text: '<img onerror=bad>'}; a.self = a; Object.defineProperty(a, 'getter', {enumerable:true, get() {throw new Error('must not execute')}}); return a })()".into())).unwrap();
    let result = block_on(host.debug_snapshot())
        .unwrap()
        .evaluation
        .unwrap()
        .value;
    assert_eq!(result.kind, "object");
    assert!(result.children.iter().any(|node| node.kind == "reference"));
    assert!(result.children.iter().any(|node| node.kind == "accessor"));
    assert!(
        result
            .children
            .iter()
            .any(|node| node.preview.contains("<img onerror=bad>"))
    );
    block_on(host.debug_execute("V.name = 'saved'; Math.random()".into())).unwrap();
    let saved: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
    block_on(host.debug_execute("Save.export('console')".into())).unwrap();
    block_on(host.debug_execute("V.name = 'changed'; Math.random()".into())).unwrap();
    block_on(host.save("import".into(), "console".into())).unwrap();
    let restored: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
    assert_eq!(restored.state["variables"], saved.state["variables"]);
    block_on(host.debug_execute("'x'.repeat(10000)".into())).unwrap();
    assert!(
        block_on(host.debug_snapshot())
            .unwrap()
            .evaluation
            .unwrap()
            .value
            .preview
            .len()
            < 5000
    );
}

#[test]
fn developer_console_inspects_all_apis_and_completes_without_getters() {
    let project: Project = Project::new("console-objects");
    project.enable_developer();
    let host: TauriHost = project.host();
    block_on(host.start()).unwrap();
    for root in narrava_loom_protocol::contract::GLOBALS {
        block_on(host.debug_execute((*root).into())).unwrap();
        let result = block_on(host.debug_snapshot())
            .unwrap()
            .evaluation
            .unwrap()
            .value;
        assert!(result.kind == "object", "{root}: {result:?}");
        assert!(!result.preview.contains("=>"));
    }
    let members = block_on(host.debug_complete("Save".into())).unwrap();
    assert!(members.iter().any(|node| node.name == "export"
        && node.signature.contains("target")
        && !node.help.is_empty()));
    block_on(
        host.debug_execute(
            "Object.defineProperty(globalThis, 'trap', {get() { V.name = 'bad'; return {x:1} }})"
                .into(),
        ),
    )
    .unwrap();
    assert!(
        block_on(host.debug_complete("trap".into()))
            .unwrap()
            .is_empty()
    );
    assert!(
        block_on(host.debug_complete("Math.random()".into()))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        block_on(host.debug_snapshot()).unwrap().state["variables"]["name"],
        "before"
    );
    assert!(
        block_on(host.debug_complete("V".into()))
            .unwrap()
            .iter()
            .any(|node| node.name == "name")
    );
}

#[test]
fn developer_console_navigation_waits_and_queries_use_the_game_runtime() {
    let project: Project = Project::new("console-engine");
    project.enable_developer();
    let host: TauriHost = project.host();
    block_on(host.start()).unwrap();
    block_on(host.debug_execute("await Host.delay(1); V.name = 'after wait';".into())).unwrap();
    let frame = block_on(host.debug_execute("Engine.goto('Target')".into()))
        .unwrap()
        .unwrap();
    assert_eq!(frame.current, "Target");
    block_on(host.debug_execute("Story.current()".into())).unwrap();
    let result = block_on(host.debug_snapshot())
        .unwrap()
        .evaluation
        .unwrap()
        .value;
    assert!(
        result
            .children
            .iter()
            .any(|node| node.name == "name" && node.preview == "\"Target\"")
    );
    assert_eq!(
        block_on(host.debug_execute("Engine.back()".into()))
            .unwrap()
            .unwrap()
            .current,
        "Start"
    );
    assert_eq!(
        block_on(host.debug_execute("Engine.forward()".into()))
            .unwrap()
            .unwrap()
            .current,
        "Target"
    );
    block_on(host.debug_execute("Promise.resolve(42)".into())).unwrap();
    assert_eq!(
        block_on(host.debug_snapshot())
            .unwrap()
            .evaluation
            .unwrap()
            .value
            .preview,
        "42"
    );
    let error = block_on(host.debug_execute(
        "await Host.delay(1); V.name = 'bad'; Math.random(); throw new Error('after wait')".into(),
    ))
    .unwrap_err();
    assert!(error.message.contains("after wait"));
    assert_eq!(
        block_on(host.debug_snapshot()).unwrap().state["variables"]["name"],
        "after wait"
    );
    block_on(host.debug_execute("Save.export('async');".into())).unwrap();
    let frame = block_on(host.debug_execute("Engine.restart()".into()))
        .unwrap()
        .unwrap();
    assert_eq!(frame.current, "Start");
    assert_eq!(
        block_on(host.debug_snapshot()).unwrap().state["variables"]["name"],
        "before"
    );
    block_on(host.debug_execute("Save.import('async')".into())).unwrap();
    assert_eq!(
        block_on(host.debug_snapshot()).unwrap().state["variables"]["name"],
        "after wait"
    );
    let error = block_on(
        host.debug_execute("const again = () => Promise.resolve().then(again); again()".into()),
    )
    .unwrap_err();
    assert_eq!(error.code, "console.jobs");
    block_on(host.debug_execute("V.name".into())).unwrap();
}

#[test]
fn developer_console_stop_cancels_wait_and_rolls_back() {
    let project: Project = Project::new("console-stop");
    project.enable_developer();
    let host: TauriHost = project.host();
    block_on(host.start()).unwrap();
    let before: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
    std::thread::scope(|scope| {
        let command =
            scope.spawn(|| {
                block_on(host.debug_execute(
            "V.name = 'pending'; Math.random(); await Host.delay(10000); V.name = 'leaked'".into(),
        ))
            });
        // 等到 Runtime 确认挂起，避免取消信号早于命令入口的重置。
        let deadline: std::time::Instant =
            std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if block_on(host.debug_snapshot())
                .is_err_and(|error| error.code == "runtime_session.pending")
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "console did not suspend"
            );
            std::thread::yield_now();
        }
        host.debug_cancel().unwrap();
        assert_eq!(
            command.join().unwrap().unwrap_err().code,
            "console.cancelled"
        );
    });
    let after: HostDebugSnapshotDto = block_on(host.debug_snapshot()).unwrap();
    assert_eq!(after.state, before.state);
    block_on(host.debug_execute("V.name".into())).unwrap();
}

#[test]
fn developer_console_does_not_execute_proxy_traps_when_inspecting() {
    let project: Project = Project::new("console-proxy");
    project.enable_developer();
    let host: TauriHost = project.host();
    block_on(host.start()).unwrap();
    block_on(host.debug_execute("globalThis.opaque = new Proxy({}, {ownKeys(){ V.name = 'bad'; throw new Error('trap'); }}); opaque".into())).unwrap();
    assert_eq!(
        block_on(host.debug_snapshot())
            .unwrap()
            .evaluation
            .unwrap()
            .value
            .kind,
        "proxy"
    );
    assert!(
        block_on(host.debug_complete("opaque".into()))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        block_on(host.debug_snapshot()).unwrap().state["variables"]["name"],
        "before"
    );
}

#[test]
fn engine_root_seed_reaches_scripts_before_worker_start() {
    use narrava_loom_core::engine::Engine;
    let project: Project = Project::new("engine-seed");
    let config = fs::read_to_string(project.0.join("config.toml")).unwrap();
    fs::write(
        project.0.join("config.toml"),
        format!("{config}\n[engine]\nseed = 42\n"),
    )
    .unwrap();
    fs::write(
        project.0.join("contents/scripts/main.js"),
        "V.seed = Engine.seed; V.first = Math.random();",
    )
    .unwrap();
    fs::write(
        project.0.join("contents/story/main.twee"),
        ":: Start\n<<set $second = random()>>\n",
    )
    .unwrap();
    project.enable_developer();
    let host: TauriHost = project.host();
    block_on(host.start()).unwrap();
    let values = block_on(host.debug_snapshot()).unwrap().state["variables"].clone();
    let expected = Engine::new(42);
    assert_eq!(values["seed"], "42");
    assert_eq!(values["first"].as_f64(), Some(expected.next_random()));
    assert_eq!(values["second"].as_f64(), Some(expected.next_random()));
}

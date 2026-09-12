use super::support::{action, export_save, import_save, navigate, ready, text, with_runtime};
use narrava_loom_protocol::{
    HostNodeDto, PendingOperation, PendingResult, RuntimeCommand, RuntimeUpdate,
};

const PLACES: &str = r#"
World.add({id:'town',name:'新手镇',bounds:[[0,0],[100,0],[100,100],[0,100]],entry:[5,5]});
World.add({id:'hospital',name:'医院',parent:'town',bounds:[[20,20],[40,20],[40,40],[20,40]],entry:[25,25]});
Macro.add('where', {handler: () => JSON.stringify(World.current())});
Macro.add('step', {handler: () => { World.move([26,27]); return ''; }});
Macro.add('failMove', {handler: () => { World.move([28,29]); throw new Error('failed world action'); }});
"#;

const STORY: &str = r#":: Start
开始菜单。<<where>>
<<link [[开始游戏|Town]]>><</link>>
:: Town [town outside]
<<where>>
<<link [[医院|Hospital]]>><</link>>
:: Hospital [hospital inside]
<<where>>
<<include "TownDescription">>
<<where>>
<<link [[诊室|Consultation]]>><</link>>
<<link [[错误分支|Broken]]>><</link>>
:: Consultation [hospital inside]
<<step>><<where>>
<<link [[复诊|Followup]]>><</link>>
:: Followup [hospital inside]
<<where>>
<<link [[回镇|Town]]>><</link>>
:: Broken [hospital inside]
<<failMove>>
:: TownDescription [town outside]
这段描述不会把玩家带回镇中心。
"#;

#[test]
fn world_menu_has_no_location_and_tags_bind_registered_ids() {
    with_runtime(STORY, PLACES, |runtime| {
        let (_, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
        assert!(text(&start).contains("null"));
        let (_, town) = navigate(runtime, &start, "Town");
        assert!(text(&town).contains(r#""place":"town""#));
        assert!(text(&town).contains(r#""environment":"outside""#));
        let (_, hospital) = navigate(runtime, &town, "Hospital");
        assert_eq!(text(&hospital).matches(r#""place":"hospital""#).count(), 2);
        assert!(!text(&hospital).contains(r#""place":"town""#));
        let (_, consultation) = navigate(runtime, &hospital, "Consultation");
        let (_, followup) = navigate(runtime, &consultation, "Followup");
        assert!(text(&followup).contains("[26,27]"));
        assert!(text(&followup).contains(r#""environment":"inside""#));
    });
}

#[test]
fn world_history_restores_position_and_returns_to_unlocated_menu() {
    with_runtime(STORY, PLACES, |runtime| {
        let (_, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
        let (_, town) = navigate(runtime, &start, "Town");
        let (_, hospital) = navigate(runtime, &town, "Hospital");
        let _consultation = navigate(runtime, &hospital, "Consultation");
        let (_, previous) = ready(runtime.execute(RuntimeCommand::Back).unwrap());
        assert_eq!(previous.current, "Hospital");
        assert!(text(&previous).contains("[25,25]"));
        let (_, next) = ready(runtime.execute(RuntimeCommand::Forward).unwrap());
        assert!(text(&next).contains("[26,27]"));
        for _ in 0..3 {
            runtime.execute(RuntimeCommand::Back).unwrap();
        }
        let (_, menu) = ready(runtime.execute(RuntimeCommand::Forward).unwrap());
        assert_eq!(menu.current, "Town");
        let (_, menu) = ready(runtime.execute(RuntimeCommand::Back).unwrap());
        assert_eq!(menu.current, "Start");
        assert!(text(&menu).contains("null"));
    });
}

#[test]
fn world_failed_navigation_rolls_back_location_and_coordinates() {
    with_runtime(STORY, PLACES, |runtime| {
        let (_, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
        let (_, town) = navigate(runtime, &start, "Town");
        let (_, hospital) = navigate(runtime, &town, "Hospital");
        let interaction: String = hospital
            .nodes
            .iter()
            .find_map(|node| match node {
                HostNodeDto::Navigation {
                    id,
                    target: Some(target),
                    ..
                } if target == "Broken" => Some(id.clone()),
                _ => None,
            })
            .unwrap();
        assert!(
            runtime
                .execute(RuntimeCommand::Activate { interaction })
                .is_err()
        );
        // 失败位移不提交历史；回到 Hospital 时仍使用之前的坐标。
        let (_, previous) = ready(runtime.execute(RuntimeCommand::Back).unwrap());
        assert_eq!(previous.current, "Town");
        let (_, hospital) = ready(runtime.execute(RuntimeCommand::Forward).unwrap());
        assert!(text(&hospital).contains("[25,25]"));
    });
}

#[test]
fn world_rejects_ambiguous_place_and_environment_tags_before_start() {
    for tags in ["town hospital", "town inside outside"] {
        let story: String = format!(":: Start\nmenu\n:: Invalid [{tags}]\ninvalid\n");
        with_runtime(&story, PLACES, |runtime| {
            assert_eq!(
                runtime.execute(RuntimeCommand::Start).unwrap_err().code,
                "world.passage_tags"
            );
        });
    }
}

#[test]
fn world_save_restores_location_after_another_place_was_visited() {
    with_runtime(STORY, PLACES, |runtime| {
        let (_, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
        let (_, town) = navigate(runtime, &start, "Town");
        let (_, hospital) = navigate(runtime, &town, "Hospital");
        let (_, consultation) = navigate(runtime, &hospital, "Consultation");
        let (_, followup) = navigate(runtime, &consultation, "Followup");
        let document: Vec<u8> = export_save(runtime);
        let _town = navigate(runtime, &followup, "Town");
        let (_, restored) = import_save(runtime, document);
        assert_eq!(restored.current, "Followup");
        assert!(text(&restored).contains("[26,27]"));
        assert!(text(&restored).contains(r#""place":"hospital""#));
    });
}

#[test]
fn world_negative_position_survives_untagged_navigation_save_and_history() {
    let story: &str = r#":: Start
<<link [[开始|Hospital]]>><</link>>
:: Hospital [hospital inside]
<<where>>
<<link "移动">><<step>><<where>><</link>>
<<link [[复诊|Followup]]>><</link>>
:: Followup
<<where>>
<<link [[回镇|Town]]>><</link>>
:: Town [town outside]
<<where>>
"#;
    let script: &str = r#"
World.add({id:'town',bounds:[[-100,-100],[100,-100],[100,100],[-100,100]],entry:[5,5]});
World.add({id:'hospital',parent:'town',bounds:[[-40,-40],[-20,-40],[-20,-20],[-40,-20]],entry:[-30,-30]});
Macro.add('where', {handler: () => JSON.stringify(World.current())});
Macro.add('step', {handler: () => { World.move([-26,-27]); return ''; }});
"#;
    with_runtime(story, script, |runtime| {
        let (_, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
        let (_, hospital) = navigate(runtime, &start, "Hospital");
        assert!(text(&hospital).contains("[-30,-30]"));
        let interaction: String = action(&hospital, "移动");
        let (_, moved) = ready(
            runtime
                .execute(RuntimeCommand::Activate { interaction })
                .unwrap(),
        );
        assert!(text(&moved).contains("[-26,-27]"));
        let (_, followup) = navigate(runtime, &moved, "Followup");
        assert!(text(&followup).contains("[-26,-27]"));
        assert!(text(&followup).contains(r#""environment":"inside""#));

        let document: Vec<u8> = export_save(runtime);
        let (_, town) = navigate(runtime, &followup, "Town");
        assert!(text(&town).contains("[5,5]"));
        let (_, restored) = import_save(runtime, document);
        assert_eq!(restored.current, "Followup");
        assert!(text(&restored).contains("[-26,-27]"));
        assert!(text(&restored).contains(r#""place":"hospital""#));

        // 回溯重放入页前快照；前进仍保留 Followup 入页时的负坐标。
        let (_, previous) = ready(runtime.execute(RuntimeCommand::Back).unwrap());
        assert_eq!(previous.current, "Hospital");
        assert!(text(&previous).contains("[-30,-30]"));
        let (_, next) = ready(runtime.execute(RuntimeCommand::Forward).unwrap());
        assert_eq!(next.current, "Followup");
        assert!(text(&next).contains("[-26,-27]"));
    });
}

#[test]
fn world_refresh_preserves_action_position_and_does_not_repeat_body_movement() {
    let story: &str = r#":: Start
<<link [[进入|Hospital]]>><</link>>
:: Hospital [hospital inside]
<<initialStep>><<where>>
<<link "移动">><<step>><<where>><</link>>
<<link [[离开|Town]]>><</link>>
:: Town [town outside]
<<where>>
"#;
    let script: String = format!(
        "{PLACES}\nMacro.add('initialStep', {{handler: () => {{ World.move([25,26]); return ''; }} }});"
    );
    with_runtime(story, &script, |runtime| {
        let (_, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
        let (_, hospital) = navigate(runtime, &start, "Hospital");
        assert!(text(&hospital).contains("[25,26]"));
        let interaction: String = action(&hospital, "移动");
        let (_, moved) = ready(
            runtime
                .execute(RuntimeCommand::Activate { interaction })
                .unwrap(),
        );
        assert!(text(&moved).contains("[26,27]"));

        let RuntimeUpdate::Pending {
            operation: PendingOperation::SelectLanguage { operation, .. },
        } = runtime
            .execute(RuntimeCommand::SelectLanguage {
                locale: "en".into(),
            })
            .unwrap()
        else {
            panic!("language request")
        };
        let (_, refreshed) = ready(
            runtime
                .execute(RuntimeCommand::Resume {
                    operation,
                    result: Some(PendingResult::SelectLanguage),
                })
                .unwrap(),
        );
        assert!(text(&refreshed).contains("[26,27]"));
        assert!(!text(&refreshed).contains("[25,26]"));

        let document: Vec<u8> = export_save(runtime);
        let _town = navigate(runtime, &refreshed, "Town");
        let (_, restored) = import_save(runtime, document);
        assert_eq!(restored.current, "Hospital");
        assert!(text(&restored).contains("[26,27]"));
        assert!(!text(&restored).contains("[25,26]"));
        let (_, town) = navigate(runtime, &restored, "Town");
        assert!(text(&town).contains(r#""place":"town""#));
    });
}

#[test]
fn world_location_is_available_to_lifecycle_reaction_conditions() {
    let script: String = format!(
        r#"{PLACES}
Reaction.add({{id:'hospital.welcome', lifecycle:true, cond:() => World.current()?.place === 'hospital', widget:'医院事件已触发'}});
"#
    );
    with_runtime(STORY, &script, |runtime| {
        let (_, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
        assert!(!text(&start).contains("医院事件已触发"));
        let (_, town) = navigate(runtime, &start, "Town");
        assert!(!text(&town).contains("医院事件已触发"));
        let (_, hospital) = navigate(runtime, &town, "Hospital");
        assert_eq!(text(&hospital).matches("医院事件已触发").count(), 1);
    });
}

#[test]
fn world_refresh_releases_position_when_the_replayed_passage_navigates() {
    for target in ["Town", "Hospital"] {
        let story: &str = r#":: Start
<<link [[进入|Hospital]]>><</link>>
:: Hospital [hospital inside]
<<redirect>><<where>>
<<link "准备">><<arm>><</link>>
:: Town [town outside]
<<stepInTown>><<where>>
"#;
        let script: String = format!(
            r#"{PLACES}
let redirect = false;
Macro.add('arm', {{handler: () => {{ redirect = true; return ''; }} }});
Macro.add('redirect', {{handler: () => {{
  if (!redirect) return '';
  redirect = false;
  World.move([26,27]);
  return '';
}} }});
Reaction.add({{id:'redirect', lifecycle:true, cond:() => redirect, goto:'{target}', limit:1}});
Macro.add('stepInTown', {{handler:() => {{ World.move([50,50]); return ''; }} }});
"#
        );
        with_runtime(story, &script, |runtime| {
            let (_, start) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
            let (_, hospital) = navigate(runtime, &start, "Hospital");
            let interaction: String = action(&hospital, "准备");
            runtime
                .execute(RuntimeCommand::Activate { interaction })
                .unwrap();
            let RuntimeUpdate::Pending {
                operation: PendingOperation::SelectLanguage { operation, .. },
            } = runtime
                .execute(RuntimeCommand::SelectLanguage {
                    locale: "en".into(),
                })
                .unwrap()
            else {
                panic!("language request")
            };
            let (_, refreshed) = ready(
                runtime
                    .execute(RuntimeCommand::Resume {
                        operation,
                        result: Some(PendingResult::SelectLanguage),
                    })
                    .unwrap(),
            );
            assert_eq!(refreshed.current, target);
            if target == "Town" {
                assert!(text(&refreshed).contains(r#""place":"town""#));
                assert!(text(&refreshed).contains("[50,50]"));
            } else {
                assert!(text(&refreshed).contains("[26,27]"));
            }
        });
    }
}

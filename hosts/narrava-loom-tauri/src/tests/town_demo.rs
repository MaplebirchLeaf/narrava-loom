//! 通过真实示例和 Worker 验证游戏循环；所有存档写入独立临时项目。
use std::{fs, path::PathBuf};

use super::release::{block_on, copy_tree};
use crate::{HostDebugSnapshotDto, HostNodeDto, HostUpdateDto, TauriHost};

struct Demo {
    host: TauriHost,
    root: PathBuf,
}

impl Demo {
    fn new(name: &str) -> Self {
        let repository: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let root: PathBuf = PathBuf::from(format!(
            "target/test-projects/town-{name}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::copy(
            repository.join("examples/config.toml"),
            root.join("config.toml"),
        )
        .unwrap();
        for directory in ["contents", "resources", "languages", "styles"] {
            copy_tree(
                &repository.join("examples").join(directory),
                &root.join(directory),
            );
        }
        let host: TauriHost = TauriHost::spawn(root.to_str().unwrap()).unwrap();
        Self { host, root }
    }

    fn inspect(&self) -> HostDebugSnapshotDto {
        block_on(self.host.debug_snapshot()).unwrap()
    }

    fn click(&self, frame: &HostUpdateDto, label: &str) -> HostUpdateDto {
        let id: &str = frame
            .nodes
            .iter()
            .find_map(|node: &HostNodeDto| match node {
                HostNodeDto::Navigation {
                    id, label: found, ..
                }
                | HostNodeDto::Button {
                    id, label: found, ..
                } if found == label => Some(id.as_str()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing action {label}: {frame:?}"));
        block_on(self.host.activate(id)).unwrap_or_else(|error| panic!("{label}: {error}"))
    }

    fn go(&self, frame: &HostUpdateDto, target: &str) -> HostUpdateDto {
        let id: &str = frame
            .nodes
            .iter()
            .find_map(|node: &HostNodeDto| match node {
                HostNodeDto::Navigation {
                    id,
                    target: Some(found),
                    ..
                }
                | HostNodeDto::Button {
                    id,
                    target: Some(found),
                    ..
                } if found == target => Some(id.as_str()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing destination {target}: {frame:?}"));
        block_on(self.host.activate(id)).unwrap_or_else(|error| panic!("{target}: {error}"))
    }

    fn begin(&self) -> HostUpdateDto {
        let start: HostUpdateDto = block_on(self.host.start()).unwrap();
        let form: HostUpdateDto = self.go(&start, "NewGame");
        self.go(&form, "Bedroom")
    }
}

impl Drop for Demo {
    fn drop(&mut self) {
        // Worker 在 TauriHost drop 时退出；存档操作在各次响应返回前已完成。
        let _result: std::io::Result<()> = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn town_demo_inputs_work_delivery_dialog_and_rest() {
    let demo: Demo = Demo::new("daily-loop");
    let start: HostUpdateDto = block_on(demo.host.start()).unwrap();
    assert!(demo.inspect().location["current"].is_null());
    let form: HostUpdateDto = demo.go(&start, "NewGame");
    let name_id: String = form
        .nodes
        .iter()
        .find_map(|node: &HostNodeDto| match node {
            HostNodeDto::Textbox { id, value, .. } if value == "Alex" => Some(id.clone()),
            _ => None,
        })
        .expect("角色创建应展示姓名输入");
    let form: HostUpdateDto = block_on(demo.host.input(name_id, serde_json::json!("Morgan")))
        .unwrap()
        .unwrap_or(form);
    assert_eq!(
        form.nodes
            .iter()
            .filter(|node| matches!(node, HostNodeDto::Textbox { .. }))
            .count(),
        1,
        "开局只保留姓名输入，不提供随机种子配置"
    );
    let bedroom: HostUpdateDto = demo.go(&form, "Bedroom");
    assert_eq!(demo.inspect().state["variables"]["town"]["name"], "Morgan");
    let dialog: HostUpdateDto = demo.click(&bedroom, "人物与日记");
    assert!(format!("{dialog:?}").contains("背包"));
    let hall: HostUpdateDto = demo.go(&bedroom, "Guesthouse");
    let hall: HostUpdateDto = demo.click(&hall, "接下委托");
    let street: HostUpdateDto = demo.go(&hall, "ResidentialStreet");
    let high: HostUpdateDto = demo.go(&street, "MarketStreet");
    let cafe: HostUpdateDto = demo.go(&high, "ClockCafe");
    let cafe: HostUpdateDto = demo.click(&cafe, "帮工两小时 · +£18 / -25 体力");
    assert_eq!(
        demo.inspect().state["variables"]["town"]["money"],
        serde_json::json!(38.0)
    );
    assert_eq!(
        demo.inspect().state["variables"]["town"]["energy"],
        serde_json::json!(75.0)
    );
    let high: HostUpdateDto = demo.go(&cafe, "MarketStreet");
    let shop: HostUpdateDto = demo.go(&high, "ShoppingCentre");
    let shop: HostUpdateDto = demo.click(&shop, "购买收音机零件 · £8");
    assert_eq!(
        demo.inspect().state["variables"]["town"]["parcels"],
        serde_json::json!(1.0)
    );
    assert!(
        !format!("{shop:?}").contains("购买收音机零件"),
        "购买后移除重复购买入口"
    );
    let high: HostUpdateDto = demo.go(&shop, "MarketStreet");
    let street: HostUpdateDto = demo.go(&high, "ResidentialStreet");
    let hall: HostUpdateDto = demo.go(&street, "Guesthouse");
    let done: HostUpdateDto = demo.click(&hall, "交付零件");
    let state: serde_json::Value = demo.inspect().state["variables"]["town"].clone();
    assert_eq!(state["quest"], "complete");
    assert_eq!(state["money"], serde_json::json!(45.0));
    assert_eq!(state["parcels"], serde_json::json!(0.0));
    assert!(
        format!("{done:?}").contains("日记新增"),
        "Reaction 应派发后续事件"
    );
    let hall: HostUpdateDto = demo.go(&done, "Guesthouse");
    let bedroom: HostUpdateDto = demo.go(&hall, "Bedroom");
    let _morning: HostUpdateDto = demo.click(&bedroom, "睡到明早");
    let state: serde_json::Value = demo.inspect().state["variables"]["town"].clone();
    assert_eq!(state["minutes"], serde_json::json!(1920.0));
    assert_eq!(state["shifts"], serde_json::json!(0.0));
    assert_eq!(state["energy"], serde_json::json!(100.0));
    assert_eq!(state["quest"], "complete");
}

#[test]
fn town_demo_save_restores_exploration_state_and_map_coordinates() {
    let demo: Demo = Demo::new("replay");
    let bedroom: HostUpdateDto = demo.begin();
    let hall: HostUpdateDto = demo.go(&bedroom, "Guesthouse");
    let street: HostUpdateDto = demo.go(&hall, "ResidentialStreet");
    let edge: HostUpdateDto = demo.go(&street, "ForestEdge");
    let trail: HostUpdateDto = demo.go(&edge, "ForestTrail");
    let before: HostDebugSnapshotDto = demo.inspect();
    let map: HostUpdateDto = demo.go(&trail, "TownMap");
    let location: String = format!("{map:?}");
    assert!(location.contains("-550"));
    assert!(location.contains("林间小径"));
    let trail: HostUpdateDto = demo.click(&map, "收起地图");
    assert_eq!(demo.inspect().location, before.location);
    block_on(demo.host.save("export".into(), "town-day".into())).unwrap();
    let _explored: HostUpdateDto = demo.click(&trail, "探索半小时");
    let after: HostDebugSnapshotDto = demo.inspect();
    assert_eq!(
        after.state["variables"]["town"]["energy"],
        serde_json::json!(85.0)
    );
    let restored: HostUpdateDto = block_on(demo.host.save("import".into(), "town-day".into()))
        .unwrap()
        .unwrap();
    assert_eq!(restored.current, "ForestTrail");
    assert_eq!(
        demo.inspect().state["variables"]["town"],
        before.state["variables"]["town"]
    );
    assert_eq!(demo.inspect().location, before.location);
}

#[test]
fn town_demo_translation_catalog() {
    use narrava_loom_core::{SourceList, hir::HirStory, mir::MirStory, twee};
    let repository: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let demo: Demo = Demo::new("catalog");
    let sources: SourceList = SourceList::discover(&demo.root).unwrap();
    let ast: twee::Story = twee::Story::build(&sources.items).unwrap();
    let hir: HirStory<'_> = HirStory::lower(&ast).unwrap();
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).unwrap();
    let template: narrava_loom_core::i18n::I18nTemplate = mir.i18n().template("en");
    let translated: narrava_loom_core::i18n::I18nTemplate = template
        .apply_nmsg(
            &fs::read_to_string(repository.join("examples/languages/en/translations.nmsg"))
                .unwrap(),
        )
        .unwrap();
    let translated: narrava_loom_core::i18n::I18nTemplate = translated
        .apply_nmsg(
            &fs::read_to_string(repository.join("examples/languages/en/messages/library.nmsg"))
                .unwrap(),
        )
        .unwrap();
    mir.i18n()
        .validate(translated)
        .expect("示例译文必须与当前原文和消息身份一致");
}

#[test]
fn town_demo_settings_save_language_and_hospital_environment() {
    let demo: Demo = Demo::new("settings");
    let bedroom: HostUpdateDto = demo.begin();
    let settings: HostUpdateDto = demo.go(&bedroom, "TownSettings");
    let settings: HostUpdateDto = demo.click(&settings, "保存小镇进度");
    assert!(demo.root.join("save/town-day.nsave").is_file());
    let before: HostDebugSnapshotDto = demo.inspect();
    let english: HostUpdateDto = demo.click(&settings, "English · 翻译示例");
    assert!(format!("{english:?}").contains("Your save keeps story state"));
    assert_eq!(
        demo.inspect().state["variables"]["town"],
        before.state["variables"]["town"]
    );
    let settings: HostUpdateDto = demo.click(&english, "简体中文");
    let settings: HostUpdateDto = demo.click(&settings, "读取小镇进度");
    let bedroom: HostUpdateDto = demo.click(&settings, "继续旅程");
    assert_eq!(bedroom.current, "Bedroom");
    let hall: HostUpdateDto = demo.go(&bedroom, "Guesthouse");
    let street: HostUpdateDto = demo.go(&hall, "ResidentialStreet");
    let high: HostUpdateDto = demo.go(&street, "MarketStreet");
    let hospital: HostUpdateDto = demo.go(&high, "TownHospital");
    let hospital: HostUpdateDto = demo.click(&hospital, "走到窗边等候区");
    let outside: HostUpdateDto = demo.go(&hospital, "HospitalOutside");
    assert_eq!(
        demo.inspect().location["current"]["point"],
        serde_json::json!([-60, 65])
    );
    assert_eq!(demo.inspect().location["current"]["environment"], "outside");
    let hospital: HostUpdateDto = demo.go(&outside, "TownHospital");
    assert_eq!(
        demo.inspect().location["current"]["point"],
        serde_json::json!([-60, 65])
    );
    assert_eq!(demo.inspect().location["current"]["environment"], "inside");
    let outside: HostUpdateDto = demo.go(&hospital, "HospitalOutside");
    let high: HostUpdateDto = demo.go(&outside, "MarketStreet");
    let park: HostUpdateDto = demo.go(&high, "TownPark");
    assert!(
        format!("{park:?}").contains("留一点时间给自己"),
        "跨文件 Widget 应在主线执行"
    );
}

#[test]
fn town_demo_energy_guard_and_after_hours_reaction() {
    let demo: Demo = Demo::new("limits");
    let bedroom: HostUpdateDto = demo.begin();
    let hall: HostUpdateDto = demo.go(&bedroom, "Guesthouse");
    let street: HostUpdateDto = demo.go(&hall, "ResidentialStreet");
    let high: HostUpdateDto = demo.go(&street, "MarketStreet");
    let cafe: HostUpdateDto = demo.go(&high, "ClockCafe");
    let cafe: HostUpdateDto = demo.click(&cafe, "帮工两小时 · +£18 / -25 体力");
    let mut cafe: HostUpdateDto = demo.click(&cafe, "帮工两小时 · +£18 / -25 体力");
    assert!(!format!("{cafe:?}").contains("label: \"帮工两小时"));
    // 14 杯咖啡耗尽 £56，推进到 16:40；后续探索可跨过晚间关门边界。
    for _ in 0..14 {
        cafe = demo.click(&cafe, "窗边喝拿铁 · £4 / 20 分钟");
    }
    assert_eq!(
        demo.inspect().state["variables"]["town"]["money"],
        serde_json::json!(0.0)
    );
    assert!(!format!("{cafe:?}").contains("label: \"窗边喝拿铁"));
    let high: HostUpdateDto = demo.go(&cafe, "MarketStreet");
    let street: HostUpdateDto = demo.go(&high, "ResidentialStreet");
    let edge: HostUpdateDto = demo.go(&street, "ForestEdge");
    let mut trail: HostUpdateDto = demo.go(&edge, "ForestTrail");
    let mut tired: bool = false;
    for _ in 0..6 {
        trail = demo.click(&trail, "探索半小时");
        tired |= format!("{trail:?}").contains("体力不多了");
    }
    assert!(tired, "深层 State 路径应触发低体力 Reaction");
    assert!(!format!("{trail:?}").contains("label: \"探索半小时"));
    let edge: HostUpdateDto = demo.go(&trail, "ForestEdge");
    let street: HostUpdateDto = demo.go(&edge, "ResidentialStreet");
    let high: HostUpdateDto = demo.go(&street, "MarketStreet");
    let hospital: HostUpdateDto = demo.go(&high, "TownHospital");
    let hospital: HostUpdateDto = demo.click(&hospital, "补给与休息 · £5 / 30 分钟");
    let outside: HostUpdateDto = demo.go(&hospital, "HospitalOutside");
    let high: HostUpdateDto = demo.go(&outside, "MarketStreet");
    let street: HostUpdateDto = demo.go(&high, "ResidentialStreet");
    let edge: HostUpdateDto = demo.go(&street, "ForestEdge");
    let closed: HostUpdateDto = demo.go(&edge, "ForestTrail");
    assert_eq!(
        closed.current, "ForestClosed",
        "lifecycle Reaction 在正文前重定向"
    );
}

#[test]
fn author_reference_reactions_replace_chain_and_navigate() {
    let demo: Demo = Demo::new("reference-reactions");
    let start: HostUpdateDto = block_on(demo.host.start()).unwrap();
    let hall: HostUpdateDto = demo.go(&start, "Hall");
    let gallery: HostUpdateDto = demo.go(&hall, "ReactionGallery");
    let gallery: HostUpdateDto = demo.click(&gallery, "触发任务完成 Event");
    let output: String = format!("{gallery:?}");
    assert!(output.contains("旧矿井任务已结算"));
    assert!(output.contains("动态 emit payload"));
    let result: HostUpdateDto = demo.click(&gallery, "把声望从 40 提升到 50");
    assert!(format!("{result:?}").contains("声望首次达到 50"));
    let gallery: HostUpdateDto = demo.go(&result, "ReactionGallery");
    let result: HostUpdateDto = demo.click(&gallery, "由 Reaction 导航");
    assert_eq!(result.current, "ReactionGotoTarget");
    let gallery: HostUpdateDto = demo.go(&result, "ReactionGallery");
    let guarded: HostUpdateDto = demo.click(&gallery, "验证 lifecycle exit");
    assert!(format!("{guarded:?}").contains("截断原正文"));
    assert!(!format!("{guarded:?}").contains("这段正文不应显示"));
}

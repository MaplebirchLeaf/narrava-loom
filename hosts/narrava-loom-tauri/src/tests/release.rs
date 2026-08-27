//! Tauri Host 的发行级集成测试（原内联于 lib.rs，按源码规范收拢）。

use std::{fs, path::Path};

use narrava_loom_core::{
    ProjectConfig, SourceList, nar::NarPackage, package_zip, resource::ResourceCatalog,
};

use crate::save_io::save_file_name;
use crate::{HostNodeDto, HostUpdateDto, TauriHost};

/// 脚本宏 `Host.delay` 会挂起 Engine 事务，Host 睡满后恢复并继续渲染。
#[test]
fn host_delay_suspends_and_resumes_the_engine_transaction() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = format!("target/test-projects/host-delay-{}", std::process::id());
    let root_path = Path::new(&root);
    fs::create_dir_all(root_path.join("contents/scripts")).unwrap();
    fs::create_dir_all(root_path.join("contents/story")).unwrap();
    fs::copy(
        repository.join("examples/config.toml"),
        root_path.join("config.toml"),
    )
    .unwrap();
    fs::write(
        root_path.join("contents/story/main.twee"),
        ":: Start\n<<delayed>><</delayed>>",
    )
    .unwrap();
    fs::write(
            root_path.join("contents/scripts/main.js"),
            "Macro.add('delayed', { body: 'inline', arguments: 'raw', execution: 'async', handler: async () => { await Host.delay(1); return 'resumed' } })",
        )
        .unwrap();

    let host = TauriHost::spawn(&root).unwrap();
    let update = host.start().expect("Tauri Host 应恢复异步事务");
    assert_eq!(update.current, "Start");
    assert!(
        update
            .nodes
            .iter()
            .any(|node| { matches!(node, HostNodeDto::Text { text, .. } if text == "resumed") })
    );

    drop(host);
    fs::remove_dir_all(root_path).unwrap();
}

/// 示例项目经 Host 全流程后，语义节点（region/image/component/replace/表单）到达 DTO。
#[test]
fn example_surface_builder_reaches_tauri_semantic_dtos() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = format!(
        "target/test-projects/surface-example-{}",
        std::process::id()
    );
    let root_path = Path::new(&root);
    if root_path.exists() {
        fs::remove_dir_all(root_path).unwrap();
    }
    for directory in [
        "contents/scripts",
        "contents/story",
        "resources/data",
        "resources/images",
    ] {
        fs::create_dir_all(root_path.join(directory)).unwrap();
    }
    for file in [
        "config.toml",
        "contents/scripts/main.ts",
        "contents/story/main.twee",
        "contents/story/widgets.twee",
        "resources/data/guide.txt",
        "resources/images/loom.svg",
    ] {
        fs::copy(repository.join("examples").join(file), root_path.join(file)).unwrap();
    }
    let host = TauriHost::spawn(&root).unwrap();
    let start = host.start().unwrap();
    let hall_id = start
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation { id, target, .. } if target == "Hall" => Some(id),
            _ => None,
        })
        .unwrap();
    let hall = host.activate(hall_id).unwrap();
    let gallery_id = hall
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation { id, target, .. } if target == "SurfaceGallery" => Some(id),
            _ => None,
        })
        .unwrap();

    let gallery = host.activate(gallery_id).unwrap();

    assert!(
        gallery
            .nodes
            .iter()
            .any(|node| matches!(node, HostNodeDto::Region { region, .. } if region == "bar"))
    );
    assert!(gallery.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::Image { resource, .. } if resource == "images/loom.svg"
    )));
    assert!(gallery.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::Component { capability, .. } if capability == "future-card"
    )));

    let hall = host
        .activate(
            gallery
                .nodes
                .iter()
                .find_map(|node| match node {
                    HostNodeDto::Navigation { id, target, .. } if target == "Hall" => Some(id),
                    _ => None,
                })
                .expect("Gallery 应能返回大厅"),
        )
        .unwrap();
    let forms = host
        .activate(
            hall.nodes
                .iter()
                .find_map(|node| match node {
                    HostNodeDto::Navigation { id, target, .. } if target == "FormGallery" => {
                        Some(id)
                    }
                    _ => None,
                })
                .expect("大厅应提供表单验收入口"),
        )
        .unwrap();
    let checkbox_id: String = forms
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Checkbox { id, selected, .. } if !selected => Some(id.clone()),
            _ => None,
        })
        .expect("表单页应产生未选中的 checkbox");
    host.input(checkbox_id, serde_json::Value::Bool(true))
        .expect("checkbox 值应写回 Worker State");
    let radio_controls: Vec<(String, String)> = forms
        .nodes
        .iter()
        .filter_map(|node| match node {
            HostNodeDto::Radiobutton { id, group, .. } => Some((id.clone(), group.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        radio_controls.len(),
        2,
        "每个 radiobutton 都应有独立交互 ID"
    );
    assert_ne!(radio_controls[0].0, radio_controls[1].0);
    assert_eq!(
        radio_controls[0].1, radio_controls[1].1,
        "绑定同一 receiver 的 radiobutton 应共享互斥组"
    );
    host.input(
        radio_controls[1].0.clone(),
        serde_json::Value::String(String::from("explore")),
    )
    .expect("radiobutton 值应写回 Worker State");

    let button_id: String = forms
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Button { id, target, .. } if target == "Hall" => Some(id.clone()),
            _ => None,
        })
        .expect("表单页应产生语义 button");
    let hall = host
        .activate(button_id.as_str())
        .expect("button 应执行正文后导航");
    assert_eq!(hall.current, "Hall");

    let replace_id: String = hall
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation { id, target, .. } if target == "ReplaceGallery" => {
                Some(id.clone())
            }
            _ => None,
        })
        .expect("大厅应提供 replace 验收入口");
    let replace_gallery = host
        .activate(replace_id.as_str())
        .expect("replace 页面应执行");
    assert!(replace_gallery.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::Container { key, .. } if key == "status-panel"
    )));
    assert!(replace_gallery.nodes.iter().any(|node| matches!(
            node,
            HostNodeDto::Replace { target, nodes, .. }
                if matches!(target, crate::HostReplaceTargetDto::Key(key) if key == "status-panel")
                    && nodes.iter().any(|node| matches!(node, HostNodeDto::Text { text, .. } if text.contains("已被替换")))
        )));
    assert!(replace_gallery.nodes.iter().any(|node| matches!(
            node,
            HostNodeDto::Replace { target, nodes, .. }
                if matches!(target, crate::HostReplaceTargetDto::Region(region) if region == "header")
                    && nodes.iter().any(|node| matches!(node, HostNodeDto::Text { text, .. } if text.contains("替换后的页眉")))
        )));
    fs::remove_dir_all(root_path).unwrap();
}

/// 作者能力（存档/读档/日志/语言）与 print Macro 的 color/style/delay 语义到达 DTO。
#[test]
fn example_author_tools_and_text_gallery_reach_tauri_dtos() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = format!(
        "target/test-projects/author-tools-example-{}",
        std::process::id()
    );
    let root_path = Path::new(&root);
    if root_path.exists() {
        fs::remove_dir_all(root_path).unwrap();
    }
    for directory in [
        "contents/scripts",
        "contents/story",
        "languages/en",
        "resources/data",
        "resources/images",
        "save",
    ] {
        fs::create_dir_all(root_path.join(directory)).unwrap();
    }
    for file in [
        "config.toml",
        "contents/scripts/main.ts",
        "contents/story/main.twee",
        "contents/story/widgets.twee",
        "languages/en/dictionary.json",
        "languages/en/manifest.json",
        "languages/en/translations.nmsg",
        "resources/data/guide.txt",
        "resources/images/loom.svg",
    ] {
        fs::copy(repository.join("examples").join(file), root_path.join(file)).unwrap();
    }
    let host = TauriHost::spawn(&root).unwrap();
    assert!(
        host.languages().unwrap().contains(&String::from("en")),
        "开发目录中的解包语言应作为可导入语言加载"
    );
    host.select_language(String::from("en"))
        .expect("示例解包语言应能被实际选择");
    let start = host.start().unwrap();
    assert!(start.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::Text { text, .. } if text.contains("Welcome")
    )));
    let hall_id = start
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation { id, target, .. } if target == "Hall" => Some(id),
            _ => None,
        })
        .unwrap();
    let hall = host.activate(hall_id).unwrap();

    // 作者能力演示页：存档/读档/日志/语言，随后返回大厅
    let author_tools_id = hall
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation { id, target, .. } if target == "AuthorToolsGallery" => {
                Some(id)
            }
            _ => None,
        })
        .expect("大厅应提供作者能力演示入口");
    let author_tools = host.activate(author_tools_id).unwrap();
    assert!(author_tools.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::Text { text, .. } if text.contains("已请求导出存档")
    )));
    assert!(author_tools.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::Text { text, .. } if text.contains("当前生效：en")
    )));
    assert!(author_tools.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::Text { text, .. } if text.contains("I18n 模板已导出")
    )));
    // 两个返回大厅的按钮：正文执行 loadGame 后导航，存档槽位来自本次进入时的导出
    let buttons: Vec<String> = author_tools
        .nodes
        .iter()
        .filter_map(|node| match node {
            HostNodeDto::Button { id, target, .. } if target == "Hall" => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(buttons.len(), 2, "作者能力演示页应有读档与幽灵槽两个按钮");
    let hall = host.activate(buttons[0].as_str()).unwrap();
    assert_eq!(hall.current, "Hall");
    // 再次进入，点击“读取不存在的槽位”：save 失败只记日志，导航与 presented 不受影响
    let author_tools_again = hall
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation { id, target, .. } if target == "AuthorToolsGallery" => {
                Some(id)
            }
            _ => None,
        })
        .expect("大厅应再次提供作者能力演示入口");
    let author_tools = host.activate(author_tools_again).unwrap();
    let hall = host
        .activate(
            author_tools
                .nodes
                .iter()
                .filter_map(|node| match node {
                    HostNodeDto::Button { id, target, .. } if target == "Hall" => Some(id),
                    _ => None,
                })
                .nth(1)
                .expect("作者能力演示页应提供读取不存在槽位的按钮"),
        )
        .expect("save 失败不应阻塞导航");
    assert_eq!(hall.current, "Hall");

    // Twee 内 Surface：print Macro 的 color/style 组合与对象形式
    let text_gallery_id = hall
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation { id, target, .. } if target == "TextGallery" => Some(id),
            _ => None,
        })
        .expect("大厅应提供 print 演示入口");
    let text_gallery = host.activate(text_gallery_id).unwrap();
    assert!(text_gallery.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::StyledText { text, color, styles, .. }
            if text.contains("正面加粗") && *color == 32 && styles.contains(&"strong")
    )));
    assert!(text_gallery.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::StyledText { text, color, styles, .. }
            if text.contains("警告对象形式") && *color == 24 && styles.contains(&"code")
    )));
    assert!(text_gallery.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::StyledText { text, delay: Some(2000), .. } if text.contains("两秒后出现的文字")
    )));
    assert!(text_gallery.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::StyledText { text, color, .. } if text.contains("63") && *color == 63
    )));

    // 弹窗页签：dialog 区域按结构性标题（heading: 2）划分页面
    let back_to_hall: String = text_gallery
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation { id, target, .. } if target == "Hall" => Some(id.clone()),
            _ => None,
        })
        .expect("TextGallery 应能返回大厅");
    let hall = host.activate(back_to_hall.as_str()).unwrap();
    let dialog_gallery_id = hall
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation { id, target, .. } if target == "DialogGallery" => Some(id),
            _ => None,
        })
        .expect("大厅应提供弹窗演示入口");
    let dialog_gallery = host.activate(dialog_gallery_id).unwrap();
    fn collect_styled<'a>(nodes: &'a [HostNodeDto], out: &mut Vec<&'a HostNodeDto>) {
        for node in nodes {
            if matches!(node, HostNodeDto::StyledText { .. }) {
                out.push(node);
            }
            match node {
                HostNodeDto::Region {
                    nodes: children, ..
                }
                | HostNodeDto::Container {
                    nodes: children, ..
                }
                | HostNodeDto::Replace {
                    nodes: children, ..
                } => collect_styled(children, out),
                HostNodeDto::Component { fallback, .. } => collect_styled(fallback, out),
                _ => {}
            }
        }
    }
    let mut styled: Vec<&HostNodeDto> = Vec::new();
    collect_styled(&dialog_gallery.nodes, &mut styled);
    let headings: Vec<&HostNodeDto> = styled
        .iter()
        .copied()
        .filter(|node| {
            matches!(
                node,
                HostNodeDto::StyledText {
                    heading: Some(2),
                    ..
                }
            )
        })
        .collect();
    assert_eq!(headings.len(), 2, "Dialog 应由两个结构性标题划分两页");
    assert!(headings.iter().any(|node| matches!(
        node,
        HostNodeDto::StyledText { text, .. } if text.contains("第一页")
    )));
    assert!(headings.iter().any(|node| matches!(
        node,
        HostNodeDto::StyledText { text, .. } if text.contains("第二页")
    )));
    let hall = host
        .activate(
            dialog_gallery
                .nodes
                .iter()
                .find_map(|node| match node {
                    HostNodeDto::Navigation { id, target, .. } if target == "Hall" => Some(id),
                    _ => None,
                })
                .expect("DialogGallery 应能返回大厅"),
        )
        .unwrap();

    // 控制流范本：switch / for / while 的真实运行时输出（返回大厅后进入）
    let macro_gallery_id = hall
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation { id, target, .. } if target == "MacroGallery" => Some(id),
            _ => None,
        })
        .expect("大厅应提供控制流范本入口");
    let macro_gallery = host.activate(macro_gallery_id).unwrap();
    let rendered: String = macro_gallery
        .nodes
        .iter()
        .filter_map(|node| match node {
            HostNodeDto::Text { text, .. } | HostNodeDto::StyledText { text, .. } => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(rendered.contains("下雨"), "switch 应命中 rain 分支");
    assert!(rendered.contains("钥匙"), "for-of 应遍历集合值");
    assert!(rendered.contains("火把"), "for-of 应遍历集合值");
    assert!(rendered.contains("地图"), "for-of 应遍历集合值");
    assert!(rendered.contains("1"), "while 应输出 1");
    assert!(
        rendered.contains("3"),
        "while 应输出 3（2 被 continue 跳过）"
    );
    assert!(!rendered.contains("4"), "while 应在 4 前 break");
    assert!(
        rendered.contains("被 include 的提示正文"),
        "include 应原地执行另一 Passage 并渲染其正文"
    );
    assert!(rendered.contains("false"), "unset 后 defined() 应为 false");
    fs::remove_dir_all(root_path).unwrap();
}

/// 仅凭 `game.nar` 发行包即可启动，且不注入开发期 Host 面板。
#[test]
fn packaged_game_starts_without_development_sources() {
    let root = format!("target/test-projects/packaged-host-{}", std::process::id());
    let root_path = Path::new(&root);
    if root_path.exists() {
        fs::remove_dir_all(root_path).unwrap();
    }
    fs::create_dir_all(root_path.join("save")).unwrap();
    let project = format!("target/test-projects/package-source-{}", std::process::id());
    let project_path = Path::new(&project);
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::create_dir_all(project_path.join("contents")).unwrap();
    fs::create_dir_all(project_path.join("resources/data")).unwrap();
    fs::copy(
        repository.join("examples/config.toml"),
        project_path.join("config.toml"),
    )
    .unwrap();
    fs::copy(
        repository.join("examples/contents/story/main.twee"),
        project_path.join("contents/main.twee"),
    )
    .unwrap();
    fs::copy(
        repository.join("examples/contents/scripts/main.ts"),
        project_path.join("contents/main.ts"),
    )
    .unwrap();
    fs::copy(
        repository.join("examples/resources/data/guide.txt"),
        project_path.join("resources/data/guide.txt"),
    )
    .unwrap();
    let config_text = fs::read_to_string(project_path.join("config.toml")).unwrap();
    let config = ProjectConfig::load(&project).unwrap();
    let sources = SourceList::discover(&project).unwrap();
    let resources = ResourceCatalog::discover(&project).unwrap();
    let package = NarPackage::build_release(&config, &sources, &resources, &config_text).unwrap();
    fs::write(
        root_path.join("game.nar"),
        package_zip::encode(package.into_files()).unwrap(),
    )
    .unwrap();

    let host = TauriHost::spawn(&root).unwrap();
    let update: HostUpdateDto = host.start().unwrap();
    assert_eq!(update.current, "Start");
    assert!(
        update.nodes.iter().any(|node| matches!(
            node,
            HostNodeDto::Region { region, nodes, .. } if region == "bar" && !nodes.is_empty()
        )),
        "启动更新必须包含游戏作者定义的 Bar 内容"
    );
    assert!(!update.nodes.iter().any(|node| matches!(
        node,
        HostNodeDto::Component { capability, .. } if capability == "narrava.host-tools"
    )));
    drop(host);
    fs::remove_dir_all(root_path).unwrap();
    fs::remove_dir_all(project_path).unwrap();
}

/// 存档槽位名被限制为安全文件名，不能逃逸存档目录。
#[test]
fn save_slot_name_cannot_escape_save_directory() {
    assert_eq!(save_file_name("quick-1").unwrap(), "quick-1.nsave");
    assert!(save_file_name("../outside").is_err());
    assert!(save_file_name("").is_err());
}

/// 活动弹窗页签的焦点提示只在左/上/右描边，避免底部重画分隔线。
#[test]
fn active_dialog_tab_focus_does_not_draw_a_bottom_separator() {
    let css: &str = include_str!("../../frontend/main.css");
    let selector: &str = "nv-story #dialog-tabs .dialog-tab.active:focus-visible {";
    let rule: &str = css
        .split_once(selector)
        .expect("活动弹窗页签应有独立焦点样式")
        .1
        .split_once('}')
        .expect("活动弹窗页签焦点样式应闭合")
        .0;

    assert!(rule.contains("inset 1px 0"), "焦点提示应绘制左边");
    assert!(rule.contains("inset -1px 0"), "焦点提示应绘制右边");
    assert!(rule.contains("inset 0 1px"), "焦点提示应绘制上边");
    assert!(
        !rule.contains("inset 0 0 0"),
        "四边内描边会在活动页签底部重新画出分隔线"
    );
}

/// 弹窗在侧栏之外的剩余空间水平居中，且 Bar 收起状态由作者控制。
#[test]
fn dialog_centers_in_the_space_outside_the_sidebar() {
    let css: &str = include_str!("../../frontend/main.css");
    let javascript: &str = include_str!("../../frontend/main.js");

    assert!(css.contains("--narrava-sidebar-offset: 17.5em;"));
    assert!(css.contains("calc(100% - var(--narrava-sidebar-offset) - 4em)"));
    assert!(css.contains("translateX(calc(var(--narrava-sidebar-offset) / 2))"));
    assert!(css.contains("nv-story.bar-stowed"));
    assert!(javascript.contains("story.classList.toggle(\"bar-stowed\", stowed)"));
}

/// 弹窗内容不含 Host 自有的存档/语言/日志面板。
#[test]
fn dialog_content_has_no_host_owned_save_language_or_log_panels() {
    let html: &str = include_str!("../../frontend/index.html");
    let javascript: &str = include_str!("../../frontend/main.js");

    assert!(!html.contains("id=\"host-dialog-surface\""));
    assert!(!javascript.contains("openHostPanel"));
    assert!(!javascript.contains("buildSavePanel"));
    assert!(!javascript.contains("buildLanguagePanel"));
    assert!(!javascript.contains("buildLogsPanel"));
}

/// Bar 与弹窗内容全部由作者定义，Host 不注入自己的面板。
#[test]
fn bar_and_dialog_content_are_entirely_author_owned() {
    let html: &str = include_str!("../../frontend/index.html");
    let javascript: &str = include_str!("../../frontend/main.js");
    let story: &str = include_str!("../../../../examples/contents/story/main.twee");
    let script: &str = include_str!("../../../../examples/contents/scripts/main.ts");

    assert!(!html.contains("data-host-panel="));
    assert!(!javascript.contains("narrava.host-tools"));
    assert!(story.contains(":: Bar\n<<barDemo>>"));
    assert!(!script.contains("narrava.host-tools"));
}

/// 侧栏身份与 Bar/BarStowed 两种内容状态均由作者定义。
#[test]
fn sidebar_identity_and_both_content_states_belong_to_the_author() {
    let html: &str = include_str!("../../frontend/index.html");
    let css: &str = include_str!("../../frontend/main.css");
    let javascript: &str = include_str!("../../frontend/main.js");
    let story: &str = include_str!("../../../../examples/contents/story/main.twee");

    assert!(!html.contains("id=\"story-title\""));
    assert!(!javascript.contains("#story-title"));
    assert!(!css.contains("#story-title"));
    assert!(!css.contains("nv-ui-bar.stowed #ui-bar-body {\n  visibility: hidden"));
    assert!(css.contains("nv-ui-bar.stowed #ui-bar-body"));
    assert!(css.contains("width: var(--narrava-control-size)"));
    assert!(javascript.contains("regions.get(\"bar-stowed\")"));
    assert!(story.contains(":: Bar\n<<barDemo>>"));
    assert!(story.contains(":: BarStowed\n<<barStowedDemo>>"));
}

/// 运行时状态放在 Passage Footer 而非侧栏。
#[test]
fn runtime_status_is_in_the_passage_footer_instead_of_the_sidebar() {
    let html: &str = include_str!("../../frontend/index.html");
    let sidebar: &str = html
        .split_once("<div id=\"ui-bar-body\">")
        .expect("侧栏正文应存在")
        .1
        .split_once("</div>\n      </nv-ui-bar>")
        .expect("侧栏正文应闭合")
        .0;
    let footer: &str = html
        .split_once("<footer class=\"passage-footer\"")
        .expect("Passage Footer 应存在")
        .1
        .split_once("</footer>")
        .expect("Passage Footer 应闭合")
        .0;

    assert!(!sidebar.contains("id=\"status\""));
    assert!(footer.contains("id=\"status\""));
    assert!(footer.contains("正在连接 Runtime…"));
}

/// 表单控件提交是局部交互，不应冻结整个 Story 或显示全局保存提示。
#[test]
fn form_changes_do_not_show_a_global_saving_state() {
    let javascript: &str = include_str!("../../frontend/main.js");

    assert!(!javascript.contains("正在保存输入…"));
    assert!(!javascript.contains("setBusy(true, \"正在保存输入"));
}

/// 共享实现保留移动入口和触屏布局，平台工程与真机验收另行完成。
#[test]
fn tauri_host_keeps_mobile_entry_and_touch_layout() {
    let source: &str = include_str!("../lib.rs")
        .split_once("#[cfg(test)]")
        .expect("测试模块前应是生产源码")
        .0;
    let css: &str = include_str!("../../frontend/main.css");

    assert!(source.contains("cfg_attr(mobile, tauri::mobile_entry_point)"));
    assert!(css.contains("@media (pointer: coarse)"));
    assert!(css.contains("env(safe-area-inset-top)"));
}

/// 键盘焦点使用细中性描边而非粗蓝框。
#[test]
fn keyboard_focus_is_thin_and_neutral_instead_of_a_thick_blue_frame() {
    let css: &str = include_str!("../../frontend/main.css");

    assert!(css.contains("--narrava-focus: rgb(221 221 221 / 60%);"));
    assert!(css.contains("box-shadow: inset 0 0 0 1px var(--narrava-focus);"));
    assert!(!css.contains("0 0 0 0.25rem var(--narrava-focus)"));
}

/// Host 默认主题覆盖全部语义字形，作者无需为基本可读性另写 CSS。
#[test]
fn host_theme_styles_every_surface_text_semantic() {
    let css: &str = include_str!("../../frontend/main.css");

    for style in [
        "emphasis", "strong", "code", "quote", "marked", "small", "inserted", "deleted",
    ] {
        assert!(
            css.contains(&format!(".surface-text.text-{style}")),
            "默认主题缺少 {style} 语义字形"
        );
    }
    assert!(css.contains("color: var(--narrava-color, var(--narrava-positive));"));
    assert!(css.contains("color: var(--narrava-color, var(--narrava-negative));"));
}

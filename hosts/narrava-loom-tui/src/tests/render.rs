//! TUI 渲染器与终端交互测试（原内联于 lib.rs，按源码规范收拢）。

use narrava_loom_core::semantic::{SemanticKey, SemanticNode, SemanticOutput, SemanticTarget};
use narrava_loom_core::{
    expression::value::TextValue,
    semantic::{RegionId, TextColor},
};

#[test]
fn runtime_dto_renders_without_borrowing_core_host_update() {
    let update = narrava_loom_protocol::HostUpdateDto {
        current: String::from("Start"),
        can_back: false,
        can_forward: false,
        nodes: vec![narrava_loom_protocol::HostNodeDto::Region {
            key: String::from("bar"),
            region: String::from("bar"),
            nodes: vec![narrava_loom_protocol::HostNodeDto::Navigation {
                key: String::from("next"),
                id: String::from("nav:next"),
                label: String::from("继续"),
                target: String::from("Next"),
            }],
        }],
    };
    let frame = crate::TuiRenderer::default().render_update(&update);
    assert_eq!(frame.current, "Start");
    assert_eq!(frame.interactions[0].id.as_deref(), Some("nav:next"));
    assert_eq!(frame.interactions[0].label, "继续");
}

use crate::{
    TuiCommand, TuiCommandError, TuiFrame, TuiInput, TuiInteraction, TuiOperation, TuiRenderer,
    run_terminal, write_frame,
};

#[test]
fn directional_focus_moves_within_and_between_interaction_groups() {
    let interaction = |group: &str, label: &str| TuiInteraction {
        group: group.to_owned(),
        id: Some(label.to_owned()),
        label: label.to_owned(),
        kind: "button",
        input: None,
    };
    let frame = TuiFrame {
        interactions: vec![
            interaction("page-one", "one-a"),
            interaction("page-one", "one-b"),
            interaction("page-two", "two-a"),
            interaction("page-two", "two-b"),
        ],
        ..TuiFrame::default()
    };
    let mut state = crate::screen::ScreenState::default();

    state.move_vertical(&frame, 1);
    assert_eq!(state.focus(), Some(1));
    state.move_horizontal(&frame, 1);
    assert_eq!(state.focus(), Some(3), "左右切组时保留组内行位置");
    state.move_vertical(&frame, -1);
    assert_eq!(state.focus(), Some(2));
    state.move_vertical(&frame, -1);
    assert_eq!(state.focus(), Some(3), "组内向上越界应循环到组尾");
    state.move_horizontal(&frame, 1);
    assert_eq!(state.focus(), Some(1), "向右越过末组应循环到首组");
}

#[test]
fn dialog_focus_stays_inside_pages_and_left_right_cycles_pages() {
    let interaction = |group: &str, label: &str| TuiInteraction {
        group: group.to_owned(),
        id: Some(label.to_owned()),
        label: label.to_owned(),
        kind: "button",
        input: None,
    };
    let frame = TuiFrame {
        interactions: vec![
            interaction("正文", "outside"),
            interaction("弹窗 · 第一页", "one-a"),
            interaction("弹窗 · 第一页", "one-b"),
            interaction("弹窗 · 第二页", "two-a"),
            interaction("弹窗 · 第二页", "two-b"),
        ],
        dialog_pages: vec![
            crate::TuiDialogPage {
                title: String::from("第一页"),
                group: String::from("弹窗 · 第一页"),
                lines: Vec::new(),
            },
            crate::TuiDialogPage {
                title: String::from("第二页"),
                group: String::from("弹窗 · 第二页"),
                lines: Vec::new(),
            },
        ],
        ..TuiFrame::default()
    };
    let mut state = crate::screen::ScreenState::default();

    state.move_vertical(&frame, 1);
    assert_eq!(state.focus(), Some(2), "弹窗打开时不应聚焦正文动作");
    state.move_horizontal(&frame, 1);
    assert_eq!(state.focus(), Some(4));
    state.move_horizontal(&frame, 1);
    assert_eq!(state.focus(), Some(2), "末页向右应循环到首页");
}

#[test]
fn full_screen_dialog_draws_one_page_and_its_actions_inside_one_border() {
    let frame = TuiFrame {
        interactions: vec![
            TuiInteraction {
                group: String::from("正文"),
                id: Some(String::from("outside")),
                label: String::from("正文动作"),
                kind: "button",
                input: None,
            },
            TuiInteraction {
                group: String::from("弹窗 · 第一页"),
                id: Some(String::from("one")),
                label: String::from("第一页动作"),
                kind: "button",
                input: None,
            },
            TuiInteraction {
                group: String::from("弹窗 · 第二页"),
                id: Some(String::from("two")),
                label: String::from("第二页动作"),
                kind: "button",
                input: None,
            },
        ],
        dialog_pages: vec![
            crate::TuiDialogPage {
                title: String::from("第一页"),
                group: String::from("弹窗 · 第一页"),
                lines: vec![String::from("第一页内容")],
            },
            crate::TuiDialogPage {
                title: String::from("第二页"),
                group: String::from("弹窗 · 第二页"),
                lines: vec![String::from("第二页内容")],
            },
        ],
        ..TuiFrame::default()
    };
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut state = crate::screen::ScreenState::default();
    state.move_vertical(&frame, 0);
    terminal
        .draw(|surface| crate::screen::draw(surface, &frame, &state))
        .unwrap();
    let rendered: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    let compact: String = rendered
        .chars()
        .filter(|character: &char| !character.is_whitespace())
        .collect();

    assert!(compact.contains("第一页内容"));
    assert!(compact.contains("第一页动作"));
    assert!(!compact.contains("第二页内容"));
    assert!(!compact.contains("第二页动作"));
    assert!(!compact.contains("正文动作"));
    assert_eq!(rendered.matches('┌').count(), 1);
}

#[test]
fn full_screen_renderer_preserves_semantic_ansi_color_as_ratatui_style() {
    let line = crate::screen::styled_line("\u{1b}[38;2;81;191;154m天气\u{1b}[0m");
    assert_eq!(line.spans[0].content, "天气");
    assert_eq!(
        line.spans[0].style.fg,
        Some(ratatui::style::Color::Rgb(81, 191, 154))
    );
}

#[test]
fn applied_input_updates_the_local_full_screen_feedback() {
    let mut frame = TuiFrame {
        interactions: vec![TuiInteraction {
            group: String::from("form"),
            id: Some(String::from("hint")),
            label: String::from("[ ]"),
            kind: "checkbox",
            input: Some(TuiInput::Checkbox {
                unchecked: narrava_loom_core::semantic::SemanticValue::Boolean(false),
                checked: narrava_loom_core::semantic::SemanticValue::Boolean(true),
                selected: false,
            }),
        }],
        ..TuiFrame::default()
    };
    crate::screen::apply_input_feedback(
        &mut frame,
        &TuiOperation::Input {
            id: String::from("hint"),
            value: narrava_loom_core::semantic::SemanticValue::Boolean(true),
        },
    );
    assert_eq!(frame.interactions[0].label, "[x]");
}

#[test]
fn tui_platform_save_round_trips_and_rejects_path_escape() {
    let root = std::env::temp_dir().join(format!("narrava-loom-tui-save-{}", std::process::id()));
    let _cleanup_before = std::fs::remove_dir_all(&root);
    let bytes: Vec<u8> = vec![1, 2, 3, 4];

    crate::platform::process_save(
        &root,
        narrava_loom_protocol::SaveOperation::Export,
        "quick",
        Some(bytes.clone()),
    )
    .unwrap();
    let restored = crate::platform::process_save(
        &root,
        narrava_loom_protocol::SaveOperation::Import,
        "quick",
        None,
    )
    .unwrap();

    assert_eq!(restored, Some(bytes));
    assert!(
        crate::platform::process_save(
            &root,
            narrava_loom_protocol::SaveOperation::Import,
            "../escape",
            None,
        )
        .is_err()
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// Region 与 Replace（按 key）就地更新对应终端区域，交互被收集进帧。
#[test]
fn region_and_key_replacements_update_terminal_surfaces() {
    let mut main = SemanticOutput::default();
    main.push_keyed(
        SemanticKey::parse("status").unwrap(),
        SemanticNode::Container {
            presentation: narrava_loom_core::semantic::ContainerPresentation::Plain,
            flow: narrava_loom_core::semantic::ContainerFlow::Stack,
            content: SemanticOutput::from_nodes(vec![SemanticNode::Text(TextValue::from(
                "旧状态",
            ))]),
        },
    )
    .unwrap();
    main.push(SemanticNode::Replace {
        target: SemanticTarget::Key(SemanticKey::parse("status").unwrap()),
        content: SemanticOutput::from_nodes(vec![
            SemanticNode::Text(TextValue::from("新状态")),
            SemanticNode::Navigation {
                id: narrava_loom_core::semantic::InteractionId::parse("status:continue").unwrap(),
                label: TextValue::from("继续"),
                target: String::from("Next"),
                role: narrava_loom_core::semantic::NavigationRole::Link,
            },
        ]),
    });
    main.push(SemanticNode::Region {
        region: RegionId::header(),
        content: SemanticOutput::from_nodes(vec![SemanticNode::Text(TextValue::from("标题"))]),
    });

    let frame = TuiRenderer::default().render("Start", &main);

    assert_eq!(frame.current, "Start");
    assert_eq!(frame.header, ["标题"]);
    assert_eq!(frame.main, ["新状态"]);
    assert_eq!(frame.interactions.len(), 1);
    assert_eq!(frame.interactions[0].label, "继续");
}

/// delay 文本先停放在 `delayed`，超过延迟后 `render_at` 才让其进入正文。
#[test]
fn styled_text_with_delay_is_parked_then_revealed() {
    let output = SemanticOutput::from_nodes(vec![
        SemanticNode::StyledText {
            text: TextValue::from("立即显示"),
            styles: Vec::new(),
            color: TextColor::DEFAULT,
            delay: None,
            heading: None,
        },
        SemanticNode::StyledText {
            text: TextValue::from("两秒后出现"),
            styles: Vec::new(),
            color: TextColor::DEFAULT,
            delay: Some(2000),
            heading: None,
        },
    ]);

    let mut renderer = TuiRenderer::default();
    let now = renderer.render("Delay", &output);
    assert_eq!(now.main, ["立即显示"], "未到延迟的文本不应出现在当前帧");
    assert_eq!(now.delayed.len(), 1);
    assert_eq!(now.delayed[0].region, "main");
    assert_eq!(now.delayed[0].delay_ms, 2000);
    assert_eq!(now.delayed[0].lines, ["两秒后出现"]);

    let later = renderer.render_at("Delay", &output, 2500);
    assert_eq!(later.main, ["立即显示两秒后出现"], "超过延迟后应进入正文");
    assert!(later.delayed.is_empty());
}

/// 结构化 HardBreak 把相邻文本保持为两条终端行。
#[test]
fn explicit_line_break_becomes_two_terminal_lines() {
    let output = SemanticOutput::from_nodes(vec![
        SemanticNode::Text(TextValue::from("第一行")),
        SemanticNode::HardBreak,
        SemanticNode::Text(TextValue::from("第二行")),
    ]);

    let frame = TuiRenderer::default().render("Break", &output);

    assert_eq!(frame.main, ["第一行", "第二行"]);
}

#[test]
fn adjacent_text_styles_and_punctuation_stay_on_the_same_line() {
    let output = SemanticOutput::from_nodes(vec![
        SemanticNode::Text(TextValue::from("你发现了")),
        SemanticNode::StyledText {
            text: TextValue::from(" 发光的钥匙"),
            styles: vec![narrava_loom_core::semantic::TextStyle::Strong],
            color: TextColor::DEFAULT,
            delay: None,
            heading: None,
        },
        SemanticNode::Text(TextValue::from("。")),
    ]);

    let frame = TuiRenderer::default().render("Inline", &output);

    assert_eq!(frame.main, ["你发现了** 发光的钥匙**。"]);
}

#[test]
fn custom_region_falls_back_without_losing_content() {
    let output = SemanticOutput::from_nodes(vec![SemanticNode::Region {
        region: RegionId::parse("hud").unwrap(),
        content: SemanticOutput::from_nodes(vec![SemanticNode::Text(TextValue::from("状态"))]),
    }]);

    let frame = TuiRenderer::default().render("Custom", &output);

    assert_eq!(frame.custom.get("hud").unwrap(), &["状态"]);
}

#[test]
fn protocol_panel_container_becomes_a_bordered_terminal_block() {
    let update = narrava_loom_protocol::HostUpdateDto {
        current: String::from("Panel"),
        can_back: false,
        can_forward: false,
        nodes: vec![narrava_loom_protocol::HostNodeDto::Container {
            key: String::from("status"),
            presentation: narrava_loom_protocol::ContainerPresentationDto::Panel,
            flow: narrava_loom_protocol::ContainerFlowDto::Stack,
            nodes: vec![narrava_loom_protocol::HostNodeDto::Text {
                key: String::from("status:text"),
                text: String::from("体力：42"),
            }],
        }],
    };

    let frame = TuiRenderer::default().render_update(&update);

    assert_eq!(frame.main, ["┌──────────┐", "│ 体力：42 │", "└──────────┘"]);
}

#[test]
fn adjacent_panel_containers_share_terminal_rows() {
    let update = narrava_loom_protocol::HostUpdateDto {
        current: String::from("Panels"),
        can_back: false,
        can_forward: false,
        nodes: ["A", "B"]
            .into_iter()
            .map(|text| narrava_loom_protocol::HostNodeDto::Container {
                key: format!("panel-{text}"),
                presentation: narrava_loom_protocol::ContainerPresentationDto::Panel,
                flow: narrava_loom_protocol::ContainerFlowDto::Row,
                nodes: vec![narrava_loom_protocol::HostNodeDto::Text {
                    key: format!("text-{text}"),
                    text: text.to_owned(),
                }],
            })
            .collect(),
    };

    let frame = TuiRenderer::default().render_update(&update);

    assert_eq!(frame.main, ["┌───┐ ┌───┐", "│ A │ │ B │", "└───┘ └───┘"]);
}

#[test]
fn dialog_pages_have_independent_borders_and_interaction_groups() {
    let page = |key: &str, title: &str, action: &str| {
        vec![
            narrava_loom_protocol::HostNodeDto::StyledText {
                key: format!("{key}-title"),
                text: title.to_owned(),
                styles: Vec::new(),
                color: 0,
                heading: Some(2),
                delay: None,
            },
            narrava_loom_protocol::HostNodeDto::Button {
                key: format!("{key}-button"),
                id: format!("{key}-action"),
                label: action.to_owned(),
                target: String::new(),
            },
        ]
    };
    let update = narrava_loom_protocol::HostUpdateDto {
        current: String::from("Dialog"),
        can_back: false,
        can_forward: false,
        nodes: vec![narrava_loom_protocol::HostNodeDto::Region {
            key: String::from("dialog"),
            region: String::from("dialog"),
            nodes: page("one", "第一页", "默认按钮")
                .into_iter()
                .chain(page("two", "第二页", "危险按钮"))
                .collect(),
        }],
    };

    let frame = TuiRenderer::default().render_update(&update);

    assert_eq!(
        frame
            .dialog
            .iter()
            .filter(|line| line.starts_with('┌'))
            .count(),
        2
    );
    assert_eq!(frame.interactions[0].group, "弹窗 · 第一页");
    assert_eq!(frame.interactions[1].group, "弹窗 · 第二页");
    assert_eq!(frame.dialog_pages.len(), 2);
    assert_eq!(frame.dialog_pages[0].title, "第一页");
    assert!(frame.dialog_pages[0].lines.is_empty());

    let mut output: Vec<u8> = Vec::new();
    write_frame(&mut output, &frame).unwrap();
    let output: String = String::from_utf8(output).unwrap();
    assert!(output.contains("    1. 默认按钮"));
    assert!(output.contains(&format!("{}\n    2. 危险按钮", "-".repeat(80))));
    assert!(!output.contains("弹窗 · 第一页："));
    assert!(!output.contains("弹窗 · 第二页："));
}

#[test]
fn panel_discards_source_layout_whitespace_at_its_edges() {
    let update = narrava_loom_protocol::HostUpdateDto {
        current: String::from("Panel whitespace"),
        can_back: false,
        can_forward: false,
        nodes: vec![narrava_loom_protocol::HostNodeDto::Container {
            key: String::from("panel"),
            presentation: narrava_loom_protocol::ContainerPresentationDto::Panel,
            flow: narrava_loom_protocol::ContainerFlowDto::Stack,
            nodes: vec![narrava_loom_protocol::HostNodeDto::Text {
                key: String::from("text"),
                text: String::from("\n状态\n"),
            }],
        }],
    };

    let frame = TuiRenderer::default().render_update(&update);

    assert_eq!(frame.main, ["┌──────┐", "│ 状态 │", "└──────┘"]);
}

#[test]
fn auxiliary_regions_are_bordered_and_sidebar_variants_are_exclusive() {
    let frame = TuiFrame {
        current: String::from("Regions"),
        header: vec![String::from("标题")],
        main: vec![String::from("正文")],
        footer: vec![String::from("页尾")],
        bar: vec![String::from("展开侧栏")],
        bar_stowed: vec![
            String::from("┌──────────┐"),
            String::from("│ 收起侧栏 │"),
            String::from("└──────────┘"),
        ],
        dialog: vec![
            String::from("┌──────────┐"),
            String::from("│ 弹窗内容 │"),
            String::from("└──────────┘"),
        ],
        ..TuiFrame::default()
    };
    let mut output: Vec<u8> = Vec::new();

    write_frame(&mut output, &frame).unwrap();
    let output: String = String::from_utf8(output).unwrap();

    assert!(output.contains("┌──────┐\n│ 标题 │\n└──────┘"));
    assert!(output.contains("┌──────────┐\n│ 展开侧栏 │\n└──────────┘"));
    assert!(!output.contains("侧栏："));
    assert!(output.contains("┌──────────┐\n│ 弹窗内容 │\n└──────────┘"));
    assert!(output.contains("┌──────┐\n│ 页尾 │\n└──────┘"));
    assert!(!output.contains("页眉："));
    assert!(!output.contains("弹窗："));
    assert!(!output.contains("页脚："));
    assert!(output.contains(&format!("{}\n正文", "=".repeat(80))));
    assert!(!output.contains("正文："));
    assert!(!output.contains("操作："));
    assert!(!output.contains("== Regions =="), "不显示 Passage 标题");
    assert!(
        output.find("│ 展开侧栏 │").unwrap() < output.find("\n正文\n").unwrap(),
        "侧栏应显示在正文上方"
    );

    let mut stowed: TuiFrame = frame;
    stowed.sidebar_mode = crate::TuiSidebarMode::Stowed;
    let mut stowed_output: Vec<u8> = Vec::new();
    write_frame(&mut stowed_output, &stowed).unwrap();
    let stowed_output: String = String::from_utf8(stowed_output).unwrap();
    assert!(!stowed_output.contains("侧栏："));
    assert!(stowed_output.contains("┌──────────┐\n│ 收起侧栏 │\n└──────────┘"));
}

#[test]
fn stowed_sidebar_renders_each_top_level_block_as_a_separate_cell() {
    let update = narrava_loom_protocol::HostUpdateDto {
        current: String::from("Hall"),
        can_back: false,
        can_forward: false,
        nodes: vec![narrava_loom_protocol::HostNodeDto::Region {
            key: String::from("bar-stowed"),
            region: String::from("bar-stowed"),
            nodes: vec![
                narrava_loom_protocol::HostNodeDto::Text {
                    key: String::from("weather"),
                    text: String::from("雨"),
                },
                narrava_loom_protocol::HostNodeDto::Text {
                    key: String::from("condition"),
                    text: String::from("痛"),
                },
                narrava_loom_protocol::HostNodeDto::Text {
                    key: String::from("hint"),
                    text: String::from("!"),
                },
            ],
        }],
    };

    let mut renderer = TuiRenderer::default();
    renderer.toggle_sidebar();
    let frame = renderer.render_update(&update);

    assert_eq!(
        frame.bar_stowed,
        ["┌────┬────┬───┐", "│ 雨 │ 痛 │ ! │", "└────┴────┴───┘"]
    );

    let mut output: Vec<u8> = Vec::new();
    write_frame(&mut output, &frame).unwrap();
    let output: String = String::from_utf8(output).unwrap();
    assert!(output.contains("┌────┬────┬───┐"));
    assert!(!output.contains("┌───────────────────────┐"));
}

#[test]
fn mutually_exclusive_sidebar_regions_do_not_duplicate_the_same_interaction() {
    let navigation = |key: &str| narrava_loom_protocol::HostNodeDto::SafeReturn {
        key: key.to_owned(),
        id: String::from("safe-return:Hall"),
        target: String::from("Hall"),
    };
    let update = narrava_loom_protocol::HostUpdateDto {
        current: String::from("Hall"),
        can_back: false,
        can_forward: false,
        nodes: vec![
            narrava_loom_protocol::HostNodeDto::Region {
                key: String::from("bar"),
                region: String::from("bar"),
                nodes: vec![navigation("bar-return")],
            },
            narrava_loom_protocol::HostNodeDto::Region {
                key: String::from("bar-stowed"),
                region: String::from("bar-stowed"),
                nodes: vec![navigation("bar-stowed-return")],
            },
        ],
    };

    let frame = TuiRenderer::default().render_update(&update);

    assert_eq!(frame.interactions.len(), 1);
    assert_eq!(
        frame.interactions[0].id.as_deref(),
        Some("safe-return:Hall")
    );
}

#[test]
fn empty_header_and_footer_remain_hidden() {
    let frame = TuiFrame {
        current: String::from("Empty chrome"),
        main: vec![String::from("正文")],
        ..TuiFrame::default()
    };
    let mut output: Vec<u8> = Vec::new();

    write_frame(&mut output, &frame).unwrap();
    let output: String = String::from_utf8(output).unwrap();

    assert!(!output.contains("页眉："));
    assert!(!output.contains("页脚："));
}

#[test]
fn replacing_panel_content_keeps_the_panel_boundary() {
    let update = narrava_loom_protocol::HostUpdateDto {
        current: String::from("Panel replace"),
        can_back: false,
        can_forward: false,
        nodes: vec![
            narrava_loom_protocol::HostNodeDto::Container {
                key: String::from("status"),
                presentation: narrava_loom_protocol::ContainerPresentationDto::Panel,
                flow: narrava_loom_protocol::ContainerFlowDto::Stack,
                nodes: vec![narrava_loom_protocol::HostNodeDto::Text {
                    key: String::from("old"),
                    text: String::from("旧"),
                }],
            },
            narrava_loom_protocol::HostNodeDto::Replace {
                key: String::from("replace"),
                target: narrava_loom_protocol::HostReplaceTargetDto::Key(String::from("status")),
                nodes: vec![narrava_loom_protocol::HostNodeDto::Text {
                    key: String::from("new"),
                    text: String::from("新状态"),
                }],
            },
        ],
    };

    let frame = TuiRenderer::default().render_update(&update);

    assert_eq!(frame.main, ["┌────────┐", "│ 新状态 │", "└────────┘"]);
}

#[test]
fn empty_plain_container_remains_a_replace_target() {
    let update = narrava_loom_protocol::HostUpdateDto {
        current: String::from("Empty slot"),
        can_back: false,
        can_forward: false,
        nodes: vec![
            narrava_loom_protocol::HostNodeDto::Container {
                key: String::from("empty"),
                presentation: narrava_loom_protocol::ContainerPresentationDto::Plain,
                flow: narrava_loom_protocol::ContainerFlowDto::Stack,
                nodes: Vec::new(),
            },
            narrava_loom_protocol::HostNodeDto::Replace {
                key: String::from("replace"),
                target: narrava_loom_protocol::HostReplaceTargetDto::Key(String::from("empty")),
                nodes: vec![narrava_loom_protocol::HostNodeDto::Text {
                    key: String::from("replacement"),
                    text: String::from("后来出现"),
                }],
            },
        ],
    };

    let frame = TuiRenderer::default().render_update(&update);

    assert_eq!(frame.main, ["后来出现"]);
}

/// 玩家使用一基序号；文本框必须显式使用 set，避免直接选择时误清空内容。
#[test]
fn terminal_commands_resolve_against_current_frame() {
    let frame = TuiFrame {
        interactions: vec![
            TuiInteraction {
                group: String::from("正文"),
                id: Some(String::from("route:quiet")),
                label: String::from("( )"),
                kind: "radiobutton",
                input: Some(TuiInput::Radio {
                    value: narrava_loom_core::semantic::SemanticValue::Text(String::from("quiet")),
                    selected: false,
                }),
            },
            TuiInteraction {
                group: String::from("正文"),
                id: Some(String::from("name")),
                label: String::from("[旅人]"),
                kind: "textbox",
                input: Some(TuiInput::Text {
                    value: String::from("旅人"),
                }),
            },
        ],
        ..TuiFrame::default()
    };

    assert_eq!(
        TuiCommand::parse("1").unwrap().resolve(&frame).unwrap(),
        TuiOperation::Input {
            id: String::from("route:quiet"),
            value: narrava_loom_core::semantic::SemanticValue::Text(String::from("quiet")),
        }
    );
    assert_eq!(
        TuiCommand::parse("set 2 游侠")
            .unwrap()
            .resolve(&frame)
            .unwrap(),
        TuiOperation::Input {
            id: String::from("name"),
            value: narrava_loom_core::semantic::SemanticValue::Text(String::from("游侠")),
        }
    );
    assert_eq!(
        TuiCommand::parse("2").unwrap().resolve(&frame),
        Err(TuiCommandError::TextNeedsValue)
    );
    assert_eq!(TuiCommand::parse("0"), Err(TuiCommandError::ZeroIndex));
}

/// 输入循环能恢复错误、显示帮助并继续处理后续有效动作。
#[test]
fn terminal_loop_is_operable_with_plain_stdin_and_stdout() {
    let frame = TuiFrame {
        current: String::from("Start"),
        main: vec![String::from("请选择。")],
        interactions: vec![TuiInteraction {
            group: String::from("正文"),
            id: Some(String::from("go")),
            label: String::from("继续"),
            kind: "link",
            input: None,
        }],
        ..TuiFrame::default()
    };
    let mut input = std::io::Cursor::new(b"wat\nhelp\n1\nquit\n".to_vec());
    let mut output = Vec::new();
    let mut activated = false;

    run_terminal(&mut input, &mut output, frame, |operation| {
        if operation
            == (TuiOperation::Activate {
                id: String::from("go"),
            })
        {
            activated = true;
        }
        Ok::<Option<TuiFrame>, &str>(None)
    })
    .unwrap();

    let printed = String::from_utf8(output).unwrap();
    assert!(!printed.contains("== Start =="));
    assert!(printed.contains("未知命令"));
    assert!(printed.contains("set <序号> <文字>"));
    assert!(activated);
}

#[test]
fn image_renders_alt_in_a_box_in_semantic_and_protocol_frames() {
    let output: SemanticOutput = SemanticOutput::from_nodes(vec![SemanticNode::Image {
        resource: String::from("images/forest.png"),
        alt: TextValue::from("森林"),
    }]);
    let mut renderer: TuiRenderer = TuiRenderer::default();
    let frame: TuiFrame = renderer.render("Start", &output);
    assert_eq!(frame.main, ["┌──────┐", "│ 森林 │", "└──────┘"]);
    let update: narrava_loom_protocol::HostUpdateDto = narrava_loom_protocol::HostUpdateDto {
        current: String::from("Start"),
        can_back: false,
        can_forward: false,
        nodes: vec![narrava_loom_protocol::HostNodeDto::Image {
            key: String::from("image"),
            resource: String::from("images/forest.png"),
            alt: String::from("森林"),
        }],
    };
    assert_eq!(renderer.render_update(&update).main, frame.main);
}

#[test]
fn meter_component_renders_the_same_numeric_bar_from_core_and_protocol() {
    use narrava_loom_core::expression::value::Value;
    for (value, min, max, expected) in [
        (72.0, 0.0, 100.0, "体力 ███████░░░ 72/100"),
        (15.0, 10.0, 20.0, "体力 █████░░░░░ 15/20"),
        (120.0, 0.0, 100.0, "体力 ██████████ 120/100"),
        (-1.0, 0.0, 100.0, "体力 ░░░░░░░░░░ -1/100"),
    ] {
        let output: SemanticOutput = narrava_loom_core::macro_runtime::meter(&[
            Value::string("体力"),
            Value::Number(value),
            Value::Number(min),
            Value::Number(max),
        ])
        .unwrap()
        .output;
        let update: narrava_loom_protocol::HostUpdateDto = narrava_loom_protocol::HostUpdateDto {
            current: String::from("Start"),
            can_back: false,
            can_forward: false,
            nodes: vec![narrava_loom_protocol::HostNodeDto::Component {
                key: String::from("stamina"),
                capability: String::from("meter"),
                version: 1,
                properties: serde_json::json!({"label": "体力", "value": value, "min": min, "max": max}),
                fallback: vec![],
            }],
        };
        assert_eq!(
            TuiRenderer::default().render("Start", &output).main,
            [expected]
        );
        assert_eq!(
            TuiRenderer::default().render_update(&update).main,
            [expected]
        );
    }
}

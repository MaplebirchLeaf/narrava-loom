//! TUI 渲染器与终端交互测试（原内联于 lib.rs，按源码规范收拢）。

use narrava_loom_core::{
    expression::value::TextValue,
    semantic::{RegionId, TextColor},
};
use narrava_loom_script::protocol_adapter::{Surface, SurfaceKey, SurfaceNode, SurfaceTarget};

#[test]
fn runtime_dto_renders_without_borrowing_core_host_update() {
    let update = narrava_loom_protocol::HostUpdateDto {
        current: String::from("Start"),
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
    run_terminal,
};

/// Region 与 Replace（按 key）就地更新对应终端区域，交互被收集进帧。
#[test]
fn region_and_key_replacements_update_terminal_surfaces() {
    let mut main = Surface::default();
    main.push_keyed(
        SurfaceKey::parse("status").unwrap(),
        SurfaceNode::Container {
            content: Surface::from_nodes(vec![SurfaceNode::Text(TextValue::from("旧状态"))]),
        },
    )
    .unwrap();
    main.push(SurfaceNode::Replace {
        target: SurfaceTarget::Key(SurfaceKey::parse("status").unwrap()),
        content: Surface::from_nodes(vec![
            SurfaceNode::Text(TextValue::from("新状态")),
            SurfaceNode::Navigation {
                id: narrava_loom_core::semantic::InteractionId::parse("status:continue").unwrap(),
                label: TextValue::from("继续"),
                target: String::from("Next"),
                role: narrava_loom_core::semantic::NavigationRole::Link,
            },
        ]),
    });
    main.push(SurfaceNode::Region {
        region: RegionId::header(),
        content: Surface::from_nodes(vec![SurfaceNode::Text(TextValue::from("标题"))]),
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
    let output = Surface::from_nodes(vec![
        SurfaceNode::StyledText {
            text: TextValue::from("立即显示"),
            styles: Vec::new(),
            color: TextColor::DEFAULT,
            delay: None,
            heading: None,
        },
        SurfaceNode::StyledText {
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
    assert_eq!(
        later.main,
        ["立即显示", "两秒后出现"],
        "超过延迟后应进入正文"
    );
    assert!(later.delayed.is_empty());
}

/// 结构化 HardBreak 把相邻文本保持为两条终端行。
#[test]
fn explicit_line_break_becomes_two_terminal_lines() {
    let output = Surface::from_nodes(vec![
        SurfaceNode::Text(TextValue::from("第一行")),
        SurfaceNode::HardBreak,
        SurfaceNode::Text(TextValue::from("第二行")),
    ]);

    let frame = TuiRenderer::default().render("Break", &output);

    assert_eq!(frame.main, ["第一行", "第二行"]);
}

#[test]
fn custom_region_falls_back_without_losing_content() {
    let output = Surface::from_nodes(vec![SurfaceNode::Region {
        region: RegionId::parse("hud").unwrap(),
        content: Surface::from_nodes(vec![SurfaceNode::Text(TextValue::from("状态"))]),
    }]);

    let frame = TuiRenderer::default().render("Custom", &output);

    assert_eq!(frame.custom.get("hud").unwrap(), &["状态"]);
}

/// 玩家使用一基序号；文本框必须显式使用 set，避免直接选择时误清空内容。
#[test]
fn terminal_commands_resolve_against_current_frame() {
    let frame = TuiFrame {
        interactions: vec![
            TuiInteraction {
                id: Some(String::from("route:quiet")),
                label: String::from("( )"),
                kind: "radiobutton",
                input: Some(TuiInput::Radio {
                    value: narrava_loom_script::protocol_adapter::SurfaceValue::Text(String::from(
                        "quiet",
                    )),
                    selected: false,
                }),
            },
            TuiInteraction {
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
            value: narrava_loom_script::protocol_adapter::SurfaceValue::Text(String::from("quiet")),
        }
    );
    assert_eq!(
        TuiCommand::parse("set 2 游侠")
            .unwrap()
            .resolve(&frame)
            .unwrap(),
        TuiOperation::Input {
            id: String::from("name"),
            value: narrava_loom_script::protocol_adapter::SurfaceValue::Text(String::from("游侠")),
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
    assert!(printed.contains("== Start =="));
    assert!(printed.contains("未知命令"));
    assert!(printed.contains("set <序号> <文字>"));
    assert!(activated);
}

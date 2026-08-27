//! 可直接操作的语义终端示例；不依赖全屏终端 UI 库。

use std::io;

use narrava_loom_core::{
    expression::value::TextValue,
    semantic::{InteractionId, NavigationRole, RegionId, TextColor, TextStyle},
};
use narrava_loom_protocol::{
    Surface, SurfaceInputBinding, SurfaceInputKind, SurfaceKey, SurfaceNode, SurfaceTarget,
    SurfaceValue,
};
use narrava_loom_tui::{TuiFrame, TuiOperation, TuiRenderer, run_terminal};

fn main() {
    let mut marked = false;
    let mut name = String::from("旅人");
    let output = demo_output(marked, &name);
    let mut renderer = TuiRenderer::default();
    let first: TuiFrame = renderer.render("TuiGallery", &output);
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    run_terminal(
        &mut reader,
        &mut writer,
        first,
        |operation| match operation {
            TuiOperation::Activate { id } if id == "demo:hall" => {
                Ok::<Option<TuiFrame>, String>(Some(hall_frame()))
            }
            TuiOperation::Input { id, value } if id == "demo:marked" => {
                let SurfaceValue::Boolean(value) = value else {
                    return Err(String::from("marked 只接受布尔值"));
                };
                marked = value;
                Ok(Some(
                    TuiRenderer::default().render("TuiGallery", &demo_output(marked, &name)),
                ))
            }
            TuiOperation::Input { id, value } if id == "demo:name" => {
                let SurfaceValue::Text(value) = value else {
                    return Err(String::from("name 只接受文字"));
                };
                name = value;
                Ok(Some(
                    TuiRenderer::default().render("TuiGallery", &demo_output(marked, &name)),
                ))
            }
            TuiOperation::Dismiss => Ok(None),
            operation => Err(format!("示例未处理操作：{operation:?}")),
        },
    )
    .expect("终端输入输出应可用");
}

fn hall_frame() -> TuiFrame {
    TuiFrame {
        current: String::from("Hall"),
        main: vec![String::from("已通过编号激活导航。输入 quit 退出。")],
        ..TuiFrame::default()
    }
}

fn demo_output(marked: bool, name: &str) -> Surface {
    let mut output = Surface::default();
    output.push(SurfaceNode::Region {
        region: RegionId::header(),
        content: Surface::from_nodes(vec![SurfaceNode::StyledText {
            text: TextValue::from("Narrava Loom · TUI"),
            styles: vec![TextStyle::Strong],
            color: TextColor::ORANGE,
            delay: None,
            heading: None,
        }]),
    });
    output
        .push_keyed(
            SurfaceKey::parse("status").unwrap(),
            SurfaceNode::Container {
                content: Surface::from_nodes(vec![SurfaceNode::Text(TextValue::from("等待替换"))]),
            },
        )
        .unwrap();
    output.push(SurfaceNode::Replace {
        target: SurfaceTarget::Key(SurfaceKey::parse("status").unwrap()),
        content: Surface::from_nodes(vec![SurfaceNode::StyledText {
            text: TextValue::from("替换完成"),
            styles: vec![TextStyle::Strong],
            color: TextColor::GREEN,
            delay: None,
            heading: None,
        }]),
    });
    output.push(SurfaceNode::StyledText {
        text: TextValue::from("两秒后出现的状态提示"),
        styles: vec![TextStyle::Strong],
        color: TextColor::GREEN,
        delay: Some(2000),
        heading: None,
    });
    output.push(SurfaceNode::Text(TextValue::from("标记状态：")));
    output.push(SurfaceNode::Input {
        id: InteractionId::parse("demo:marked").unwrap(),
        binding: SurfaceInputBinding {
            receiver: String::from("$marked"),
            kind: SurfaceInputKind::Checkbox {
                unchecked: SurfaceValue::Boolean(false),
                checked: SurfaceValue::Boolean(true),
                selected: marked,
            },
        },
    });
    output.push(SurfaceNode::Text(TextValue::from("玩家名：")));
    output.push(SurfaceNode::Input {
        id: InteractionId::parse("demo:name").unwrap(),
        binding: SurfaceInputBinding {
            receiver: String::from("$name"),
            kind: SurfaceInputKind::Text {
                value: TextValue::from(name),
            },
        },
    });
    output.push(SurfaceNode::Navigation {
        id: InteractionId::parse("demo:hall").unwrap(),
        label: TextValue::from("返回大厅"),
        target: String::from("Hall"),
        role: NavigationRole::Link,
    });
    output.push(SurfaceNode::Region {
        region: RegionId::dialog(),
        content: Surface::from_nodes(vec![SurfaceNode::Text(TextValue::from(
            "终端 Host 可把 Dialog 映射为独立面板。",
        ))]),
    });
    output
}

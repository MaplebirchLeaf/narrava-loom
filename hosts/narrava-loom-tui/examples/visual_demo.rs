//! 可直接操作的语义终端示例；不依赖全屏终端 UI 库。

use std::io;

use narrava_loom_core::{
    expression::value::TextValue,
    presentation::{
        InteractionId, NavigationRole, PresentationInputBinding, PresentationInputKind,
        PresentationKey, PresentationNode, PresentationOutput, PresentationRegion,
        PresentationTarget, PresentationValue, TextStyle, TextTone,
    },
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
                let PresentationValue::Boolean(value) = value else {
                    return Err(String::from("marked 只接受布尔值"));
                };
                marked = value;
                Ok(Some(
                    TuiRenderer::default().render("TuiGallery", &demo_output(marked, &name)),
                ))
            }
            TuiOperation::Input { id, value } if id == "demo:name" => {
                let PresentationValue::Text(value) = value else {
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

fn demo_output(marked: bool, name: &str) -> PresentationOutput {
    let mut output = PresentationOutput::default();
    output.push(PresentationNode::Region {
        region: PresentationRegion::Header,
        content: PresentationOutput::from_nodes(vec![PresentationNode::StyledText {
            text: TextValue::from("Narrava Loom · TUI"),
            styles: vec![TextStyle::Strong],
            tone: TextTone::ORANGE,
            delay: None,
            heading: None,
        }]),
    });
    output
        .push_keyed(
            PresentationKey::parse("status").unwrap(),
            PresentationNode::Container {
                content: PresentationOutput::from_nodes(vec![PresentationNode::Text(
                    TextValue::from("等待替换"),
                )]),
            },
        )
        .unwrap();
    output.push(PresentationNode::Replace {
        target: PresentationTarget::Key(PresentationKey::parse("status").unwrap()),
        content: PresentationOutput::from_nodes(vec![PresentationNode::StyledText {
            text: TextValue::from("替换完成"),
            styles: vec![TextStyle::Strong],
            tone: TextTone::GREEN,
            delay: None,
            heading: None,
        }]),
    });
    output.push(PresentationNode::StyledText {
        text: TextValue::from("两秒后出现的状态提示"),
        styles: vec![TextStyle::Strong],
        tone: TextTone::GREEN,
        delay: Some(2000),
        heading: None,
    });
    output.push(PresentationNode::Text(TextValue::from("标记状态：")));
    output.push(PresentationNode::Input {
        id: InteractionId::parse("demo:marked").unwrap(),
        binding: PresentationInputBinding {
            receiver: String::from("$marked"),
            kind: PresentationInputKind::Checkbox {
                unchecked: PresentationValue::Boolean(false),
                checked: PresentationValue::Boolean(true),
                selected: marked,
            },
        },
    });
    output.push(PresentationNode::Text(TextValue::from("玩家名：")));
    output.push(PresentationNode::Input {
        id: InteractionId::parse("demo:name").unwrap(),
        binding: PresentationInputBinding {
            receiver: String::from("$name"),
            kind: PresentationInputKind::Text {
                value: TextValue::from(name),
            },
        },
    });
    output.push(PresentationNode::Navigation {
        id: InteractionId::parse("demo:hall").unwrap(),
        label: TextValue::from("返回大厅"),
        target: String::from("Hall"),
        role: NavigationRole::Link,
    });
    output.push(PresentationNode::Region {
        region: PresentationRegion::Dialog,
        content: PresentationOutput::from_nodes(vec![PresentationNode::Text(TextValue::from(
            "终端 Host 可把 Dialog 映射为独立面板。",
        ))]),
    });
    output
}

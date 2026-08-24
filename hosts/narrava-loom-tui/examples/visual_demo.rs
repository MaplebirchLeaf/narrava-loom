//! Prints one semantic frame so TUI mapping can be inspected without a terminal UI library.

use narrava_loom_core::{
    expression::value::TextValue,
    presentation::{
        InteractionId, NavigationRole, PresentationKey, PresentationNode, PresentationOutput,
        PresentationRegion, PresentationTarget, TextStyle, TextTone,
    },
};
use narrava_loom_tui::{TuiFrame, TuiRenderer};

fn main() {
    let output = demo_output();
    let frame = TuiRenderer::default().render("TuiGallery", &output);
    print_frame(&frame);
}

fn demo_output() -> PresentationOutput {
    let mut output = PresentationOutput::default();
    output.push(PresentationNode::Region {
        region: PresentationRegion::Header,
        content: PresentationOutput::from_nodes(vec![PresentationNode::StyledText {
            text: TextValue::from("Narrava Loom · TUI"),
            styles: vec![TextStyle::Heading1],
            tone: TextTone::Accent,
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
            tone: TextTone::Positive,
        }]),
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

fn print_frame(frame: &TuiFrame) {
    println!("┌─ {} ─────────────────────────────", frame.current);
    print_region("HEADER", &frame.header);
    print_region("MAIN", &frame.main);
    print_region("BAR", &frame.bar);
    print_region("DIALOG", &frame.dialog);
    println!("├─ ACTIONS");
    for (index, interaction) in frame.interactions.iter().enumerate() {
        println!(
            "│ {}. [{}] {}",
            index + 1,
            interaction.kind,
            interaction.label
        );
    }
    println!("└──────────────────────────────────");
}

fn print_region(name: &str, lines: &[String]) {
    println!("├─ {name}");
    if lines.is_empty() {
        println!("│ (empty)");
    } else {
        for line in lines {
            println!("│ {line}");
        }
    }
}

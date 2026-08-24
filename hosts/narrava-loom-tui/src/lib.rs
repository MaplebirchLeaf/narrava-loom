//! Core Presentation 到终端区域、文本和交互列表的最小 Host Renderer。

use std::collections::BTreeMap;

use narrava_loom_core::presentation::{
    NavigationRole, PresentationAction, PresentationInputKind, PresentationNode,
    PresentationOutput, PresentationRegion, PresentationTarget, TextStyle,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TuiInteraction {
    pub id: Option<String>,
    pub label: String,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TuiBlock {
    key: Option<String>,
    lines: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TuiSurface {
    blocks: Vec<TuiBlock>,
}

impl TuiSurface {
    fn lines(&self) -> Vec<String> {
        self.blocks
            .iter()
            .flat_map(|block| block.lines.iter().cloned())
            .collect()
    }

    fn replace_key(&mut self, key: &str, lines: &[String]) -> bool {
        let Some(block) = self
            .blocks
            .iter_mut()
            .find(|block| block.key.as_deref() == Some(key))
        else {
            return false;
        };
        block.lines = lines.to_vec();
        true
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TuiFrame {
    pub current: String,
    pub header: Vec<String>,
    pub main: Vec<String>,
    pub footer: Vec<String>,
    pub bar: Vec<String>,
    pub bar_stowed: Vec<String>,
    pub dialog: Vec<String>,
    pub interactions: Vec<TuiInteraction>,
}

#[derive(Clone, Debug, Default)]
pub struct TuiRenderer {
    surfaces: BTreeMap<&'static str, TuiSurface>,
    interactions: Vec<TuiInteraction>,
}

impl TuiRenderer {
    pub fn render(&mut self, current: &str, output: &PresentationOutput) -> TuiFrame {
        self.surfaces.clear();
        self.interactions.clear();
        self.render_output(PresentationRegion::Main, output);
        self.frame(current)
    }

    fn render_output(&mut self, region: PresentationRegion, output: &PresentationOutput) {
        for (index, node) in output.nodes().iter().enumerate() {
            let key = output.key(index).map(|key| key.as_str().to_owned());
            match node {
                PresentationNode::Region { region, content } => {
                    self.render_output(*region, content)
                }
                PresentationNode::Replace { target, content } => {
                    let lines = render_content(content, &mut self.interactions);
                    match target {
                        PresentationTarget::Region(target) => {
                            self.surface_mut(*target).blocks = vec![TuiBlock { key: None, lines }];
                        }
                        PresentationTarget::Key(target) => {
                            for surface in self.surfaces.values_mut() {
                                if surface.replace_key(target.as_str(), &lines) {
                                    break;
                                }
                            }
                        }
                    }
                }
                _ => {
                    let lines = render_node(node, &mut self.interactions);
                    if !lines.is_empty() {
                        self.surface_mut(region)
                            .blocks
                            .push(TuiBlock { key, lines });
                    }
                }
            }
        }
    }

    fn surface_mut(&mut self, region: PresentationRegion) -> &mut TuiSurface {
        self.surfaces.entry(region_name(region)).or_default()
    }

    fn frame(&self, current: &str) -> TuiFrame {
        TuiFrame {
            current: current.to_owned(),
            header: self.lines(PresentationRegion::Header),
            main: self.lines(PresentationRegion::Main),
            footer: self.lines(PresentationRegion::Footer),
            bar: self.lines(PresentationRegion::Bar),
            bar_stowed: self.lines(PresentationRegion::BarStowed),
            dialog: self.lines(PresentationRegion::Dialog),
            interactions: self.interactions.clone(),
        }
    }

    fn lines(&self, region: PresentationRegion) -> Vec<String> {
        self.surfaces
            .get(region_name(region))
            .map_or_else(Vec::new, TuiSurface::lines)
    }
}

fn render_content(
    output: &PresentationOutput,
    interactions: &mut Vec<TuiInteraction>,
) -> Vec<String> {
    output
        .nodes()
        .iter()
        .flat_map(|node| render_node(node, interactions))
        .collect()
}

fn render_node(node: &PresentationNode, interactions: &mut Vec<TuiInteraction>) -> Vec<String> {
    match node {
        PresentationNode::Text(text) => vec![unicode(text)],
        PresentationNode::StyledText { text, styles, .. } => {
            vec![styled(unicode(text), styles)]
        }
        PresentationNode::Image {
            resource,
            alt,
            caption,
        } => vec![format!(
            "[图像: {} <{}>{}]",
            unicode(alt),
            resource,
            caption
                .as_ref()
                .map(|value| format!(" — {}", unicode(value)))
                .unwrap_or_default()
        )],
        PresentationNode::Component { fallback, .. } => render_content(fallback, interactions),
        PresentationNode::Container { content } => render_content(content, interactions),
        PresentationNode::Action { label, action, .. } => {
            interactions.push(TuiInteraction {
                id: None,
                label: unicode(label),
                kind: match action {
                    PresentationAction::Dismiss => "dismiss",
                },
            });
            Vec::new()
        }
        PresentationNode::Input { id, binding } => {
            let (label, kind) = match &binding.kind {
                PresentationInputKind::Checkbox { selected, .. } => {
                    (if *selected { "[x]" } else { "[ ]" }.to_owned(), "checkbox")
                }
                PresentationInputKind::Radio { selected, .. } => (
                    if *selected { "(o)" } else { "( )" }.to_owned(),
                    "radiobutton",
                ),
                PresentationInputKind::Text { value } => {
                    (format!("[{}]", unicode(value)), "textbox")
                }
            };
            interactions.push(TuiInteraction {
                id: Some(id.as_str().to_owned()),
                label,
                kind,
            });
            Vec::new()
        }
        PresentationNode::Navigation {
            id, label, role, ..
        } => {
            interactions.push(TuiInteraction {
                id: Some(id.as_str().to_owned()),
                label: unicode(label),
                kind: match role {
                    NavigationRole::Link => "link",
                    NavigationRole::Button => "button",
                },
            });
            Vec::new()
        }
        PresentationNode::SafeReturn { id, target } => {
            interactions.push(TuiInteraction {
                id: Some(id.as_str().to_owned()),
                label: format!("返回 {target}"),
                kind: "safe-return",
            });
            Vec::new()
        }
        PresentationNode::Region { content, .. } | PresentationNode::Replace { content, .. } => {
            render_content(content, interactions)
        }
    }
}

fn styled(mut text: String, styles: &[TextStyle]) -> String {
    for style in styles.iter().rev() {
        text = match style {
            TextStyle::Strong => format!("**{text}**"),
            TextStyle::Emphasis => format!("*{text}*"),
            TextStyle::Code => format!("`{text}`"),
            TextStyle::Heading1 => format!("# {text}"),
            TextStyle::Heading2 => format!("## {text}"),
            TextStyle::Heading3 => format!("### {text}"),
            _ => text,
        };
    }
    text
}

fn unicode(value: &narrava_loom_core::expression::value::TextValue) -> String {
    value
        .to_unicode_string()
        .unwrap_or_else(|| String::from("<非 Unicode 文本>"))
}

fn region_name(region: PresentationRegion) -> &'static str {
    match region {
        PresentationRegion::Header => "header",
        PresentationRegion::Main => "main",
        PresentationRegion::Footer => "footer",
        PresentationRegion::Bar => "bar",
        PresentationRegion::BarStowed => "bar-stowed",
        PresentationRegion::Dialog => "dialog",
    }
}

#[cfg(test)]
mod tests {
    use narrava_loom_core::{
        expression::value::TextValue,
        presentation::{
            PresentationKey, PresentationNode, PresentationOutput, PresentationRegion,
            PresentationTarget,
        },
    };

    use super::TuiRenderer;

    #[test]
    fn region_and_key_replacements_update_terminal_surfaces() {
        let mut main = PresentationOutput::default();
        main.push_keyed(
            PresentationKey::parse("status").unwrap(),
            PresentationNode::Container {
                content: PresentationOutput::from_nodes(vec![PresentationNode::Text(
                    TextValue::from("旧状态"),
                )]),
            },
        )
        .unwrap();
        main.push(PresentationNode::Replace {
            target: PresentationTarget::Key(PresentationKey::parse("status").unwrap()),
            content: PresentationOutput::from_nodes(vec![
                PresentationNode::Text(TextValue::from("新状态")),
                PresentationNode::Navigation {
                    id: narrava_loom_core::presentation::InteractionId::parse("status:continue")
                        .unwrap(),
                    label: TextValue::from("继续"),
                    target: String::from("Next"),
                    role: narrava_loom_core::presentation::NavigationRole::Link,
                },
            ]),
        });
        main.push(PresentationNode::Region {
            region: PresentationRegion::Header,
            content: PresentationOutput::from_nodes(vec![PresentationNode::Text(TextValue::from(
                "标题",
            ))]),
        });

        let frame = TuiRenderer::default().render("Start", &main);

        assert_eq!(frame.current, "Start");
        assert_eq!(frame.header, ["标题"]);
        assert_eq!(frame.main, ["新状态"]);
        assert_eq!(frame.interactions.len(), 1);
        assert_eq!(frame.interactions[0].label, "继续");
    }
}

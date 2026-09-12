//! Protocol 节点到 TUI 文本块与交互的适配。

use super::*;

impl TuiRenderer {
    pub(super) fn render_dto_nodes(
        &mut self,
        region: &str,
        nodes: &[HostNodeDto],
        elapsed_ms: u64,
    ) {
        let interaction_group: String = region_group(region);
        for node in nodes {
            let interaction_start: usize = self.interactions.len();
            match node {
                HostNodeDto::Dialog {
                    key,
                    initial,
                    pages,
                } => {
                    self.dialog_key = Some(key.clone());
                    self.dialog_initial.clone_from(initial);
                    self.page_regions.clear();
                    for page in pages {
                        let page_region: String = format!("dialog-page:{}", page.title);
                        let start: usize = self.interactions.len();
                        self.render_dto_nodes(&page_region, &page.nodes, elapsed_ms);
                        for item in &mut self.interactions[start..] {
                            item.group = format!("弹窗 · {}", page.title);
                        }
                        self.page_regions.push((page.title.clone(), page_region));
                    }
                }
                HostNodeDto::Region { region, nodes, .. } => {
                    self.render_dto_nodes(region, nodes, elapsed_ms);
                }
                HostNodeDto::Replace { target, nodes, .. } => {
                    let lines: Vec<String> = render_dto_content(nodes, &mut self.interactions);
                    match target {
                        HostReplaceTargetDto::Region(target) => {
                            self.surfaces.entry(target.clone()).or_default().blocks =
                                vec![TuiBlock {
                                    key: None,
                                    presentation: TuiBlockPresentation::Plain,
                                    flow: TuiBlockFlow::Stack,
                                    lines,

                                    inline: false,
                                }];
                        }
                        HostReplaceTargetDto::Key(target) => {
                            for surface in self.surfaces.values_mut() {
                                if surface.replace_key(target, &lines) {
                                    break;
                                }
                            }
                        }
                    }
                }
                HostNodeDto::StyledText {
                    delay: Some(delay), ..
                } if *delay > elapsed_ms => {
                    let lines: Vec<String> = render_dto_node(node, &mut self.interactions);
                    if !lines.is_empty() {
                        self.delayed.push(TuiDelayedText {
                            region: region.to_owned(),
                            lines,
                            delay_ms: *delay,
                        });
                    }
                }
                _ => {
                    let (lines, presentation, flow): (
                        Vec<String>,
                        TuiBlockPresentation,
                        TuiBlockFlow,
                    ) = match node {
                        HostNodeDto::Container {
                            presentation,
                            flow,
                            nodes,
                            ..
                        } => {
                            let presentation: TuiBlockPresentation = (*presentation).into();
                            let flow: TuiBlockFlow = (*flow).into();
                            let lines: Vec<String> =
                                render_dto_content(nodes, &mut self.interactions);
                            (presentation.render(lines), presentation, flow)
                        }
                        _ => (
                            render_dto_node(node, &mut self.interactions),
                            if matches!(node, HostNodeDto::Image { .. }) {
                                TuiBlockPresentation::Panel
                            } else {
                                TuiBlockPresentation::Plain
                            },
                            TuiBlockFlow::Stack,
                        ),
                    };
                    if !lines.is_empty()
                        || matches!(
                            node,
                            HostNodeDto::Container { .. } | HostNodeDto::HardBreak { .. }
                        )
                    {
                        self.surfaces
                            .entry(region.to_owned())
                            .or_default()
                            .blocks
                            .push(TuiBlock {
                                key: Some(dto_key(node).to_owned()),
                                presentation,
                                flow,
                                lines,

                                inline: matches!(
                                    node,
                                    HostNodeDto::Text { .. } | HostNodeDto::StyledText { .. }
                                ),
                            });
                    }
                }
            }
            self.label_new_interactions(interaction_start, &interaction_group);
        }
    }
}

fn dto_key(node: &HostNodeDto) -> &str {
    match node {
        HostNodeDto::Text { key, .. }
        | HostNodeDto::HardBreak { key }
        | HostNodeDto::StyledText { key, .. }
        | HostNodeDto::Image { key, .. }
        | HostNodeDto::Dialog { key, .. }
        | HostNodeDto::Region { key, .. }
        | HostNodeDto::Container { key, .. }
        | HostNodeDto::Component { key, .. }
        | HostNodeDto::Replace { key, .. }
        | HostNodeDto::Action { key, .. }
        | HostNodeDto::Checkbox { key, .. }
        | HostNodeDto::Radiobutton { key, .. }
        | HostNodeDto::Textbox { key, .. }
        | HostNodeDto::Navigation { key, .. }
        | HostNodeDto::Button { key, .. }
        | HostNodeDto::SafeReturn { key, .. } => key,
    }
}

fn render_dto_content(
    nodes: &[HostNodeDto],
    interactions: &mut Vec<TuiInteraction>,
) -> Vec<String> {
    nodes
        .iter()
        .flat_map(|node| render_dto_node(node, interactions))
        .collect()
}

fn render_dto_node(node: &HostNodeDto, interactions: &mut Vec<TuiInteraction>) -> Vec<String> {
    match node {
        HostNodeDto::Text { text, .. } => visible_lines(text.clone()),
        HostNodeDto::HardBreak { .. } => Vec::new(),
        HostNodeDto::StyledText {
            text,
            styles,
            color,
            heading,
            ..
        } => visible_lines(styled_dto(text.clone(), styles, *color, *heading)),
        HostNodeDto::Image { alt, .. } => panel_lines(vec![alt.clone()]),
        HostNodeDto::Container {
            presentation,
            nodes,
            ..
        } => {
            let presentation: TuiBlockPresentation = (*presentation).into();
            presentation.render(render_dto_content(nodes, interactions))
        }
        HostNodeDto::Component {
            capability,
            version: 1,
            properties,
            ..
        } if capability == "meter" => {
            vec![render_meter(
                properties
                    .get("label")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
                properties.get("value").and_then(serde_json::Value::as_f64),
                properties.get("min").and_then(serde_json::Value::as_f64),
                properties.get("max").and_then(serde_json::Value::as_f64),
            )]
        }
        HostNodeDto::Component { fallback, .. }
        | HostNodeDto::Region {
            nodes: fallback, ..
        }
        | HostNodeDto::Replace {
            nodes: fallback, ..
        } => render_dto_content(fallback, interactions),
        HostNodeDto::Dialog { .. } => Vec::new(),
        HostNodeDto::Action { label, action, .. } => {
            interactions.push(TuiInteraction {
                group: String::new(),
                id: None,
                label: label.clone(),
                kind: match action.as_str() {
                    "dismiss" => "dismiss",
                    _ => "action",
                },
                input: None,
            });
            Vec::new()
        }
        HostNodeDto::Checkbox {
            id,
            unchecked,
            checked,
            selected,
            ..
        } => {
            interactions.push(TuiInteraction {
                group: String::new(),
                id: Some(id.clone()),
                label: if *selected { "[x]" } else { "[ ]" }.to_owned(),
                kind: "checkbox",
                input: Some(TuiInput::Checkbox {
                    unchecked: dto_surface_value(unchecked),
                    checked: dto_surface_value(checked),
                    selected: *selected,
                }),
            });
            Vec::new()
        }
        HostNodeDto::Radiobutton {
            id,
            value,
            selected,
            ..
        } => {
            interactions.push(TuiInteraction {
                group: String::new(),
                id: Some(id.clone()),
                label: if *selected { "(o)" } else { "( )" }.to_owned(),
                kind: "radiobutton",
                input: Some(TuiInput::Radio {
                    value: dto_surface_value(value),
                    selected: *selected,
                }),
            });
            Vec::new()
        }
        HostNodeDto::Textbox { id, value, .. } => {
            interactions.push(TuiInteraction {
                group: String::new(),
                id: Some(id.clone()),
                label: format!("[{value}]"),
                kind: "textbox",
                input: Some(TuiInput::Text {
                    value: value.clone(),
                }),
            });
            Vec::new()
        }
        HostNodeDto::Navigation { id, label, .. } => {
            push_dto_action(interactions, id, label, "link")
        }
        HostNodeDto::Button { id, label, .. } => push_dto_action(interactions, id, label, "button"),
        HostNodeDto::SafeReturn { id, target, .. } => {
            push_dto_action(interactions, id, &format!("返回 {target}"), "safe-return")
        }
    }
}

fn push_dto_action(
    interactions: &mut Vec<TuiInteraction>,
    id: &str,
    label: &str,
    kind: &'static str,
) -> Vec<String> {
    interactions.push(TuiInteraction {
        group: String::new(),
        id: Some(id.to_owned()),
        label: label.to_owned(),
        kind,
        input: None,
    });
    Vec::new()
}

fn dto_surface_value(value: &serde_json::Value) -> SemanticValue {
    match value {
        serde_json::Value::Null => SemanticValue::Null,
        serde_json::Value::Bool(value) => SemanticValue::Boolean(*value),
        serde_json::Value::Number(value) => value
            .as_f64()
            .map(SemanticValue::Number)
            .unwrap_or(SemanticValue::Null),
        serde_json::Value::String(value) => SemanticValue::Text(value.clone()),
        _ => SemanticValue::Null,
    }
}

fn styled_dto(mut text: String, styles: &[String], color: u8, heading: Option<u8>) -> String {
    for style in styles.iter().rev() {
        text = match style.as_str() {
            "strong" => format!("**{text}**"),
            "emphasis" => format!("*{text}*"),
            "code" => format!("`{text}`"),
            "inserted" => format!("++{text}++"),
            "deleted" => format!("~~{text}~~"),
            _ => text,
        };
    }
    decorate_text(text, color, heading.is_some())
}

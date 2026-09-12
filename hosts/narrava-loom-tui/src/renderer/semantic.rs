//! Core 语义输出的直接渲染入口；供嵌入式调用和测试使用。

use super::*;

impl TuiRenderer {
    /// 递归渲染输出树：Region 下钻、Replace 就地覆盖、未到时的延迟文本停放。
    pub(super) fn render_output(
        &mut self,
        region: RegionId,
        output: &SemanticOutput,
        elapsed_ms: u64,
    ) {
        let interaction_group: String = region_group(region.as_str());
        for (index, node) in output.nodes().iter().enumerate() {
            let interaction_start: usize = self.interactions.len();
            let key = output.key(index).map(|key| key.as_str().to_owned());
            match node {
                SemanticNode::Dialog { initial, pages } => {
                    self.dialog_key =
                        Some(key.clone().unwrap_or_else(|| format!("dialog:{index}")));
                    self.dialog_initial.clone_from(initial);
                    self.page_regions.clear();
                    for page in pages {
                        let page_region: String = format!("dialog-page:{}", page.title);
                        let start: usize = self.interactions.len();
                        self.render_output(
                            RegionId::parse(&page_region).expect("nonempty region"),
                            &page.content,
                            elapsed_ms,
                        );
                        for item in &mut self.interactions[start..] {
                            item.group = format!("弹窗 · {}", page.title);
                        }
                        self.page_regions.push((page.title.clone(), page_region));
                    }
                }
                SemanticNode::Region { region, content } => {
                    self.render_output(region.clone(), content, elapsed_ms)
                }
                SemanticNode::Replace { target, content } => {
                    let lines = render_content(content, &mut self.interactions);
                    match target {
                        SemanticTarget::Region(target) => {
                            self.surface_mut(target).blocks = vec![TuiBlock {
                                key: None,
                                presentation: TuiBlockPresentation::Plain,
                                flow: TuiBlockFlow::Stack,
                                lines,

                                inline: false,
                            }];
                        }
                        SemanticTarget::Key(target) => {
                            for surface in self.surfaces.values_mut() {
                                if surface.replace_key(target.as_str(), &lines) {
                                    break;
                                }
                            }
                        }
                    }
                }
                SemanticNode::StyledText {
                    delay: Some(delay), ..
                } if *delay > elapsed_ms => {
                    let lines = render_node(node, &mut self.interactions);
                    if !lines.is_empty() {
                        self.delayed.push(TuiDelayedText {
                            region: region.as_str().to_owned(),
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
                        SemanticNode::Container {
                            presentation,
                            flow,
                            content,
                        } => {
                            let presentation: TuiBlockPresentation = (*presentation).into();
                            let flow: TuiBlockFlow = (*flow).into();
                            let lines: Vec<String> =
                                render_content(content, &mut self.interactions);
                            (presentation.render(lines), presentation, flow)
                        }
                        _ => (
                            render_node(node, &mut self.interactions),
                            if matches!(node, SemanticNode::Image { .. }) {
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
                            SemanticNode::Container { .. } | SemanticNode::HardBreak
                        )
                    {
                        self.surface_mut(&region).blocks.push(TuiBlock {
                            key,
                            presentation,
                            flow,
                            lines,

                            inline: matches!(
                                node,
                                SemanticNode::Text(_) | SemanticNode::StyledText { .. }
                            ),
                        });
                    }
                }
            }
            self.label_new_interactions(interaction_start, &interaction_group);
        }
    }
}

/// 渲染子输出并返回其全部行（顺带把可触发节点收集进 `interactions`）。
fn render_content(output: &SemanticOutput, interactions: &mut Vec<TuiInteraction>) -> Vec<String> {
    output
        .nodes()
        .iter()
        .flat_map(|node| render_node(node, interactions))
        .collect()
}

/// 渲染单个节点：文本/样式文本成行，图像以 alt 方框显示，Action/Input/Navigation/SafeReturn
/// 收集为交互（不产出行）。
fn render_node(node: &SemanticNode, interactions: &mut Vec<TuiInteraction>) -> Vec<String> {
    match node {
        SemanticNode::Text(text) => visible_lines(unicode(text)),
        SemanticNode::HardBreak => Vec::new(),
        SemanticNode::StyledText {
            text,
            styles,
            color,
            heading,
            ..
        } => visible_lines(styled(unicode(text), styles, *color, *heading)),
        SemanticNode::Image { alt, .. } => panel_lines(vec![unicode(alt)]),
        SemanticNode::Component {
            capability,
            version: 1,
            properties,
            ..
        } if capability.as_str() == "meter" => {
            let number = |name: &str| match properties.get(name) {
                Some(SemanticValue::Number(value)) => Some(*value),
                _ => None,
            };
            let label: &str = match properties.get("label") {
                Some(SemanticValue::Text(label)) => label,
                _ => "",
            };
            vec![render_meter(
                label,
                number("value"),
                number("min"),
                number("max"),
            )]
        }
        SemanticNode::Component { fallback, .. } => render_content(fallback, interactions),
        SemanticNode::Container {
            presentation,
            content,
            ..
        } => {
            let presentation: TuiBlockPresentation = (*presentation).into();
            presentation.render(render_content(content, interactions))
        }
        SemanticNode::Dialog { .. } => Vec::new(),
        SemanticNode::Action { label, action, .. } => {
            interactions.push(TuiInteraction {
                group: String::new(),
                id: None,
                label: unicode(label),
                kind: match action {
                    SemanticAction::Dismiss => "dismiss",
                },
                input: None,
            });
            Vec::new()
        }
        SemanticNode::Input { id, binding } => {
            let (label, kind, input) = match &binding.kind {
                SemanticInputKind::Checkbox {
                    unchecked,
                    checked,
                    selected,
                } => (
                    if *selected { "[x]" } else { "[ ]" }.to_owned(),
                    "checkbox",
                    TuiInput::Checkbox {
                        unchecked: unchecked.clone(),
                        checked: checked.clone(),
                        selected: *selected,
                    },
                ),
                SemanticInputKind::Radio {
                    value, selected, ..
                } => (
                    if *selected { "(o)" } else { "( )" }.to_owned(),
                    "radiobutton",
                    TuiInput::Radio {
                        value: value.clone(),
                        selected: *selected,
                    },
                ),
                SemanticInputKind::Text { value } => {
                    let value: String = unicode(value);
                    (format!("[{value}]"), "textbox", TuiInput::Text { value })
                }
            };
            interactions.push(TuiInteraction {
                group: String::new(),
                id: Some(id.as_str().to_owned()),
                label,
                kind,
                input: Some(input),
            });
            Vec::new()
        }
        SemanticNode::Navigation {
            id, label, role, ..
        } => {
            interactions.push(TuiInteraction {
                group: String::new(),
                id: Some(id.as_str().to_owned()),
                label: unicode(label),
                kind: match role {
                    NavigationRole::Link => "link",
                    NavigationRole::Button => "button",
                },
                input: None,
            });
            Vec::new()
        }
        SemanticNode::SafeReturn { id, target } => {
            interactions.push(TuiInteraction {
                group: String::new(),
                id: Some(id.as_str().to_owned()),
                label: format!("返回 {target}"),
                kind: "safe-return",
                input: None,
            });
            Vec::new()
        }
        SemanticNode::Region { content, .. } | SemanticNode::Replace { content, .. } => {
            render_content(content, interactions)
        }
    }
}

/// 按语义样式包裹标记符，结构性标题加粗下划线，并按 color 梯度染色；color 为 0 时不染色。
fn styled(
    mut text: String,
    styles: &[TextStyle],
    color: TextColor,
    heading: Option<HeadingLevel>,
) -> String {
    for style in styles.iter().rev() {
        text = match style {
            TextStyle::Strong => format!("**{text}**"),
            TextStyle::Emphasis => format!("*{text}*"),
            TextStyle::Code => format!("`{text}`"),
            TextStyle::Inserted => format!("++{text}++"),
            TextStyle::Deleted => format!("~~{text}~~"),
            _ => text,
        };
    }
    decorate_text(text, color.index(), heading.is_some())
}

/// TextValue 转 Unicode 字符串；非 Unicode 文本给占位。
fn unicode(value: &narrava_loom_core::expression::value::TextValue) -> String {
    value
        .to_unicode_string()
        .unwrap_or_else(|| String::from("<非 Unicode 文本>"))
}

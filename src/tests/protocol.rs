//! 宿主无关语义输出测试。

use crate::{
    expression::value::TextValue,
    semantic::{
        ActionRole, ComponentCapability, InteractionId, InteractionIdError, RegionId,
        SemanticAction, SemanticKey, SemanticKeyError, SemanticNode, SemanticOutput, SemanticValue,
        TextColor, TextStyle,
    },
};

#[test]
fn output_preserves_semantic_node_order() {
    let web_text: TextValue = TextValue::from_units(vec![0xD800]);
    let output: SemanticOutput = SemanticOutput::from_nodes(vec![
        SemanticNode::Text(TextValue::from("进入森林")),
        SemanticNode::Text(web_text.clone()),
        SemanticNode::Navigation {
            role: crate::semantic::NavigationRole::Link,
            id: InteractionId::from_key("start:choice:0"),
            label: TextValue::from("继续"),
            target: String::from("Forest"),
        },
    ]);

    assert_eq!(output.nodes().len(), 3);
    assert_eq!(output.nodes()[1], SemanticNode::Text(web_text));
    assert!(output.has_navigation());
}

#[test]
fn output_appends_and_merges_without_a_host_renderer() {
    let mut output: SemanticOutput = SemanticOutput::default();
    output.push(SemanticNode::Text(TextValue::from("前")));
    output.append(SemanticOutput::from_nodes(vec![SemanticNode::Text(
        TextValue::from("后"),
    )]));

    assert_eq!(output.len(), 2);
    assert!(!output.is_empty());
    assert_eq!(
        output.nodes(),
        &[
            SemanticNode::Text(TextValue::from("前")),
            SemanticNode::Text(TextValue::from("后")),
        ]
    );
}

#[test]
fn safe_return_is_not_an_author_navigation_action() {
    let output: SemanticOutput = SemanticOutput::from_nodes(vec![SemanticNode::SafeReturn {
        id: InteractionId::from_key("history:2:safe-return"),
        target: String::from("Start"),
    }]);

    assert!(!output.has_navigation());
}

#[test]
fn interaction_identity_resolves_only_core_presented_actions() {
    let id: InteractionId = InteractionId::from_key("start:choice:0");
    let unknown: InteractionId = InteractionId::from_key("start:choice:1");
    let output: SemanticOutput = SemanticOutput::from_nodes(vec![SemanticNode::Navigation {
        role: crate::semantic::NavigationRole::Link,
        id: id.clone(),
        label: TextValue::from("进入森林"),
        target: String::from("Forest"),
    }]);

    assert_eq!(output.interaction_target(&id), Some("Forest"));
    assert_eq!(output.interaction_target(&unknown), None);
}

#[test]
fn binding_cannot_decode_an_empty_interaction_identity() {
    assert_eq!(InteractionId::parse(""), Err(InteractionIdError::Empty));
}

#[test]
fn surface_keys_are_explicit_stable_and_unique_within_one_output() {
    let mut output = SemanticOutput::default();
    let title = SemanticKey::parse("passage:title").unwrap();
    output
        .push_keyed(
            title.clone(),
            SemanticNode::StyledText {
                text: TextValue::from("森林入口"),
                styles: vec![TextStyle::Strong],
                color: TextColor::DEFAULT,
                delay: None,
                heading: None,
            },
        )
        .unwrap();

    assert_eq!(output.key(0), Some(&title));
    assert_eq!(
        output
            .push_keyed(title, SemanticNode::Text(TextValue::from("重复")))
            .unwrap_err(),
        SemanticKeyError::Duplicate(String::from("passage:title"))
    );
    assert_eq!(SemanticKey::parse(""), Err(SemanticKeyError::Empty));
}

#[test]
fn keyed_container_keeps_empty_replace_target_visible_to_hosts() {
    let mut output = SemanticOutput::default();
    output
        .push_keyed(
            SemanticKey::parse("replace-me").unwrap(),
            SemanticNode::Container {
                presentation: crate::semantic::ContainerPresentation::Plain,
                flow: crate::semantic::ContainerFlow::Stack,
                content: SemanticOutput::default(),
            },
        )
        .unwrap();

    assert_eq!(output.key(0).unwrap().as_str(), "replace-me");
    assert!(matches!(
        output.nodes(),
        [SemanticNode::Container { content, .. }] if content.is_empty()
    ));
}

#[test]
fn semantic_text_and_regions_remain_host_neutral() {
    let content = SemanticOutput::from_nodes(vec![SemanticNode::StyledText {
        text: TextValue::from("体力不足"),
        styles: vec![TextStyle::Strong],
        color: TextColor::YELLOW,
        delay: None,
        heading: None,
    }]);
    let output = SemanticOutput::from_nodes(vec![SemanticNode::Region {
        region: RegionId::bar(),
        content,
    }]);

    let [SemanticNode::Region { region, content }] = output.nodes() else {
        panic!("应建立语义区域");
    };
    assert_eq!(region, &RegionId::bar());
    assert!(matches!(
        content.nodes(),
        [SemanticNode::StyledText {
            color: TextColor::YELLOW,
            ..
        }]
    ));
}

#[test]
fn region_ids_keep_standard_names_and_accept_custom_regions() {
    assert_eq!(RegionId::main().as_str(), "main");
    assert_eq!(RegionId::bar_stowed().as_str(), "bar-stowed");
    assert_eq!(RegionId::parse("hud").unwrap().as_str(), "hud");
    assert!(RegionId::parse("").is_err());
}

#[test]
fn versioned_components_keep_pure_properties_and_semantic_fallback() {
    let output = SemanticOutput::from_nodes(vec![SemanticNode::Component {
        capability: ComponentCapability::parse("meter").unwrap(),
        version: 1,
        properties: std::collections::BTreeMap::from([
            (String::from("value"), SemanticValue::Number(42.0)),
            (
                String::from("label"),
                SemanticValue::Text(String::from("体力")),
            ),
        ]),
        fallback: SemanticOutput::from_nodes(vec![SemanticNode::Text(TextValue::from("体力：42"))]),
    }]);

    assert!(matches!(
        output.nodes(),
        [SemanticNode::Component { capability, version: 1, fallback, .. }]
            if capability.as_str() == "meter" && fallback.len() == 1
    ));
}

#[test]
fn non_navigation_actions_have_explicit_host_semantics() {
    let output = SemanticOutput::from_nodes(vec![SemanticNode::Action {
        label: TextValue::from("关闭"),
        action: SemanticAction::Dismiss,
        role: ActionRole::Secondary,
    }]);

    assert!(!output.has_navigation());
    assert!(matches!(
        output.nodes(),
        [SemanticNode::Action {
            action: SemanticAction::Dismiss,
            role: ActionRole::Secondary,
            ..
        }]
    ));
}

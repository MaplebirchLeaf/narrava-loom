//! 宿主无关语义输出测试。

use crate::{
    expression::value::TextValue,
    presentation::{
        ActionRole, ComponentCapability, InteractionId, InteractionIdError, PresentationAction,
        PresentationKey, PresentationKeyError, PresentationNode, PresentationOutput,
        PresentationRegion, PresentationValue, TextStyle, TextTone,
    },
};

#[test]
fn output_preserves_semantic_node_order() {
    let web_text: TextValue = TextValue::from_units(vec![0xD800]);
    let output: PresentationOutput = PresentationOutput::from_nodes(vec![
        PresentationNode::Text(TextValue::from("进入森林")),
        PresentationNode::Text(web_text.clone()),
        PresentationNode::Navigation {
            role: crate::presentation::NavigationRole::Link,
            id: InteractionId::from_key("start:choice:0"),
            label: TextValue::from("继续"),
            target: String::from("Forest"),
        },
    ]);

    assert_eq!(output.nodes().len(), 3);
    assert_eq!(output.nodes()[1], PresentationNode::Text(web_text));
    assert!(output.has_navigation());
}

#[test]
fn output_appends_and_merges_without_a_host_renderer() {
    let mut output: PresentationOutput = PresentationOutput::default();
    output.push(PresentationNode::Text(TextValue::from("前")));
    output.append(PresentationOutput::from_nodes(vec![
        PresentationNode::Text(TextValue::from("后")),
    ]));

    assert_eq!(output.len(), 2);
    assert!(!output.is_empty());
    assert_eq!(
        output.nodes(),
        &[
            PresentationNode::Text(TextValue::from("前")),
            PresentationNode::Text(TextValue::from("后")),
        ]
    );
}

#[test]
fn safe_return_is_not_an_author_navigation_action() {
    let output: PresentationOutput =
        PresentationOutput::from_nodes(vec![PresentationNode::SafeReturn {
            id: InteractionId::from_key("history:2:safe-return"),
            target: String::from("Start"),
        }]);

    assert!(!output.has_navigation());
}

#[test]
fn interaction_identity_resolves_only_core_presented_actions() {
    let id: InteractionId = InteractionId::from_key("start:choice:0");
    let unknown: InteractionId = InteractionId::from_key("start:choice:1");
    let output: PresentationOutput =
        PresentationOutput::from_nodes(vec![PresentationNode::Navigation {
            role: crate::presentation::NavigationRole::Link,
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
fn presentation_keys_are_explicit_stable_and_unique_within_one_output() {
    let mut output = PresentationOutput::default();
    let title = PresentationKey::parse("passage:title").unwrap();
    output
        .push_keyed(
            title.clone(),
            PresentationNode::StyledText {
                text: TextValue::from("森林入口"),
                styles: vec![TextStyle::Strong],
                tone: TextTone::DEFAULT,
                delay: None,
                heading: None,
            },
        )
        .unwrap();

    assert_eq!(output.key(0), Some(&title));
    assert_eq!(
        output
            .push_keyed(title, PresentationNode::Text(TextValue::from("重复")))
            .unwrap_err(),
        PresentationKeyError::Duplicate(String::from("passage:title"))
    );
    assert_eq!(PresentationKey::parse(""), Err(PresentationKeyError::Empty));
}

#[test]
fn keyed_container_keeps_empty_replace_target_visible_to_hosts() {
    let mut output = PresentationOutput::default();
    output
        .push_keyed(
            PresentationKey::parse("replace-me").unwrap(),
            PresentationNode::Container {
                content: PresentationOutput::default(),
            },
        )
        .unwrap();

    assert_eq!(output.key(0).unwrap().as_str(), "replace-me");
    assert!(matches!(
        output.nodes(),
        [PresentationNode::Container { content }] if content.is_empty()
    ));
}

#[test]
fn semantic_text_and_regions_remain_host_neutral() {
    let content = PresentationOutput::from_nodes(vec![PresentationNode::StyledText {
        text: TextValue::from("体力不足"),
        styles: vec![TextStyle::Strong],
        tone: TextTone::YELLOW,
        delay: None,
        heading: None,
    }]);
    let output = PresentationOutput::from_nodes(vec![PresentationNode::Region {
        region: PresentationRegion::Bar,
        content,
    }]);

    let [PresentationNode::Region { region, content }] = output.nodes() else {
        panic!("应建立语义区域");
    };
    assert_eq!(*region, PresentationRegion::Bar);
    assert!(matches!(
        content.nodes(),
        [PresentationNode::StyledText {
            tone: TextTone::YELLOW,
            ..
        }]
    ));
}

#[test]
fn versioned_components_keep_pure_properties_and_semantic_fallback() {
    let output = PresentationOutput::from_nodes(vec![PresentationNode::Component {
        capability: ComponentCapability::parse("meter").unwrap(),
        version: 1,
        properties: std::collections::BTreeMap::from([
            (String::from("value"), PresentationValue::Number(42.0)),
            (
                String::from("label"),
                PresentationValue::Text(String::from("体力")),
            ),
        ]),
        fallback: PresentationOutput::from_nodes(vec![PresentationNode::Text(TextValue::from(
            "体力：42",
        ))]),
    }]);

    assert!(matches!(
        output.nodes(),
        [PresentationNode::Component { capability, version: 1, fallback, .. }]
            if capability.as_str() == "meter" && fallback.len() == 1
    ));
}

#[test]
fn non_navigation_actions_have_explicit_host_semantics() {
    let output = PresentationOutput::from_nodes(vec![PresentationNode::Action {
        label: TextValue::from("关闭"),
        action: PresentationAction::Dismiss,
        role: ActionRole::Secondary,
    }]);

    assert!(!output.has_navigation());
    assert!(matches!(
        output.nodes(),
        [PresentationNode::Action {
            action: PresentationAction::Dismiss,
            role: ActionRole::Secondary,
            ..
        }]
    ));
}

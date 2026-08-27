use super::*;

fn delayed<'hir>(target: &str, body: &'hir [HirBodyNode<'hir>]) -> MacroInteraction<'hir, 'hir> {
    MacroInteraction::new(target, body, CapturedMacroLocals::empty())
}

fn link_arguments(label: &str, target: &str) -> Vec<Value> {
    vec![Value::object(vec![
        (String::from("label"), Value::string(label)),
        (String::from("target"), Value::string(target)),
    ])]
}

#[test]
fn macro_interactions_add_get_update_and_take_without_implicit_overwrite() {
    let first_body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Text("first"))];
    let second_body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Text("second"))];
    let id: InteractionId = InteractionId::from_key("link:1");
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();

    interactions
        .add(id.clone(), delayed("Forest", &first_body))
        .expect("首次 Interaction 应能新增");
    assert!(interactions.has(&id));
    assert_eq!(
        interactions.get(&id).map(MacroInteraction::target),
        Some("Forest")
    );
    assert_eq!(
        interactions.add(id.clone(), delayed("Town", &second_body)),
        Err(MacroInteractionError::Duplicate)
    );
    assert_eq!(
        interactions.get(&id).map(MacroInteraction::target),
        Some("Forest")
    );

    let previous: MacroInteraction<'_, '_> = interactions
        .update(&id, delayed("Town", &second_body))
        .expect("显式 update 应替换已有 Interaction");
    assert_eq!(previous.target(), "Forest");
    assert_eq!(
        interactions.get(&id).map(MacroInteraction::target),
        Some("Town")
    );

    let taken: MacroInteraction<'_, '_> = interactions.take(&id).expect("激活应取走所有权");
    assert_eq!(taken.target(), "Town");
    assert!(!interactions.has(&id));
    assert_eq!(interactions.take(&id), None);
}

#[test]
fn macro_interactions_del_and_missing_update_are_explicit() {
    let body: Vec<HirBodyNode<'_>> = Vec::new();
    let id: InteractionId = InteractionId::from_key("link:2");
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();

    assert_eq!(
        interactions.update(&id, delayed("Map", &body)),
        Err(MacroInteractionError::Missing)
    );
    assert_eq!(interactions.del(&id), None);
    interactions
        .add(id.clone(), delayed("Map", &body))
        .expect("Interaction 应能新增");
    assert_eq!(
        interactions
            .del(&id)
            .map(|action: MacroInteraction<'_, '_>| action.target().to_owned()),
        Some(String::from("Map"))
    );
    assert!(interactions.is_empty());
}

#[test]
fn link_with_body_registers_body_target_and_selected_captures() {
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Text("after click"))];
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(7, 11);
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter();
    locals
        .set("selected", Value::Number(3.0))
        .expect("局部变量应可写入");
    let captures: CapturedMacroLocals<Value> = locals.capture(&["selected"]);
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();

    let execution: BodyExecution = link_with_body(
        &link_arguments("进入森林", "Forest"),
        identity,
        &body,
        captures,
        &mut interactions,
    )
    .expect("容器 link 应完成登记");
    let [SemanticNode::Navigation { id, .. }] = execution.output.nodes() else {
        panic!("link 应只产生一个 Navigation");
    };

    let action: MacroInteraction<'_, '_> = interactions.take(id).expect("导航应有关联动作");
    let (target, registered_body, captures): (_, _, CapturedMacroLocals<Value>) =
        action.into_parts();
    let scopes: MacroLocalScopes<Value> = captures.into_scopes();
    assert_eq!(target, "Forest");
    assert_eq!(registered_body, body.as_slice());
    assert_eq!(scopes.get("selected"), Some(&Value::Number(3.0)));
}

#[test]
fn link_with_body_rejects_duplicate_id_without_replacing_original_action() {
    let first_body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Text("first"))];
    let second_body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Text("second"))];
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(3, 5);
    let arguments: Vec<Value> = link_arguments("前往", "Map");
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();

    link_with_body(
        &arguments,
        identity,
        &first_body,
        CapturedMacroLocals::empty(),
        &mut interactions,
    )
    .expect("首次登记应成功");
    let error: Diagnostic = link_with_body(
        &arguments,
        identity,
        &second_body,
        CapturedMacroLocals::empty(),
        &mut interactions,
    )
    .expect_err("重复 ID 不应覆盖已有动作");

    assert_eq!(error.code, "macro.link.interaction_registration_failed");
    let remaining: &MacroInteraction<'_, '_> = interactions
        .get(&InteractionId::from_key("link:3:5:524d5f80:Map"))
        .expect("原动作应保留");
    assert_eq!(remaining.body(), first_body.as_slice());
}

#[test]
fn button_uses_button_role() {
    let body: Vec<HirBodyNode<'_>> = vec![logic_node(HirBodyKind::Text("clicked"))];
    let mut interactions: MacroInteractions<'_, '_> = MacroInteractions::new();
    let button: BodyExecution = button_with_body(
        &link_arguments("确认", "Hall"),
        RuntimeExecutionIdentity::new(4, 2),
        &body,
        CapturedMacroLocals::empty(),
        &mut interactions,
    )
    .expect("button 应登记延迟正文");
    assert!(matches!(
        button.output.nodes()[0],
        SemanticNode::Navigation {
            role: NavigationRole::Button,
            ..
        }
    ));
}

#[test]
fn replace_uses_host_neutral_region_or_key_targets() {
    let content = SemanticOutput::from_nodes(vec![SemanticNode::Text(TextValue::from("替换内容"))]);
    let header: BodyExecution = replace("header", content.clone()).expect("固定区域应有效");
    assert!(matches!(
        &header.output.nodes()[0],
        SemanticNode::Replace { target: SemanticTarget::Region(region), .. }
            if region == &RegionId::header()
    ));

    let keyed: BodyExecution = replace("status-panel", content).expect("稳定 key 应有效");
    assert!(matches!(
        keyed.output.nodes()[0],
        SemanticNode::Replace {
            target: SemanticTarget::Key(ref key),
            ..
        } if key.as_str() == "status-panel"
    ));
}

#[test]
fn slot_creates_a_keyed_container_for_later_replace() {
    let content = SemanticOutput::from_nodes(vec![SemanticNode::Text(TextValue::from("初始内容"))]);
    let execution: BodyExecution =
        crate::macro_runtime::slot("status-panel", content).expect("有效 key 应建立稳定替换槽");

    assert_eq!(
        execution.output.key(0).map(|key| key.as_str()),
        Some("status-panel")
    );
    assert!(matches!(
        execution.output.nodes(),
        [SemanticNode::Container { content }] if content.len() == 1
    ));
}

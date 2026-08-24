use super::*;

#[test]
fn provides_basic_macro_definition_operations() {
    let mut definitions: MacroDefinitions<&str> = MacroDefinitions::new();

    assert!(!definitions.has("widget"));
    assert_eq!(definitions.add("widget", "first"), None);
    assert!(definitions.has("widget"));
    assert_eq!(definitions.get("widget"), Some(&"first"));
    assert_eq!(definitions.add("widget", "second"), Some("first"));
    assert_eq!(definitions.get("widget"), Some(&"second"));
    assert_eq!(definitions.del("widget"), Some("second"));
    assert!(!definitions.has("widget"));
}

#[test]
fn registers_widget_in_the_shared_macro_definition_container() {
    let widget: HirWidget<'_> = HirWidget {
        name: "greet",
        body: vec![logic_set("@called = true")],
    };
    let native: MacroDefinition<RuntimeMacroHandler<'_, '_, &str>> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::Raw,
        MacroExecutionKind::Sync,
        RuntimeMacroHandler::Native("native-handler"),
    );
    let mut definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'_, '_, &str>>> =
        MacroDefinitions::new();
    let _empty: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &str>>> =
        definitions.add("greet", native);

    let previous: Option<MacroDefinition<RuntimeMacroHandler<'_, '_, &str>>> =
        register_widget(&mut definitions, &widget);

    assert!(matches!(
        previous.map(|definition| definition.handler),
        Some(RuntimeMacroHandler::Native("native-handler"))
    ));
    let definition: &MacroDefinition<RuntimeMacroHandler<'_, '_, &str>> =
        definitions.get("greet").expect("Widget 应完成注册");
    assert_eq!(definition.body_kind, MacroBodyKind::Inline);
    assert_eq!(definition.argument_kind, MacroArgumentKind::ArgumentList);
    assert_eq!(definition.execution_kind, MacroExecutionKind::Sync);
    let RuntimeMacroHandler::Widget(body) = definition.handler else {
        panic!("当前定义应保存 Widget 正文")
    };
    assert_eq!(body, widget.body.as_slice());
}

#[test]
fn updates_only_an_existing_macro_definition() {
    let mut definitions: MacroDefinitions<&str> = MacroDefinitions::new();

    assert_eq!(
        definitions.update("missing", "new"),
        Err(MacroDefinitionError::MissingDefinition)
    );
    assert!(!definitions.has("missing"));

    let _previous: Option<&str> = definitions.add("widget", "first");
    assert_eq!(definitions.update("widget", "second"), Ok("first"));
    assert_eq!(definitions.get("widget"), Some(&"second"));
}

#[test]
fn keeps_macro_names_case_sensitive() {
    let mut definitions: MacroDefinitions<u8> = MacroDefinitions::new();

    let _previous: Option<u8> = definitions.add("link", 1);
    let _previous: Option<u8> = definitions.add("Link", 2);

    assert_eq!(definitions.get("link"), Some(&1));
    assert_eq!(definitions.get("Link"), Some(&2));
}

use super::*;

fn generated_names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn discriminator<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value).unwrap()["type"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn runtime_messages_round_trip_without_runtime_objects() {
    let command = RuntimeCommand::Input {
        interaction: String::from("input:route"),
        value: serde_json::json!({ "choice": "quiet" }),
    };
    let encoded = serde_json::to_string(&command).unwrap();
    assert_eq!(
        serde_json::from_str::<RuntimeCommand>(&encoded).unwrap(),
        command
    );

    let update = RuntimeUpdate::Ready {
        update: HostUpdateDto {
            current: String::from("Start"),
            can_back: false,
            can_forward: false,
            nodes: vec![HostNodeDto::Text {
                key: String::from("start:0:text"),
                text: String::from("Hello"),
            }],
        },
    };
    let encoded = serde_json::to_string(&update).unwrap();
    let json: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(json["update"]["can_back"], false);
    assert_eq!(json["update"]["can_forward"], false);
    assert!(json["update"].get("canBack").is_none());
    assert_eq!(
        serde_json::from_str::<RuntimeUpdate>(&encoded).unwrap(),
        update
    );
}

#[test]
fn pending_operation_preserves_opaque_identity() {
    let operation = PendingOperation::Delay {
        operation: 17,
        milliseconds: 250,
    };
    assert_eq!(operation.id(), 17);
}

#[test]
fn session_request_and_response_keep_the_same_cross_language_identity() {
    let session = RuntimeSessionId::new("game_7").unwrap();
    let request = RuntimeRequest::new(session.clone(), RuntimeCommand::Start);
    let encoded = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::from_str::<RuntimeRequest>(&encoded).unwrap(),
        request
    );

    let response = RuntimeResponse {
        protocol_version: RUNTIME_PROTOCOL_VERSION,
        session,
        update: RuntimeUpdate::Applied,
    };
    let encoded = serde_json::to_string(&response).unwrap();
    assert_eq!(
        serde_json::from_str::<RuntimeResponse>(&encoded).unwrap(),
        response
    );
}

#[test]
fn runtime_envelopes_expose_the_canonical_protocol_version() {
    let request = RuntimeRequest::new(
        RuntimeSessionId::new("main").unwrap(),
        RuntimeCommand::Start,
    );
    let json = serde_json::to_value(request).unwrap();
    assert_eq!(json["protocolVersion"], RUNTIME_PROTOCOL_VERSION);
}

#[test]
fn session_identity_rejects_values_that_are_unsafe_for_external_registries() {
    assert!(RuntimeSessionId::new("").is_err());
    assert!(RuntimeSessionId::new("../game").is_err());
    assert!(RuntimeSessionId::new("game:1").is_err());
}

#[test]
fn runtime_protocol_discriminators_match_the_canonical_contract() {
    let commands = [
        RuntimeCommand::Start,
        RuntimeCommand::Back,
        RuntimeCommand::Forward,
        RuntimeCommand::Activate {
            interaction: String::new(),
        },
        RuntimeCommand::Input {
            interaction: String::new(),
            value: serde_json::Value::Null,
        },
        RuntimeCommand::Save {
            operation: SaveOperation::Export,
            target: String::new(),
        },
        RuntimeCommand::SelectLanguage {
            locale: String::new(),
        },
        RuntimeCommand::Resume {
            operation: 1,
            result: None,
        },
        RuntimeCommand::Cancel { operation: 1 },
    ];
    assert_eq!(
        commands.iter().map(discriminator).collect::<Vec<_>>(),
        generated_names(contract::RUNTIME_COMMANDS)
    );

    let empty_update = HostUpdateDto {
        current: String::new(),
        can_back: false,
        can_forward: false,
        nodes: Vec::new(),
    };
    let updates = [
        RuntimeUpdate::Ready {
            update: empty_update,
        },
        RuntimeUpdate::Applied,
        RuntimeUpdate::Pending {
            operation: PendingOperation::Delay {
                operation: 1,
                milliseconds: 1,
            },
        },
    ];
    assert_eq!(
        updates.iter().map(discriminator).collect::<Vec<_>>(),
        generated_names(contract::RUNTIME_UPDATES)
    );
    assert_eq!(
        vec![
            discriminator(&PendingOperation::Delay {
                operation: 1,
                milliseconds: 1,
            }),
            discriminator(&PendingOperation::Save {
                operation: 2,
                direction: SaveOperation::Export,
                target: String::new(),
                document: Some(String::new()),
            }),
            discriminator(&PendingOperation::SelectLanguage {
                operation: 3,
                locale: String::new(),
            }),
        ],
        generated_names(contract::PENDING_OPERATIONS)
    );
}

#[test]
fn surface_node_discriminators_match_the_canonical_contract() {
    let text = String::new();
    let key = || String::new();
    let nodes = vec![
        HostNodeDto::Text {
            key: key(),
            text: text.clone(),
        },
        HostNodeDto::HardBreak { key: key() },
        HostNodeDto::StyledText {
            key: key(),
            text: text.clone(),
            styles: Vec::new(),
            color: 0,
            delay: None,
            heading: None,
        },
        HostNodeDto::Image {
            key: key(),
            resource: text.clone(),
            alt: text.clone(),
            caption: None,
        },
        HostNodeDto::Region {
            key: key(),
            region: text.clone(),
            nodes: Vec::new(),
        },
        HostNodeDto::Container {
            key: key(),
            nodes: Vec::new(),
        },
        HostNodeDto::Component {
            key: key(),
            capability: text.clone(),
            version: 1,
            properties: serde_json::Value::Null,
            fallback: Vec::new(),
        },
        HostNodeDto::Replace {
            key: key(),
            target: HostReplaceTargetDto::Region(text.clone()),
            nodes: Vec::new(),
        },
        HostNodeDto::Action {
            key: key(),
            label: text.clone(),
            action: text.clone(),
            role: text.clone(),
        },
        HostNodeDto::Checkbox {
            key: key(),
            id: text.clone(),
            unchecked: serde_json::Value::Null,
            checked: serde_json::Value::Bool(true),
            selected: false,
        },
        HostNodeDto::Radiobutton {
            key: key(),
            id: text.clone(),
            group: text.clone(),
            value: serde_json::Value::Null,
            selected: false,
        },
        HostNodeDto::Textbox {
            key: key(),
            id: text.clone(),
            value: text.clone(),
        },
        HostNodeDto::Navigation {
            key: key(),
            id: text.clone(),
            label: text.clone(),
            target: text.clone(),
        },
        HostNodeDto::Button {
            key: key(),
            id: text.clone(),
            label: text.clone(),
            target: text.clone(),
        },
        HostNodeDto::SafeReturn {
            key: key(),
            id: text.clone(),
            target: text,
        },
    ];
    assert_eq!(
        nodes.iter().map(discriminator).collect::<Vec<_>>(),
        generated_names(contract::SURFACE_NODES)
    );
}

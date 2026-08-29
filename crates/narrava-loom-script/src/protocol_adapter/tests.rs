//! `protocol_bridge` 的受验证转换测试。

use crate::protocol_adapter::SurfaceNode;
use narrava_loom_core::{
    expression::value::{TextValue, Value},
    semantic::{RegionId, TextColor, TextStyle},
};

use crate::protocol_adapter::protocol_bridge::output;

fn text(value: &str) -> Value {
    Value::String(TextValue::from(value))
}

/// 脚本 builder 值被解析为带 key 的语义节点树。
#[test]
fn builder_values_become_keyed_semantic_text_image_and_regions() {
    let value = Value::object(vec![
        ("__narravaSurface".into(), text("region")),
        ("region".into(), text("bar")),
        ("key".into(), text("status")),
        (
            "children".into(),
            Value::array(vec![Value::object(vec![
                ("__narravaSurface".into(), text("text")),
                ("text".into(), text("危险")),
                ("styles".into(), Value::array(vec![text("strong")])),
                ("color".into(), Value::Number(8.0)),
            ])]),
        ),
    ]);

    let output = output(&value).unwrap().unwrap();

    assert_eq!(output.key(0).unwrap().as_str(), "status");
    let [SurfaceNode::Region { region, content }] = output.nodes() else {
        panic!("应转换 Region");
    };
    assert_eq!(*region, RegionId::bar());
    assert!(matches!(
        content.nodes(),
        [SurfaceNode::StyledText { styles, color: TextColor::RED, .. }]
            if styles == &[TextStyle::Strong]
    ));
}

/// 未知样式等平台值被拒绝，错误码不绑定具体 Host。
#[test]
fn builder_values_reject_dom_names_and_unknown_semantics() {
    let value = Value::object(vec![
        ("__narravaSurface".into(), text("text")),
        ("text".into(), text("危险")),
        ("styles".into(), Value::array(vec![text("red")])),
    ]);

    assert_eq!(output(&value).unwrap_err().code, "protocol.surface.invalid");
}

#[test]
fn builder_accepts_hard_break_and_custom_region() {
    let value = Value::object(vec![
        ("__narravaSurface".into(), text("region")),
        ("region".into(), text("hud")),
        (
            "children".into(),
            Value::array(vec![Value::object(vec![(
                "__narravaSurface".into(),
                text("hard-break"),
            )])]),
        ),
    ]);

    let output = output(&value).unwrap().unwrap();
    assert!(matches!(
        output.nodes(),
        [SurfaceNode::Region { region, content }]
            if region.as_str() == "hud"
                && matches!(content.nodes(), [SurfaceNode::HardBreak])
    ));
}

#[test]
fn builder_rejects_br_markup_instead_of_rescanning_script_text() {
    let value = Value::object(vec![
        ("__narravaSurface".into(), text("text")),
        ("text".into(), text("上一行<br>下一行")),
    ]);

    let error = output(&value).unwrap_err();
    assert_eq!(error.code, "protocol.surface.invalid");
    assert!(error.message.contains("Surface.hardBreak()"));
}

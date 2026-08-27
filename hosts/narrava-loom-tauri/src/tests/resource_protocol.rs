//! `resource_protocol` 的受验证资源协议测试（原内联于 resource_protocol.rs，按源码规范收拢）。

use narrava_loom_core::resource::{ResourceCatalog, ResourceInput};
use tauri::http::{StatusCode, header};

use crate::resource_protocol::respond;

/// 协议只按请求的已验证路径读取对应 Resource。
#[test]
fn protocol_reads_only_the_requested_validated_resource() {
    let resources = ResourceCatalog::new([
        ResourceInput::new("images/forest one.png", vec![1, 2, 3]),
        ResourceInput::new("images/unused.png", vec![9; 1024]),
    ])
    .unwrap();

    let response = respond(&resources, "/images/forest%20one.png");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(response.body(), &[1, 2, 3]);
}

/// 非法编码与未知路径分别映射为 400/404。
#[test]
fn protocol_rejects_invalid_encoding_and_unknown_paths() {
    let resources = ResourceCatalog::default();
    assert_eq!(
        respond(&resources, "/bad%2").status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        respond(&resources, "/missing.png").status(),
        StatusCode::NOT_FOUND
    );
}

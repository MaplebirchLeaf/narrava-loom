//! WebView 按逻辑路径读取单个 Resource 的 Tauri 协议适配器。

use narrava_loom_core::resource::ResourceCatalog;
use tauri::http::{Response, StatusCode, header};

pub(crate) fn respond(resources: &ResourceCatalog, encoded_path: &str) -> Response<Vec<u8>> {
    let Some(path) = decode_path(encoded_path.trim_start_matches('/')) else {
        return failure(StatusCode::BAD_REQUEST, "Resource URL 编码无效");
    };
    let Some(info) = resources.info(&path) else {
        return failure(StatusCode::NOT_FOUND, "Resource 不存在");
    };
    let bytes = match resources.read(&path) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return failure(StatusCode::NOT_FOUND, "Resource 不存在"),
        Err(_) => return failure(StatusCode::INTERNAL_SERVER_ERROR, "Resource 读取失败"),
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, info.media_type())
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(bytes.to_vec())
        .expect("固定 Resource response 必须有效")
}

fn failure(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(message.as_bytes().to_vec())
        .expect("固定错误 response 必须有效")
}

fn decode_path(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex(*bytes.get(index + 1)?)?;
            let low = hex(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use narrava_loom_core::resource::{ResourceCatalog, ResourceInput};
    use tauri::http::{StatusCode, header};

    use super::respond;

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
}

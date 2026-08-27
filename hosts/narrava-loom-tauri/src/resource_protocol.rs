//! WebView 按逻辑路径读取单个 Resource 的 Tauri 协议适配器。

use narrava_loom_core::resource::ResourceCatalog;
use tauri::http::{Response, StatusCode, header};

/// 按 WebView 请求的逻辑路径返回单个 Resource；错误映射为对应 HTTP 状态码。
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

/// 构造纯文本错误响应。
fn failure(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(message.as_bytes().to_vec())
        .expect("固定错误 response 必须有效")
}

/// 解码 URL 百分号编码；编码非法或结果非 UTF-8 时返回 `None`。
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

/// 单个十六进制字符转数值。
fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

//! I18n 文本模板唯一的花括号语法解析入口。

/// 模板解析出的一个片段：普通文本、受控占位符或转义花括号。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum I18nTemplatePart<'text> {
    Text(&'text str),
    Placeholder(&'text str),
    LiteralBrace(char),
}

/// 解析花括号模板；`{{`/`}}` 转义为字面花括号，`{name}` 为占位符，未闭合或嵌套花括号返回 `None`。
pub(super) fn parse_template(text: &str) -> Option<Vec<I18nTemplatePart<'_>>> {
    let bytes: &[u8] = text.as_bytes();
    let mut parts: Vec<I18nTemplatePart<'_>> = Vec::new();
    let mut index: usize = 0;
    let mut text_start: usize = 0;

    while index < bytes.len() {
        let escaped: Option<(char, usize)> = match bytes[index] {
            b'{' if bytes.get(index + 1) == Some(&b'{') => Some(('{', 2)),
            b'}' if bytes.get(index + 1) == Some(&b'}') => Some(('}', 2)),
            b'{' => {
                push_text(&mut parts, text, text_start, index);
                let start: usize = index + 1;
                let relative_end: usize = bytes[start..].iter().position(|byte| *byte == b'}')?;
                let end: usize = start + relative_end;
                if start == end || bytes[start..end].contains(&b'{') {
                    return None;
                }
                parts.push(I18nTemplatePart::Placeholder(&text[start..end]));
                index = end + 1;
                text_start = index;
                continue;
            }
            b'}' => return None,
            _ => None,
        };
        if let Some((character, width)) = escaped {
            push_text(&mut parts, text, text_start, index);
            parts.push(I18nTemplatePart::LiteralBrace(character));
            index += width;
            text_start = index;
        } else {
            index += 1;
        }
    }
    push_text(&mut parts, text, text_start, text.len());
    Some(parts)
}

/// 把一段非空文本追加到片段列表。
fn push_text<'text>(
    parts: &mut Vec<I18nTemplatePart<'text>>,
    text: &'text str,
    start: usize,
    end: usize,
) {
    if start != end {
        parts.push(I18nTemplatePart::Text(&text[start..end]));
    }
}

//! Text Token 内的字面文本、Inline Macro 与同行容器解析。
//!
//! 这一层只在单个 Token 的 Span 内扫描配对符号；跨行容器、if/switch 分支和
//! Passage 结构仍由父解析器负责，从而把局部字符扫描与高层 Token 编排分开。

use super::*;

/// 把单个 Text Token 分成字面文本、Inline Macro 与允许同行书写的固有容器。
pub(super) fn parse_body_nodes<'source, 'source_path>(
    text: &'source str,
    source: &'source_path SourcePath,
    span: Span,
) -> Result<Vec<BodyNode<'source>>, ParseError<'source_path>> {
    let line: &str = strip_line_ending(text);
    let opening: Option<&str> = line.strip_prefix("<<");
    let macro_kind: Option<BodyNodeKind<'source>> = opening.and_then(|opening: &str| {
        let header_end: usize = opening.find(">>")?;
        let header: &str = opening[..header_end].trim();
        let remainder: &str = &opening[header_end + 2..];
        let separator: Option<usize> = header.find(char::is_whitespace);
        let (name, arguments): (&str, &str) = match separator {
            Some(index) => (&header[..index], header[index..].trim()),
            None => (header, ""),
        };
        let closing: Option<&str> = remainder
            .strip_prefix("<</")
            .and_then(|value: &str| value.strip_suffix(">>"));

        if name.is_empty() || closing != Some(name) {
            return None;
        }

        Some(BodyNodeKind::Macro(MacroNode {
            name,
            arguments,
            arguments_span: macro_arguments_span(text, span, arguments),
            syntax_kind: crate::twee::MacroSyntaxKind::Container,
            body: Vec::new(),
        }))
    });
    if let Some(kind) = macro_kind {
        return Ok(vec![BodyNode { kind, span }]);
    }

    let mut nodes: Vec<BodyNode<'source>> = Vec::new();
    let mut cursor: usize = 0;
    while let Some(relative_start) = text[cursor..].find("<<") {
        let start: usize = cursor + relative_start;
        if start > cursor {
            nodes.push(BodyNode {
                kind: BodyNodeKind::Text(&text[cursor..start]),
                span: relative_span(text, span, cursor, start),
            });
        }
        let end: usize = match find_inline_macro_end(text, start + 2) {
            Ok(Some(end)) => end,
            Ok(None) => {
                nodes.push(BodyNode {
                    kind: BodyNodeKind::Text(&text[start..]),
                    span: relative_span(text, span, start, text.len()),
                });
                cursor = text.len();
                break;
            }
            Err(offset) => {
                return Err(ParseError {
                    source,
                    kind: ParseErrorKind::UngroupedMacroShift,
                    span: relative_span(text, span, offset, offset + 2),
                });
            }
        };
        let header: &str = text[start + 2..end].trim();
        if header.is_empty() || header.starts_with('/') {
            nodes.push(BodyNode {
                kind: BodyNodeKind::Text(&text[start..end + 2]),
                span: relative_span(text, span, start, end + 2),
            });
            cursor = end + 2;
            continue;
        }
        let separator: Option<usize> = header.find(char::is_whitespace);
        let (name, arguments): (&str, &str) = match separator {
            Some(index) => (&header[..index], header[index..].trim()),
            None => (header, ""),
        };
        if let Some(container_name) = inline_container_name(name)
            && (container_name != "silently" || arguments.is_empty())
        {
            let body_start: usize = end + 2;
            let closing_start: usize =
                match find_inline_container_closing(text, body_start, container_name) {
                    Ok(Some(closing)) => closing,
                    Ok(None) => {
                        return Err(ParseError {
                            source,
                            kind: ParseErrorKind::UnclosedMacro {
                                name: container_name,
                            },
                            span: relative_span(text, span, start, body_start),
                        });
                    }
                    Err(offset) => {
                        return Err(ParseError {
                            source,
                            kind: ParseErrorKind::UngroupedMacroShift,
                            span: relative_span(text, span, offset, offset + 2),
                        });
                    }
                };
            let closing_end: usize = closing_start + name.len() + "<</>>".len();
            let body: Vec<BodyNode<'source>> = parse_body_nodes(
                &text[body_start..closing_start],
                source,
                relative_span(text, span, body_start, closing_start),
            )?;
            nodes.push(BodyNode {
                kind: BodyNodeKind::Macro(MacroNode {
                    name,
                    arguments,
                    arguments_span: macro_arguments_span(text, span, arguments),
                    syntax_kind: crate::twee::MacroSyntaxKind::Container,
                    body,
                }),
                span: relative_span(text, span, start, closing_end),
            });
            cursor = closing_end;
            continue;
        }
        nodes.push(BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name,
                arguments,
                arguments_span: macro_arguments_span(text, span, arguments),
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: Vec::new(),
            }),
            span: relative_span(text, span, start, end + 2),
        });
        cursor = end + 2;
    }
    if cursor < text.len() {
        nodes.push(BodyNode {
            kind: BodyNodeKind::Text(&text[cursor..]),
            span: relative_span(text, span, cursor, text.len()),
        });
    }
    if nodes.is_empty() {
        nodes.push(BodyNode {
            kind: BodyNodeKind::Text(text),
            span,
        });
    }
    Ok(nodes)
}

/// 把允许同行书写的固有容器 Macro 名称归一化为规范名；其他名称返回 None。
fn inline_container_name(name: &str) -> Option<&'static str> {
    match name {
        "silently" => Some("silently"),
        "capture" => Some("capture"),
        "link" => Some("link"),
        "button" => Some("button"),
        "replace" => Some("replace"),
        "slot" => Some("slot"),
        _ => None,
    }
}

/// 按 Macro 边界寻找同行同名闭合位置，忽略引号内的伪标记。
fn find_inline_container_closing(
    source: &str,
    start: usize,
    name: &str,
) -> Result<Option<usize>, usize> {
    let mut depth: usize = 1;
    let mut cursor: usize = start;

    while let Some(relative_start) = source[cursor..].find("<<") {
        let macro_start: usize = cursor + relative_start;
        let Some(macro_end) = find_inline_macro_end(source, macro_start + 2)? else {
            return Ok(None);
        };
        let header = source[macro_start + 2..macro_end].trim();
        let opening_name = header.split_whitespace().next().unwrap_or("");
        if opening_name == name {
            depth += 1;
        } else if header.strip_prefix('/') == Some(name) {
            depth -= 1;
            if depth == 0 {
                return Ok(Some(macro_start));
            }
        }
        cursor = macro_end + 2;
    }

    Ok(None)
}

/// 找到分组深度为零的 Macro `>>`；顶层 `<<` 必须先用圆括号隔开。
fn find_inline_macro_end(source: &str, start: usize) -> Result<Option<usize>, usize> {
    let bytes: &[u8] = source.as_bytes();
    let mut depth: usize = 0;
    let mut quote: Option<u8> = None;
    let mut escaped: bool = false;
    let mut index: usize = start;
    while index + 1 < bytes.len() {
        let byte: u8 = bytes[index];
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' | b'`' => quote = Some(byte),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'<' if depth == 0 && bytes[index + 1] == b'<' => return Err(index),
            b'>' if depth == 0 && bytes[index + 1] == b'>' => return Ok(Some(index)),
            _ => {}
        }
        index += 1;
    }
    Ok(None)
}

/// 参数位置独立于完整 Macro Span，供 HIR 精确映射 Expression 错误。
pub(super) fn macro_arguments_span(text: &str, parent: Span, arguments: &str) -> Span {
    let start: usize = if arguments.is_empty() {
        text.find(">>").unwrap_or(text.len())
    } else {
        text.find(arguments).unwrap_or(0)
    };
    relative_span(text, parent, start, start + arguments.len())
}

/// 将单行 Token 内的相对字节范围转换为 Twee Source 范围。
pub(super) fn relative_span(text: &str, parent: Span, start: usize, end: usize) -> Span {
    let preceding_characters: usize = text[..start].chars().count();
    Span {
        start: parent.start + start,
        end: parent.start + end,
        line: parent.line,
        column: parent.column + preceding_characters,
    }
}

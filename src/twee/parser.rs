//! Twee 解析：把 Token 组合成 Passage 与正文节点。

use super::*;

pub fn parse<'source>(
    tokens: &[Token<'source>],
) -> Result<Vec<Passage<'source>>, ParseError<'source>> {
    let visible: Vec<Token<'source>> = remove_comments(tokens)?;
    parse_visible(&visible)
}

/// 把一段动态 Twee 正文片段（不含 Passage 声明）解析为 BodyNode 列表。
///
/// 片段没有文件来源，`source` 由调用方持有（可用 [`SourcePath::fragment`]）；
/// 当前支持单行片段（不含换行分隔的嵌套宏）。
pub fn parse_fragment<'source, 'source_path>(
    text: &'source str,
    source: &'source_path crate::source::SourcePath,
) -> Result<Vec<BodyNode<'source>>, ParseError<'source_path>> {
    parse_body_nodes(
        text,
        source,
        Span {
            start: 0,
            end: text.len(),
            line: 1,
            column: 1,
        },
    )
}

/// 注释不进入 AST；正文切片仍引用原 Source，因此 Span 可以保持精确。
fn remove_comments<'source>(
    tokens: &[Token<'source>],
) -> Result<Vec<Token<'source>>, ParseError<'source>> {
    let mut visible: Vec<Token<'source>> = Vec::new();
    let mut opening: Option<(&'source SourcePath, Span)> = None;

    for token in tokens {
        match &token.kind {
            TokenKind::PassageDeclaration { name, tags } if opening.is_none() => {
                visible.push(Token {
                    source: token.source,
                    content: token.content,
                    kind: TokenKind::PassageDeclaration {
                        name,
                        tags: tags.clone(),
                    },
                    span: token.span,
                });
            }
            TokenKind::PassageDeclaration { .. } => {}
            TokenKind::Text(text) => {
                let mut cursor: usize = 0;
                while cursor < text.len() {
                    if opening.is_some() {
                        let Some(relative_end) = text[cursor..].find("%/") else {
                            break;
                        };
                        cursor += relative_end + "%/".len();
                        opening = None;
                        if text[cursor..].trim().is_empty() {
                            break;
                        }
                        continue;
                    }

                    let Some(relative_start) = text[cursor..].find("/%") else {
                        push_text_fragment(&mut visible, token, text, cursor, text.len());
                        break;
                    };
                    let comment_start: usize = cursor + relative_start;
                    push_text_fragment(&mut visible, token, text, cursor, comment_start);
                    opening = Some((
                        token.source,
                        relative_span(text, token.span, comment_start, comment_start + 2),
                    ));
                    cursor = comment_start + "/%".len();
                }
            }
        }
    }

    if let Some((source, span)) = opening {
        return Err(ParseError {
            source,
            kind: ParseErrorKind::UnclosedComment,
            span,
        });
    }

    Ok(visible)
}

fn push_text_fragment<'source>(
    visible: &mut Vec<Token<'source>>,
    token: &Token<'source>,
    text: &'source str,
    start: usize,
    end: usize,
) {
    if start == end {
        return;
    }
    let span: Span = if start == 0 && end == text.len() {
        token.span
    } else {
        relative_span(text, token.span, start, end)
    };
    visible.push(Token {
        source: token.source,
        content: token.content,
        kind: TokenKind::Text(&text[start..end]),
        span,
    });
}

fn parse_visible<'source>(
    tokens: &[Token<'source>],
) -> Result<Vec<Passage<'source>>, ParseError<'source>> {
    let mut passages: Vec<Passage<'source>> = Vec::new();
    let mut current: Option<Passage<'source>> = None;
    let mut index: usize = 0;

    while index < tokens.len() {
        let token: &Token<'source> = &tokens[index];
        match &token.kind {
            TokenKind::PassageDeclaration { name, tags } => {
                if name.is_empty() {
                    return Err(ParseError {
                        source: token.source,
                        kind: ParseErrorKind::EmptyPassageName,
                        span: token.span,
                    });
                }

                let completed: Option<Passage<'source>> = current.take();
                if let Some(passage) = completed {
                    passages.push(passage);
                }

                current = Some(Passage {
                    source: token.source,
                    content: token.content,
                    name,
                    tags: tags.clone(),
                    body: Vec::new(),
                    span: token.span,
                });
            }
            TokenKind::Text(text) => {
                let passage: Option<&mut Passage<'source>> = current.as_mut();
                let Some(passage) = passage else {
                    return Err(ParseError {
                        source: token.source,
                        kind: ParseErrorKind::TextBeforeDeclaration,
                        span: token.span,
                    });
                };

                if let Some(name) = parse_macro_closing_line(text) {
                    return Err(ParseError {
                        source: token.source,
                        kind: ParseErrorKind::UnexpectedMacroClosing { name },
                        span: token.span,
                    });
                }

                let multiline: Option<(BodyNode<'source>, usize)> =
                    parse_multiline_macro(tokens, index)?;
                if let Some((node, next_index)) = multiline {
                    passage.span.end = node.span.end;
                    passage.body.push(node);
                    index = next_index;
                    continue;
                }

                let nodes: Vec<BodyNode<'source>> =
                    parse_body_nodes(text, token.source, token.span)?;
                passage.body.extend(nodes);
                passage.span.end = token.span.end;
            }
        }

        index += 1;
    }

    let completed: Option<Passage<'source>> = current;
    if let Some(passage) = completed {
        passages.push(passage);
    }

    Ok(passages)
}

/// 组合完整闭合的跨行 Macro，正文暂时只保留 Text。
fn parse_multiline_macro<'source>(
    tokens: &[Token<'source>],
    opening_index: usize,
) -> Result<Option<(BodyNode<'source>, usize)>, ParseError<'source>> {
    let opening: &Token<'source> = &tokens[opening_index];
    let TokenKind::Text(opening_text) = opening.kind else {
        return Ok(None);
    };
    let Some((name, arguments)) = parse_macro_header_line(opening_text) else {
        return Ok(None);
    };
    if matches!(
        name,
        "break"
            | "continue"
            | "return"
            | "exit"
            | "set"
            | "unset"
            | "run"
            | "include"
            | "goto"
            | "print"
    ) {
        return Ok(Some((
            BodyNode {
                kind: BodyNodeKind::Macro(MacroNode {
                    name,
                    arguments,
                    arguments_span: macro_arguments_span(opening_text, opening.span, arguments),
                    syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                    body: Vec::new(),
                }),
                span: opening.span,
            },
            opening_index + 1,
        )));
    }
    if name == "if" {
        return parse_if_macro(tokens, opening_index, opening_text, arguments);
    }
    if name == "switch" {
        return parse_switch_macro(tokens, opening_index, opening_text, arguments);
    }
    if !requires_container_syntax(name) && !has_matching_closing(tokens, opening_index + 1, name) {
        return Ok(Some((
            BodyNode {
                kind: BodyNodeKind::Macro(MacroNode {
                    name,
                    arguments,
                    arguments_span: macro_arguments_span(opening_text, opening.span, arguments),
                    syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                    body: Vec::new(),
                }),
                span: opening.span,
            },
            opening_index + 1,
        )));
    }

    let mut index: usize = opening_index + 1;
    let mut body: Vec<BodyNode<'source>> = Vec::new();

    while index < tokens.len() {
        let token: &Token<'source> = &tokens[index];
        let TokenKind::Text(text) = token.kind else {
            return Err(ParseError {
                source: opening.source,
                kind: ParseErrorKind::UnclosedMacro { name },
                span: opening.span,
            });
        };

        if let Some(closing_name) = parse_macro_closing_line(text) {
            if closing_name != name {
                return Err(ParseError {
                    source: token.source,
                    kind: ParseErrorKind::MismatchedMacroClosing {
                        expected: name,
                        found: closing_name,
                    },
                    span: token.span,
                });
            }

            let node: BodyNode<'source> = BodyNode {
                kind: BodyNodeKind::Macro(MacroNode {
                    name,
                    arguments,
                    arguments_span: macro_arguments_span(opening_text, opening.span, arguments),
                    syntax_kind: crate::twee::MacroSyntaxKind::Container,
                    body,
                }),
                span: Span {
                    start: opening.span.start,
                    end: token.span.end,
                    line: opening.span.line,
                    column: opening.span.column,
                },
            };
            return Ok(Some((node, index + 1)));
        }

        if parse_macro_header_line(text).is_some() {
            let nested: Option<(BodyNode<'source>, usize)> = parse_multiline_macro(tokens, index)?;
            let Some((nested, next_index)) = nested else {
                unreachable!("已识别的 Macro 开头必须产生节点或错误");
            };
            body.push(nested);
            index = next_index;
            continue;
        }

        let nodes: Vec<BodyNode<'source>> = parse_body_nodes(text, token.source, token.span)?;
        body.extend(nodes);
        index += 1;
    }

    Err(ParseError {
        source: opening.source,
        kind: ParseErrorKind::UnclosedMacro { name },
        span: opening.span,
    })
}

/// 编译器固有容器即使缺少闭合也必须立即报错；动态 Macro 则由显式闭合决定形态，
/// Runtime Definition 会继续验证调用形态是否与 `body` 契约一致。
fn requires_container_syntax(name: &str) -> bool {
    matches!(
        name,
        "for"
            | "while"
            | "silently"
            | "capture"
            | "widget"
            | "link"
            | "button"
            | "replace"
            | "slot"
    )
}

fn has_matching_closing(tokens: &[Token<'_>], start: usize, name: &str) -> bool {
    for token in &tokens[start..] {
        match token.kind {
            TokenKind::PassageDeclaration { .. } => return false,
            TokenKind::Text(text) if parse_macro_closing_line(text) == Some(name) => return true,
            TokenKind::Text(_) => {}
        }
    }
    false
}

/// `case` 与 `default` 由外层 `switch` 闭合。
fn parse_switch_macro<'source>(
    tokens: &[Token<'source>],
    opening_index: usize,
    opening_text: &'source str,
    arguments: &'source str,
) -> Result<Option<(BodyNode<'source>, usize)>, ParseError<'source>> {
    let opening: &Token<'source> = &tokens[opening_index];
    let mut body: Vec<BodyNode<'source>> = Vec::new();
    let mut clause: Option<PendingIfClause<'source>> = None;
    let mut index: usize = opening_index + 1;

    while index < tokens.len() {
        let token: &Token<'source> = &tokens[index];
        let TokenKind::Text(text) = token.kind else {
            return Err(ParseError {
                source: opening.source,
                kind: ParseErrorKind::UnclosedMacro { name: "switch" },
                span: opening.span,
            });
        };

        if let Some(closing_name) = parse_macro_closing_line(text) {
            if closing_name != "switch" {
                return Err(ParseError {
                    source: token.source,
                    kind: ParseErrorKind::MismatchedMacroClosing {
                        expected: "switch",
                        found: closing_name,
                    },
                    span: token.span,
                });
            }
            finish_if_clause(&mut body, clause.take(), token.span.start);
            return Ok(Some((
                BodyNode {
                    kind: BodyNodeKind::Macro(MacroNode {
                        name: "switch",
                        arguments,
                        arguments_span: macro_arguments_span(opening_text, opening.span, arguments),
                        syntax_kind: crate::twee::MacroSyntaxKind::Container,
                        body,
                    }),
                    span: Span {
                        start: opening.span.start,
                        end: token.span.end,
                        line: opening.span.line,
                        column: opening.span.column,
                    },
                },
                index + 1,
            )));
        }

        if let Some((clause_name @ ("case" | "default"), clause_arguments)) =
            parse_macro_header_line(text)
        {
            finish_if_clause(&mut body, clause.take(), token.span.start);
            clause = Some(PendingIfClause {
                name: clause_name,
                arguments: clause_arguments,
                arguments_span: macro_arguments_span(text, token.span, clause_arguments),
                span: token.span,
                body: Vec::new(),
            });
            index += 1;
            continue;
        }

        if parse_macro_header_line(text).is_some() {
            let nested: Option<(BodyNode<'source>, usize)> = parse_multiline_macro(tokens, index)?;
            let Some((nested, next_index)) = nested else {
                unreachable!("已识别的 Macro 开头必须产生节点或错误");
            };
            append_if_body(&mut body, &mut clause, vec![nested]);
            index = next_index;
            continue;
        }

        let nodes: Vec<BodyNode<'source>> = parse_body_nodes(text, token.source, token.span)?;
        append_if_body(&mut body, &mut clause, nodes);
        index += 1;
    }

    Err(ParseError {
        source: opening.source,
        kind: ParseErrorKind::UnclosedMacro { name: "switch" },
        span: opening.span,
    })
}

/// `elseif` 与 `else` 由外层 `if` 闭合，不要求各自的结束标签。
fn parse_if_macro<'source>(
    tokens: &[Token<'source>],
    opening_index: usize,
    opening_text: &'source str,
    arguments: &'source str,
) -> Result<Option<(BodyNode<'source>, usize)>, ParseError<'source>> {
    let opening: &Token<'source> = &tokens[opening_index];
    let mut body: Vec<BodyNode<'source>> = Vec::new();
    let mut clause: Option<PendingIfClause<'source>> = None;
    let mut index: usize = opening_index + 1;

    while index < tokens.len() {
        let token: &Token<'source> = &tokens[index];
        let TokenKind::Text(text) = token.kind else {
            return Err(ParseError {
                source: opening.source,
                kind: ParseErrorKind::UnclosedMacro { name: "if" },
                span: opening.span,
            });
        };

        if let Some(closing_name) = parse_macro_closing_line(text) {
            if closing_name != "if" {
                return Err(ParseError {
                    source: token.source,
                    kind: ParseErrorKind::MismatchedMacroClosing {
                        expected: "if",
                        found: closing_name,
                    },
                    span: token.span,
                });
            }
            finish_if_clause(&mut body, clause.take(), token.span.start);
            return Ok(Some((
                BodyNode {
                    kind: BodyNodeKind::Macro(MacroNode {
                        name: "if",
                        arguments,
                        arguments_span: macro_arguments_span(opening_text, opening.span, arguments),
                        syntax_kind: crate::twee::MacroSyntaxKind::Container,
                        body,
                    }),
                    span: Span {
                        start: opening.span.start,
                        end: token.span.end,
                        line: opening.span.line,
                        column: opening.span.column,
                    },
                },
                index + 1,
            )));
        }

        if let Some((clause_name @ ("elseif" | "else"), clause_arguments)) =
            parse_macro_header_line(text)
        {
            finish_if_clause(&mut body, clause.take(), token.span.start);
            clause = Some(PendingIfClause {
                name: clause_name,
                arguments: clause_arguments,
                arguments_span: macro_arguments_span(text, token.span, clause_arguments),
                span: token.span,
                body: Vec::new(),
            });
            index += 1;
            continue;
        }

        if parse_macro_header_line(text).is_some() {
            let nested: Option<(BodyNode<'source>, usize)> = parse_multiline_macro(tokens, index)?;
            let Some((nested, next_index)) = nested else {
                unreachable!("已识别的 Macro 开头必须产生节点或错误");
            };
            append_if_body(&mut body, &mut clause, vec![nested]);
            index = next_index;
            continue;
        }

        let nodes: Vec<BodyNode<'source>> = parse_body_nodes(text, token.source, token.span)?;
        append_if_body(&mut body, &mut clause, nodes);
        index += 1;
    }

    Err(ParseError {
        source: opening.source,
        kind: ParseErrorKind::UnclosedMacro { name: "if" },
        span: opening.span,
    })
}

struct PendingIfClause<'source> {
    name: &'source str,
    arguments: &'source str,
    arguments_span: Span,
    span: Span,
    body: Vec<BodyNode<'source>>,
}

fn append_if_body<'source>(
    body: &mut Vec<BodyNode<'source>>,
    clause: &mut Option<PendingIfClause<'source>>,
    nodes: Vec<BodyNode<'source>>,
) {
    match clause {
        Some(clause) => clause.body.extend(nodes),
        None => body.extend(nodes),
    }
}

fn finish_if_clause<'source>(
    body: &mut Vec<BodyNode<'source>>,
    clause: Option<PendingIfClause<'source>>,
    end: usize,
) {
    if let Some(clause) = clause {
        body.push(BodyNode {
            kind: BodyNodeKind::Macro(MacroNode {
                name: clause.name,
                arguments: clause.arguments,
                arguments_span: clause.arguments_span,
                syntax_kind: crate::twee::MacroSyntaxKind::Container,
                body: clause.body,
            }),
            span: Span {
                start: clause.span.start,
                end,
                line: clause.span.line,
                column: clause.span.column,
            },
        });
    }
}

/// 读取独占一行的 Macro 开头。
fn parse_macro_header_line(text: &str) -> Option<(&str, &str)> {
    let line: &str = strip_line_ending(text);
    let header: &str = line.strip_prefix("<<")?.strip_suffix(">>")?.trim();
    if header.is_empty() || header.starts_with('/') || has_ungrouped_shift(header) {
        return None;
    }

    let separator: Option<usize> = header.find(char::is_whitespace);
    Some(match separator {
        Some(index) => (&header[..index], header[index..].trim()),
        None => (header, ""),
    })
}

fn has_ungrouped_shift(source: &str) -> bool {
    let bytes: &[u8] = source.as_bytes();
    let mut depth: usize = 0;
    let mut quote: Option<u8> = None;
    let mut escaped: bool = false;
    let mut index: usize = 0;
    while index < bytes.len() {
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
            b'<' | b'>' if depth == 0 && bytes.get(index + 1) == Some(&byte) => return true,
            _ => {}
        }
        index += 1;
    }
    false
}

/// 读取独占一行的 Macro 结尾。
fn parse_macro_closing_line(text: &str) -> Option<&str> {
    strip_line_ending(text)
        .strip_prefix("<</")?
        .strip_suffix(">>")
        .map(str::trim)
}

fn strip_line_ending(text: &str) -> &str {
    let line: &str = text.strip_suffix('\n').unwrap_or(text);
    line.strip_suffix('\r').unwrap_or(line)
}

/// 把单个 Text Token 分成字面文本、Inline Macro 与允许同行书写的固有容器。
fn parse_body_nodes<'source, 'source_path>(
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
fn macro_arguments_span(text: &str, parent: Span, arguments: &str) -> Span {
    let start: usize = if arguments.is_empty() {
        text.find(">>").unwrap_or(text.len())
    } else {
        text.find(arguments).unwrap_or(0)
    };
    relative_span(text, parent, start, start + arguments.len())
}

/// 将单行 Token 内的相对字节范围转换为 Twee Source 范围。
fn relative_span(text: &str, parent: Span, start: usize, end: usize) -> Span {
    let preceding_characters: usize = text[..start].chars().count();
    Span {
        start: parent.start + start,
        end: parent.start + end,
        line: parent.line,
        column: parent.column + preceding_characters,
    }
}

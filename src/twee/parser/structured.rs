//! 跨 Token 的容器 Macro、分支和循环正文解析。
//!
//! 本模块消费完整 Token 行并负责嵌套层级、结束标签以及 if/switch 子句归组；
//! 单行正文的字符级扫描委托给 inline 模块，避免两套解析器重复处理 Macro 头。

use super::*;

/// 组合完整闭合的跨行 Macro，正文暂时只保留 Text。
pub(super) fn parse_multiline_macro<'source>(
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
pub(super) fn parse_macro_closing_line(text: &str) -> Option<&str> {
    strip_line_ending(text)
        .strip_prefix("<</")?
        .strip_suffix(">>")
        .map(str::trim)
}

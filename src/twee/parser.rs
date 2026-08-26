//! Twee 解析：把 Token 组合成 Passage 与正文节点。

use super::*;

mod inline;
mod structured;

use inline::{macro_arguments_span, parse_body_nodes, relative_span};
use structured::{parse_macro_closing_line, parse_multiline_macro};

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

fn strip_line_ending(text: &str) -> &str {
    let line: &str = text.strip_suffix('\n').unwrap_or(text);
    line.strip_suffix('\r').unwrap_or(line)
}

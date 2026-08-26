//! Twee 词法：把 Source 拆成声明与正文 Token。

use super::*;

/// 把 Source 逐行词法化为 Token：`::` 开头为 Passage 声明，其余为正文 Text。
pub fn lex(source: &Source) -> Vec<Token<'_>> {
    let mut tokens: Vec<Token<'_>> = Vec::new();
    let mut start: usize = 0;

    for (index, raw_line) in source.content.split_inclusive('\n').enumerate() {
        let line: &str = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line: &str = line.strip_suffix('\r').unwrap_or(line);
        let declaration: Option<&str> = line.strip_prefix("::");
        let kind: TokenKind<'_> = match declaration {
            Some(value) => parse_declaration(value.trim()),
            None => TokenKind::Text(raw_line),
        };
        let end: usize = start + raw_line.len();

        tokens.push(Token {
            source: &source.path,
            content: &source.content,
            kind,
            span: Span {
                start,
                end,
                line: index + 1,
                column: 1,
            },
        });
        start = end;
    }

    tokens
}

/// 从 `::` 后的内容中分离名称与标签。
fn parse_declaration(value: &str) -> TokenKind<'_> {
    let tags: Option<(&str, &str)> =
        value
            .rsplit_once(" [")
            .and_then(|(name, tags): (&str, &str)| {
                tags.strip_suffix(']').map(|tags: &str| (name, tags))
            });

    match tags {
        Some((name, tags)) => TokenKind::PassageDeclaration {
            name: name.trim(),
            tags: tags.split_whitespace().collect(),
        },
        None => TokenKind::PassageDeclaration {
            name: value,
            tags: Vec::new(),
        },
    }
}

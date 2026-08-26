//! Macro 参数原文的轻量顶层扫描。

/// 返回原文首个以空白分隔的单词及其相对范围。
pub(super) fn first_word(source: &str) -> Option<(&str, usize, usize)> {
    let start: usize = trim_start_index(source, 0, source.len());
    let end: usize = source[start..]
        .find(char::is_whitespace)
        .map_or(source.len(), |index: usize| start + index);
    (start < end).then_some((&source[start..end], start, end))
}

/// 去除首尾空白后返回切片及其绝对范围；全空白返回 None。
pub(super) fn trimmed_slice(
    source: &str,
    start: usize,
    end: usize,
) -> Option<(&str, usize, usize)> {
    let trimmed_start: usize = trim_start_index(source, start, end);
    let trimmed_end: usize = trim_end_index(source, trimmed_start, end);
    (trimmed_start < trimmed_end).then_some((
        &source[trimmed_start..trimmed_end],
        trimmed_start,
        trimmed_end,
    ))
}

/// 只在 Expression 顶层识别两侧有空白的 Macro 参数关键字。
pub(super) fn find_top_level_keyword(source: &str, keyword: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut escaped: bool = false;
    let mut depth: usize = 0;

    for (index, character) in source.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }

        match character {
            '\'' | '"' => quote = Some(character),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 && source[index..].starts_with(keyword) => {
                let before_is_space: bool = source[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace);
                let after_index: usize = index + keyword.len();
                let after_is_space: bool = source[after_index..]
                    .chars()
                    .next()
                    .is_some_and(char::is_whitespace);
                if before_is_space && after_is_space {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// 查找 `set` 的普通赋值等号，跳过比较及复合赋值运算符。
pub(super) fn find_top_level_assignment(source: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut escaped: bool = false;
    let mut depth: usize = 0;

    for (index, character) in source.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }

        match character {
            '\'' | '"' => quote = Some(character),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            '=' if depth == 0 => {
                let previous: Option<char> = source[..index].chars().next_back();
                let next: Option<char> = source[index + 1..].chars().next();
                let is_compound: bool = previous.is_some_and(|value: char| {
                    matches!(
                        value,
                        '=' | '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^' | '?'
                    )
                });
                if !is_compound && next != Some('=') {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// 返回 [start, end) 内第一个非空白字符的偏移；全空白返回 end。
pub(super) fn trim_start_index(source: &str, start: usize, end: usize) -> usize {
    source[start..end]
        .char_indices()
        .find(|(_, character): &(usize, char)| !character.is_whitespace())
        .map_or(end, |(index, _)| start + index)
}

fn trim_end_index(source: &str, start: usize, end: usize) -> usize {
    source[start..end]
        .char_indices()
        .rev()
        .find(|(_, character): &(usize, char)| !character.is_whitespace())
        .map_or(start, |(index, character)| {
            start + index + character.len_utf8()
        })
}

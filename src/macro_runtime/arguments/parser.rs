//! Macro Argument List 与 Interaction Target 的边界解析。

use crate::expression::{Expression, ParseError, parse};

use super::{InteractionTarget, InteractionTargetError, MacroArgument, MacroArgumentListError};

/// 解析 link、button、choice 可共用的交互参数外壳。
pub fn parse_interaction_target(
    source: &str,
) -> Result<InteractionTarget<'_>, InteractionTargetError> {
    let inner: &str = source
        .trim()
        .strip_prefix("[[")
        .and_then(|value: &str| value.strip_suffix("]]"))
        .ok_or(InteractionTargetError::InvalidWrapper)?;
    let (label, target): (&str, &str) = inner
        .split_once('|')
        .ok_or(InteractionTargetError::MissingSeparator)?;
    let label: &str = label.trim();
    let target: &str = target.trim();
    if label.is_empty() {
        return Err(InteractionTargetError::EmptyLabel);
    }
    if target.is_empty() {
        return Err(InteractionTargetError::EmptyTarget);
    }
    Ok(InteractionTarget { label, target })
}

/// 解析以顶层空白分隔、可混合 Interaction Target 的 Macro 参数列表。
pub fn parse_argument_list<'source>(
    source: &'source str,
) -> Result<Vec<MacroArgument<'source>>, MacroArgumentListError> {
    let mut arguments: Vec<MacroArgument<'source>> = Vec::new();
    let mut cursor: usize = 0;

    while cursor < source.len() {
        cursor = skip_whitespace(source, cursor);
        if cursor == source.len() {
            break;
        }

        if source[cursor..].starts_with("[[") {
            let relative_end: usize = source[cursor + 2..].find("]]").ok_or(
                MacroArgumentListError::InteractionTarget {
                    error: InteractionTargetError::InvalidWrapper,
                    offset: cursor,
                },
            )?;
            let end: usize = cursor + 2 + relative_end + 2;
            if end < source.len()
                && !source[end..]
                    .chars()
                    .next()
                    .is_some_and(char::is_whitespace)
            {
                return Err(MacroArgumentListError::InteractionTarget {
                    error: InteractionTargetError::InvalidWrapper,
                    offset: cursor,
                });
            }
            let target: InteractionTarget<'source> = parse_interaction_target(&source[cursor..end])
                .map_err(|error: InteractionTargetError| {
                    MacroArgumentListError::InteractionTarget {
                        error,
                        offset: cursor,
                    }
                })?;
            let fragment: &str = &source[cursor..end];
            let inner: &str = &fragment[2..fragment.len() - 2];
            let separator: usize = inner
                .find('|')
                .expect("已解析的 Interaction Target 必有分隔符");
            let label_part: &str = &inner[..separator];
            let target_part: &str = &inner[separator + 1..];
            let label_offset: usize = cursor
                + 2
                + label_part
                    .find(target.label)
                    .expect("已解析的 label 必须来自原参数");
            let target_offset: usize = cursor
                + 2
                + separator
                + 1
                + target_part
                    .find(target.target)
                    .expect("已解析的 target 必须来自原参数");
            arguments.push(MacroArgument::InteractionTarget {
                target,
                label_offset,
                target_offset,
            });
            cursor = end;
            continue;
        }

        let end: usize = expression_argument_end(source, cursor);
        let expression: Expression<'source> =
            parse(&source[cursor..end]).map_err(|error: ParseError| {
                MacroArgumentListError::Expression {
                    error,
                    offset: cursor,
                }
            })?;
        arguments.push(MacroArgument::Expression {
            expression,
            offset: cursor,
        });
        cursor = end;
    }

    Ok(arguments)
}

fn skip_whitespace(source: &str, mut cursor: usize) -> usize {
    while let Some(character) = source[cursor..].chars().next() {
        if !character.is_whitespace() {
            break;
        }
        cursor += character.len_utf8();
    }
    cursor
}

/// 查找一个普通 Expression 参数的末尾；括号和字符串内的空白不分段。
fn expression_argument_end(source: &str, start: usize) -> usize {
    let mut delimiter_stack: Vec<char> = Vec::new();
    let mut quote: Option<char> = None;
    let mut escaped: bool = false;

    for (relative, character) in source[start..].char_indices() {
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
            '(' | '[' | '{' => delimiter_stack.push(character),
            ')' | ']' | '}' => {
                let _: Option<char> = delimiter_stack.pop();
            }
            _ if character.is_whitespace() && delimiter_stack.is_empty() => {
                return start + relative;
            }
            _ => {}
        }
    }
    source.len()
}

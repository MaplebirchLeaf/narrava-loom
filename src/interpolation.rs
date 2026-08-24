//! 显式 Macro 参数中的 `${expression}` 边界扫描。
//!
//! 普通 Twee 正文不调用本模块，也不会自动插值。

/// 从 Expression 起点查找所属 `${...}` 的闭合花括号。
///
/// 嵌套括号里的 `}` 和引号内的字符不会提前结束插值；具体括号是否匹配仍由
/// Expression Parser 报告。
pub fn find_interpolation_end(source: &str, start: usize) -> Option<usize> {
    let mut delimiters: Vec<char> = Vec::new();
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
            '(' | '[' | '{' => delimiters.push(character),
            ')' | ']' => {
                let _: Option<char> = delimiters.pop();
            }
            '}' if delimiters.is_empty() => return Some(start + relative),
            '}' => {
                let _: Option<char> = delimiters.pop();
            }
            _ => {}
        }
    }

    None
}

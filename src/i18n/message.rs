//! `translations.nmsg` 的紧凑块格式。

use std::{collections::BTreeMap, fmt};

use super::{I18nTemplate, I18nTemplateMessage};

const SOURCE: &str = "[source]";
const TRANSLATION: &str = "[translation]";
const VALUES: &str = "[values]";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum I18nMessageError {
    InvalidSyntax { line: usize, message: String },
    UnknownMessage { id: String },
    DuplicateMessage { id: String },
    SourceMismatch { id: String },
    DuplicateValue { id: String, name: String },
}

pub(super) fn encode(template: &I18nTemplate) -> String {
    let mut output: String = String::new();
    for (index, (id, message)) in template.passages().iter().enumerate() {
        if index > 0 {
            output.push_str("\n\n");
        }
        output.push_str(":: ");
        output.push_str(id);
        output.push('\n');
        write_section(&mut output, SOURCE, message.source());
        write_section(&mut output, TRANSLATION, message.text());
        if !message.values().is_empty() {
            output.push_str(VALUES);
            output.push('\n');
            for (value_index, (name, dictionary)) in message.values().iter().enumerate() {
                if value_index > 0 {
                    output.push('\n');
                }
                output.push_str(name);
                output.push_str(" = ");
                output.push_str(dictionary);
            }
        } else if output.ends_with('\n') {
            output.pop();
        }
    }
    output
}

pub(super) fn decode(
    language: &str,
    input: &str,
    dictionary: BTreeMap<String, BTreeMap<String, String>>,
) -> Result<I18nTemplate, I18nMessageError> {
    let passages: BTreeMap<String, I18nTemplateMessage> = parse(input)?;
    Ok(I18nTemplate::new(language, dictionary, passages))
}

pub(super) fn apply(
    template: &I18nTemplate,
    input: &str,
) -> Result<I18nTemplate, I18nMessageError> {
    let parsed: BTreeMap<String, I18nTemplateMessage> = parse(input)?;
    let mut passages: BTreeMap<String, I18nTemplateMessage> = template.passages().clone();
    for (id, message) in parsed {
        let Some(previous) = template.passages().get(&id) else {
            return Err(I18nMessageError::UnknownMessage { id });
        };
        if message.source() != previous.source() {
            return Err(I18nMessageError::SourceMismatch { id });
        }
        passages.insert(id, message);
    }
    Ok(I18nTemplate::new(
        template.language(),
        template.dictionary().clone(),
        passages,
    ))
}

fn parse(input: &str) -> Result<BTreeMap<String, I18nTemplateMessage>, I18nMessageError> {
    if input.contains('\r') {
        return Err(syntax(1, "只接受 LF 换行"));
    }
    let lines: Vec<&str> = input.split('\n').collect();
    let mut index: usize = 0;
    let mut messages: BTreeMap<String, I18nTemplateMessage> = BTreeMap::new();
    while index < lines.len() && !(index + 1 == lines.len() && lines[index].is_empty()) {
        let line_number: usize = index + 1;
        let Some(id) = lines[index].strip_prefix(":: ") else {
            return Err(syntax(line_number, "消息必须以 `:: id` 开始"));
        };
        if id.is_empty() {
            return Err(syntax(line_number, "消息 ID 不能为空"));
        }
        index += 1;
        expect_line(&lines, &mut index, SOURCE)?;
        let source: String = collect_text(&lines, &mut index, &[TRANSLATION])?;
        expect_line(&lines, &mut index, TRANSLATION)?;
        let translation: String = collect_text(&lines, &mut index, &[VALUES, ":: "])?;
        let values: BTreeMap<String, String> = if lines.get(index) == Some(&VALUES) {
            index += 1;
            collect_values(&lines, &mut index, id)?
        } else {
            BTreeMap::new()
        };
        validate_message_separator(&lines, index)?;
        if messages
            .insert(
                id.to_owned(),
                I18nTemplateMessage::new(source, translation, values),
            )
            .is_some()
        {
            return Err(I18nMessageError::DuplicateMessage { id: id.to_owned() });
        }
    }
    Ok(messages)
}

fn write_section(output: &mut String, marker: &str, text: &str) {
    output.push_str(marker);
    output.push('\n');
    output.push_str(text);
    if !text.is_empty() && !text.ends_with('\n') {
        output.push('\n');
    }
}

fn expect_line(lines: &[&str], index: &mut usize, expected: &str) -> Result<(), I18nMessageError> {
    if lines.get(*index) != Some(&expected) {
        return Err(syntax(*index + 1, format!("缺少 `{expected}`")));
    }
    *index += 1;
    Ok(())
}

fn collect_text(
    lines: &[&str],
    index: &mut usize,
    stops: &[&str],
) -> Result<String, I18nMessageError> {
    let start: usize = *index;
    while *index < lines.len() && !is_stop(lines[*index], stops) {
        *index += 1;
    }
    if *index == lines.len() && !stops.contains(&":: ") {
        return Err(syntax(start + 1, "文本 section 未闭合"));
    }
    let mut end: usize = *index;
    // 两条消息之间的单个空行只用于阅读，不属于 translation 内容。
    if lines
        .get(*index)
        .is_some_and(|line: &&str| line.starts_with(":: "))
        && end > start
        && lines[end - 1].is_empty()
    {
        end -= 1;
    }
    Ok(lines[start..end].join("\n"))
}

fn collect_values(
    lines: &[&str],
    index: &mut usize,
    id: &str,
) -> Result<BTreeMap<String, String>, I18nMessageError> {
    let mut values: BTreeMap<String, String> = BTreeMap::new();
    while *index < lines.len() && !lines[*index].starts_with(":: ") {
        let line: &str = lines[*index];
        if line.is_empty()
            && lines
                .get(*index + 1)
                .is_some_and(|next: &&str| next.starts_with(":: "))
        {
            *index += 1;
            break;
        }
        if line.is_empty() && *index + 1 == lines.len() {
            break;
        }
        let Some((name, dictionary)) = line.split_once(" = ") else {
            return Err(syntax(*index + 1, "values 必须使用 `name = dictionary`"));
        };
        if name.is_empty()
            || values
                .insert(name.to_owned(), dictionary.to_owned())
                .is_some()
        {
            return Err(I18nMessageError::DuplicateValue {
                id: id.to_owned(),
                name: name.to_owned(),
            });
        }
        *index += 1;
    }
    Ok(values)
}

fn is_stop(line: &str, stops: &[&str]) -> bool {
    stops
        .iter()
        .any(|stop: &&str| *stop == line || (*stop == ":: " && line.starts_with(":: ")))
}

fn validate_message_separator(lines: &[&str], index: usize) -> Result<(), I18nMessageError> {
    if !lines
        .get(index)
        .is_some_and(|line: &&str| line.starts_with(":: "))
    {
        return Ok(());
    }
    if index == 0 || !lines[index - 1].is_empty() {
        return Err(syntax(index + 1, "两条消息之间必须保留一个空行"));
    }
    if index >= 2 && lines[index - 2].is_empty() {
        return Err(syntax(index, "两条消息之间只能保留一个空行"));
    }
    Ok(())
}

fn syntax(line: usize, message: impl Into<String>) -> I18nMessageError {
    I18nMessageError::InvalidSyntax {
        line,
        message: message.into(),
    }
}

impl fmt::Display for I18nMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSyntax { line, message } => {
                write!(formatter, "NMSG 第 {line} 行无效: {message}")
            }
            Self::UnknownMessage { id } => write!(formatter, "NMSG 含未知消息: {id}"),
            Self::DuplicateMessage { id } => write!(formatter, "NMSG 消息重复: {id}"),
            Self::SourceMismatch { id } => write!(formatter, "NMSG 原文已被修改: {id}"),
            Self::DuplicateValue { id, name } => {
                write!(formatter, "NMSG 消息 {id} 的 value 重复或为空: {name}")
            }
        }
    }
}

impl std::error::Error for I18nMessageError {}

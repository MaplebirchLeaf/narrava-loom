//! 已验证译文、默认文本和 Runtime placeholder 值的解析。

use std::collections::BTreeMap;

use super::{
    I18nCatalog, I18nTemplateMessage, I18nValidatedTemplate,
    template::{I18nTemplatePart, parse_template},
};

/// 最终文本来自目标语言还是默认语言回退。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum I18nTextOrigin {
    Translation,
    Default,
}

/// 一次 I18n 查询产生的宿主无关文本。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I18nResolvedText {
    text: String,
    origin: I18nTextOrigin,
}

impl I18nResolvedText {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn origin(&self) -> I18nTextOrigin {
        self.origin
    }
}

/// Runtime 无法安全解析某条目录消息的原因。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum I18nResolveError {
    DifferentCatalog,
    UnknownMessage { id: String },
    MissingValue { id: String, placeholder: String },
    InvalidTemplate { id: String },
}

pub(super) fn resolve(
    catalog: &I18nCatalog,
    translation: &I18nValidatedTemplate,
    id: &str,
    values: &BTreeMap<String, String>,
) -> Result<I18nResolvedText, I18nResolveError> {
    if !translation.belongs_to(catalog) {
        return Err(I18nResolveError::DifferentCatalog);
    }
    let source = catalog
        .messages()
        .iter()
        .find(|message| message.id().as_str() == id)
        .ok_or_else(|| I18nResolveError::UnknownMessage { id: id.to_owned() })?;
    let translated: Option<&I18nTemplateMessage> = translation
        .passages()
        .get(id)
        .filter(|message: &&I18nTemplateMessage| !message.text().is_empty());
    let (pattern, bindings, origin): (&str, Option<&BTreeMap<String, String>>, I18nTextOrigin) =
        match translated {
            Some(message) => (
                message.text(),
                Some(message.values()),
                I18nTextOrigin::Translation,
            ),
            None => (source.text(), None, I18nTextOrigin::Default),
        };
    let pattern: &str = if translated.is_some() {
        pattern.trim_end_matches('\n')
    } else {
        pattern
    };
    let mut text: String = render(id, pattern, values, bindings, translation.dictionary())?;
    if translated.is_some() {
        text.push_str(&source.text()[source.text().trim_end_matches('\n').len()..]);
    }
    Ok(I18nResolvedText { text, origin })
}

pub(super) fn resolve_default(
    catalog: &I18nCatalog,
    id: &str,
    values: &BTreeMap<String, String>,
) -> Result<I18nResolvedText, I18nResolveError> {
    let source = catalog
        .messages()
        .iter()
        .find(|message| message.id().as_str() == id)
        .ok_or_else(|| I18nResolveError::UnknownMessage { id: id.to_owned() })?;
    let text: String = render(id, source.text(), values, None, &BTreeMap::new())?;
    Ok(I18nResolvedText {
        text,
        origin: I18nTextOrigin::Default,
    })
}

fn render(
    id: &str,
    pattern: &str,
    values: &BTreeMap<String, String>,
    bindings: Option<&BTreeMap<String, String>>,
    dictionaries: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<String, I18nResolveError> {
    let mut output: String = String::new();
    let parts: Vec<I18nTemplatePart<'_>> =
        parse_template(pattern).ok_or_else(|| invalid_template(id))?;
    for part in parts {
        match part {
            I18nTemplatePart::Text(text) => output.push_str(text),
            I18nTemplatePart::LiteralBrace(character) => output.push(character),
            I18nTemplatePart::Placeholder(name) => {
                let raw: &String =
                    values
                        .get(name)
                        .ok_or_else(|| I18nResolveError::MissingValue {
                            id: id.to_owned(),
                            placeholder: name.to_owned(),
                        })?;
                output.push_str(resolve_dictionary_value(name, raw, bindings, dictionaries));
            }
        }
    }
    Ok(output)
}

fn resolve_dictionary_value<'value>(
    placeholder: &str,
    raw: &'value str,
    bindings: Option<&BTreeMap<String, String>>,
    dictionaries: &'value BTreeMap<String, BTreeMap<String, String>>,
) -> &'value str {
    let Some(dictionary_name) = bindings.and_then(|values| values.get(placeholder)) else {
        return raw;
    };
    if dictionary_name.is_empty() {
        return raw;
    }
    dictionaries
        .get(dictionary_name)
        .and_then(|dictionary| dictionary.get(raw))
        .map(String::as_str)
        .unwrap_or(raw)
}

fn invalid_template(id: &str) -> I18nResolveError {
    I18nResolveError::InvalidTemplate { id: id.to_owned() }
}

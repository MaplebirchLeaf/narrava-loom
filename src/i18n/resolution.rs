//! 已验证译文、默认文本和 Runtime placeholder 值的解析。

use std::collections::{BTreeMap, BTreeSet};

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
    /// 最终渲染出的文本。
    pub fn text(&self) -> &str {
        &self.text
    }

    /// 文本来自目标语言译文还是默认原文回退。
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

/// 用已验证译文解析一条消息；译文缺失时回退默认原文。
pub(super) fn resolve(
    catalog: &I18nCatalog,
    translation: &I18nValidatedTemplate,
    id: &str,
    values: &BTreeMap<String, String>,
) -> Result<I18nResolvedText, I18nResolveError> {
    resolve_with_dictionary_values(catalog, translation, id, values, None)
}

/// VM 保留原始 Value 类型时使用；只有集合中的 placeholder 可以查询动态字典。
pub(super) fn resolve_runtime(
    catalog: &I18nCatalog,
    translation: &I18nValidatedTemplate,
    id: &str,
    values: &BTreeMap<String, String>,
    dictionary_values: &BTreeSet<String>,
) -> Result<I18nResolvedText, I18nResolveError> {
    resolve_with_dictionary_values(catalog, translation, id, values, Some(dictionary_values))
}

/// 解析公共路径：校验目录身份、选择译文或默认原文、渲染并保留尾部空白。
fn resolve_with_dictionary_values(
    catalog: &I18nCatalog,
    translation: &I18nValidatedTemplate,
    id: &str,
    values: &BTreeMap<String, String>,
    dictionary_values: Option<&BTreeSet<String>>,
) -> Result<I18nResolvedText, I18nResolveError> {
    if !translation.belongs_to(catalog) {
        return Err(I18nResolveError::DifferentCatalog);
    }
    let source = catalog
        .message(id)
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
    let mut text: String = render(
        id,
        pattern,
        values,
        bindings,
        translation.dictionary(),
        dictionary_values,
    )?;
    if translated.is_some() {
        text.push_str(&source.text()[source.text().trim_end_matches('\n').len()..]);
    }
    Ok(I18nResolvedText { text, origin })
}

/// 不加载目标语言时，用默认原文与模板规则直接解析。
pub(super) fn resolve_default(
    catalog: &I18nCatalog,
    id: &str,
    values: &BTreeMap<String, String>,
) -> Result<I18nResolvedText, I18nResolveError> {
    let source = catalog
        .message(id)
        .ok_or_else(|| I18nResolveError::UnknownMessage { id: id.to_owned() })?;
    let text: String = render(id, source.text(), values, None, &BTreeMap::new(), None)?;
    Ok(I18nResolvedText {
        text,
        origin: I18nTextOrigin::Default,
    })
}

/// 把模板片段渲染为文本：占位符取值，并按绑定查询动态字典。
fn render(
    id: &str,
    pattern: &str,
    values: &BTreeMap<String, String>,
    bindings: Option<&BTreeMap<String, String>>,
    dictionaries: &BTreeMap<String, BTreeMap<String, String>>,
    dictionary_values: Option<&BTreeSet<String>>,
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
                output.push_str(resolve_dictionary_value(
                    name,
                    raw,
                    bindings,
                    dictionaries,
                    dictionary_values,
                ));
            }
        }
    }
    Ok(output)
}

/// 按占位符绑定查询动态字典；无绑定、绑定为空或未命中时原样返回。
fn resolve_dictionary_value<'value>(
    placeholder: &str,
    raw: &'value str,
    bindings: Option<&BTreeMap<String, String>>,
    dictionaries: &'value BTreeMap<String, BTreeMap<String, String>>,
    dictionary_values: Option<&BTreeSet<String>>,
) -> &'value str {
    if dictionary_values.is_some_and(|eligible: &BTreeSet<String>| !eligible.contains(placeholder))
    {
        return raw;
    }
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

/// 构造 `InvalidTemplate` 错误的简写。
fn invalid_template(id: &str) -> I18nResolveError {
    I18nResolveError::InvalidTemplate { id: id.to_owned() }
}

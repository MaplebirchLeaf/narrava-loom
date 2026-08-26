//! 外部译文进入 Core 前的确定性校验。

use std::collections::{BTreeMap, BTreeSet};

use super::{
    I18nCatalog, I18nCatalogIdentity, I18nMessage, I18nTemplate, I18nTemplateMessage,
    template::{I18nTemplatePart, parse_template},
};

/// 译文文件相对默认语言目录不合法的原因。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum I18nValidationError {
    InvalidLanguageTag {
        language: String,
    },
    UnknownMessage {
        id: String,
    },
    SourceMismatch {
        id: String,
    },
    InvalidPlaceholderSyntax {
        id: String,
    },
    MissingPlaceholder {
        id: String,
        name: String,
    },
    UnknownPlaceholder {
        id: String,
        name: String,
    },
    UnknownDictionary {
        id: String,
        placeholder: String,
        dictionary: String,
    },
}

/// 已通过目录校验、可以安全进入后续解析阶段的译文。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I18nValidatedTemplate {
    catalog: I18nCatalogIdentity,
    template: I18nTemplate,
}

impl I18nValidatedTemplate {
    /// 目标语言标签。
    pub fn language(&self) -> &str {
        self.template.language()
    }

    /// 动态字典。
    pub fn dictionary(&self) -> &BTreeMap<String, BTreeMap<String, String>> {
        self.template.dictionary()
    }

    /// 消息表。
    pub fn passages(&self) -> &BTreeMap<String, I18nTemplateMessage> {
        self.template.passages()
    }

    /// 消耗校验结果，取回模板本体。
    pub fn into_template(self) -> I18nTemplate {
        self.template
    }

    /// 译文是否由该目录构建校验；仅 crate 内部使用。
    pub(super) fn belongs_to(&self, catalog: &I18nCatalog) -> bool {
        self.catalog == catalog.identity
    }
}

/// 校验语言标签、消息存在性、原文一致性与 placeholder 集合；全部通过才返回已验证模板。
pub(super) fn validate(
    catalog: &I18nCatalog,
    template: I18nTemplate,
) -> Result<I18nValidatedTemplate, Vec<I18nValidationError>> {
    let mut errors: Vec<I18nValidationError> = Vec::new();
    if !is_language_tag_well_formed(template.language()) {
        errors.push(I18nValidationError::InvalidLanguageTag {
            language: template.language().to_owned(),
        });
    }

    let source_messages: BTreeMap<&str, &I18nMessage> = catalog
        .messages()
        .iter()
        .map(|message: &I18nMessage| (message.id().as_str(), message))
        .collect();
    for (id, translated) in template.passages() {
        let Some(source) = source_messages.get(id.as_str()) else {
            errors.push(I18nValidationError::UnknownMessage { id: id.clone() });
            continue;
        };
        validate_message(id, source, translated, template.dictionary(), &mut errors);
    }

    if errors.is_empty() {
        Ok(I18nValidatedTemplate {
            catalog: catalog.identity,
            template,
        })
    } else {
        Err(errors)
    }
}

/// 校验单条消息的原文、placeholder 与字典绑定，错误累积到 `errors`。
fn validate_message(
    id: &str,
    source: &I18nMessage,
    translated: &I18nTemplateMessage,
    dictionary: &BTreeMap<String, BTreeMap<String, String>>,
    errors: &mut Vec<I18nValidationError>,
) {
    if translated.source().trim_end_matches('\n') != source.text().trim_end_matches('\n') {
        errors.push(I18nValidationError::SourceMismatch { id: id.to_owned() });
    }
    let (expected, actual) = validate_text(id, source, translated.text(), errors);
    for (placeholder, dictionary_name) in translated.values() {
        if !expected.contains(placeholder.as_str()) && !actual.contains(placeholder) {
            errors.push(I18nValidationError::UnknownPlaceholder {
                id: id.to_owned(),
                name: placeholder.clone(),
            });
        }
        if !dictionary_name.is_empty() && !dictionary.contains_key(dictionary_name) {
            errors.push(I18nValidationError::UnknownDictionary {
                id: id.to_owned(),
                placeholder: placeholder.clone(),
                dictionary: dictionary_name.clone(),
            });
        }
    }
}

/// 解析译文模板并核对占位符集合，返回（期望集合，实际集合）。
fn validate_text<'source>(
    id: &str,
    source: &'source I18nMessage,
    text: &str,
    errors: &mut Vec<I18nValidationError>,
) -> (BTreeSet<&'source str>, BTreeSet<String>) {
    let expected: BTreeSet<&str> = source
        .placeholders()
        .iter()
        .map(|placeholder| placeholder.name())
        .collect();
    let actual: BTreeSet<String> = if text.is_empty() {
        expected
            .iter()
            .map(|name: &&str| (*name).to_owned())
            .collect()
    } else {
        match parse_template(text) {
            Some(parts) => parts
                .into_iter()
                .filter_map(|part: I18nTemplatePart<'_>| match part {
                    I18nTemplatePart::Placeholder(name) => Some(name.to_owned()),
                    I18nTemplatePart::Text(_) | I18nTemplatePart::LiteralBrace(_) => None,
                })
                .collect(),
            None => {
                errors.push(I18nValidationError::InvalidPlaceholderSyntax { id: id.to_owned() });
                BTreeSet::new()
            }
        }
    };

    for name in &expected {
        if !actual.contains(*name) {
            errors.push(I18nValidationError::MissingPlaceholder {
                id: id.to_owned(),
                name: (*name).to_owned(),
            });
        }
    }
    for name in &actual {
        if !expected.contains(name.as_str()) {
            errors.push(I18nValidationError::UnknownPlaceholder {
                id: id.to_owned(),
                name: name.clone(),
            });
        }
    }
    (expected, actual)
}

/// 接受常见 BCP 47 形状；完整注册表语义留给以后独立的 locale 层。
pub fn is_language_tag_well_formed(language: &str) -> bool {
    let mut subtags = language.split('-');
    let Some(primary) = subtags.next() else {
        return false;
    };
    if !(2..=8).contains(&primary.len()) || !primary.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return false;
    }
    subtags.all(|subtag: &str| {
        (1..=8).contains(&subtag.len()) && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
    })
}

//! 译者模板的无文件系统合并规则。

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use super::{
    I18nCatalog, I18nMessage, I18nTemplate, I18nTemplateMessage,
    template::{I18nTemplatePart, parse_template},
};

/// 上次导出的消息为何不能继续进入当前可运行模板。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum I18nExportObsoleteReason {
    Removed,
    Incompatible,
}

/// 被完整保留、等待翻译者或工具处理的旧消息。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I18nExportObsolete {
    id: String,
    reason: I18nExportObsoleteReason,
    message: I18nTemplateMessage,
}

impl I18nExportObsolete {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn reason(&self) -> I18nExportObsoleteReason {
        self.reason
    }

    pub fn message(&self) -> &I18nTemplateMessage {
        &self.message
    }
}

/// 一次可序列化模板合并及其确定性变更报告。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I18nExport {
    template: I18nTemplate,
    added: Vec<String>,
    retained: Vec<String>,
    obsolete: Vec<I18nExportObsolete>,
}

impl I18nExport {
    pub fn template(&self) -> &I18nTemplate {
        &self.template
    }

    pub fn into_template(self) -> I18nTemplate {
        self.template
    }

    pub fn added(&self) -> &[String] {
        &self.added
    }

    pub fn retained(&self) -> &[String] {
        &self.retained
    }

    pub fn obsolete(&self) -> &[I18nExportObsolete] {
        &self.obsolete
    }
}

/// 两种语言文件不能通过合并隐式互相覆盖。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum I18nExportError {
    LanguageMismatch { requested: String, previous: String },
}

impl fmt::Display for I18nExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LanguageMismatch {
                requested,
                previous,
            } => write!(
                formatter,
                "请求导出语言 {requested} 与旧模板语言 {previous} 不一致"
            ),
        }
    }
}

impl std::error::Error for I18nExportError {}

pub(super) fn export(
    catalog: &I18nCatalog,
    language: String,
    previous: Option<&I18nTemplate>,
) -> Result<I18nExport, I18nExportError> {
    if let Some(previous) = previous
        && previous.language() != language
    {
        return Err(I18nExportError::LanguageMismatch {
            requested: language,
            previous: previous.language().to_owned(),
        });
    }

    let generated: I18nTemplate = catalog.template(language);
    let dictionaries: BTreeMap<String, BTreeMap<String, String>> = previous
        .map(I18nTemplate::dictionary)
        .cloned()
        .unwrap_or_default();
    let mut passages: BTreeMap<String, I18nTemplateMessage> = BTreeMap::new();
    let mut added: Vec<String> = Vec::new();
    let mut retained: Vec<String> = Vec::new();
    let mut obsolete: Vec<I18nExportObsolete> = Vec::new();

    for (id, fresh) in generated.passages() {
        let source: &I18nMessage = catalog
            .messages()
            .iter()
            .find(|message: &&I18nMessage| message.id().as_str() == id)
            .expect("导出模板消息必须来自当前目录");
        match previous.and_then(|template: &I18nTemplate| template.passages().get(id)) {
            Some(old) if is_compatible(source, old, &dictionaries) => {
                passages.insert(id.clone(), old.clone());
                retained.push(id.clone());
            }
            Some(old) => {
                passages.insert(id.clone(), fresh.clone());
                added.push(id.clone());
                obsolete.push(I18nExportObsolete {
                    id: id.clone(),
                    reason: I18nExportObsoleteReason::Incompatible,
                    message: old.clone(),
                });
            }
            None => {
                passages.insert(id.clone(), fresh.clone());
                added.push(id.clone());
            }
        }
    }

    if let Some(previous) = previous {
        for (id, old) in previous.passages() {
            if !generated.passages().contains_key(id) {
                obsolete.push(I18nExportObsolete {
                    id: id.clone(),
                    reason: I18nExportObsoleteReason::Removed,
                    message: old.clone(),
                });
            }
        }
    }
    obsolete.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(I18nExport {
        template: I18nTemplate::new(generated.language(), dictionaries, passages),
        added,
        retained,
        obsolete,
    })
}

fn is_compatible(
    source: &I18nMessage,
    previous: &I18nTemplateMessage,
    dictionaries: &BTreeMap<String, BTreeMap<String, String>>,
) -> bool {
    if previous.source() != source.text() {
        return false;
    }
    let expected: BTreeSet<&str> = source
        .placeholders()
        .iter()
        .map(|placeholder| placeholder.name())
        .collect();
    let actual: BTreeSet<&str> = if previous.text().is_empty() {
        expected.clone()
    } else {
        let Some(parts) = parse_template(previous.text()) else {
            return false;
        };
        parts
            .iter()
            .filter_map(|part: &I18nTemplatePart<'_>| match part {
                I18nTemplatePart::Placeholder(name) => Some(*name),
                I18nTemplatePart::Text(_) | I18nTemplatePart::LiteralBrace(_) => None,
            })
            .collect()
    };
    actual == expected
        && previous.values().iter().all(|(placeholder, dictionary)| {
            expected.contains(placeholder.as_str())
                && (dictionary.is_empty() || dictionaries.contains_key(dictionary))
        })
}

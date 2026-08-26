//! 从叙事 HIR 提取宿主无关的可翻译文本目录。

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

mod catalog;
mod diagnostics;
mod export;
mod json;
mod language;
mod manifest;
mod message;
mod package;
mod resolution;
mod template;
mod validation;

use catalog::collect_body;

pub use diagnostics::I18nDiagnostic;
pub use export::{I18nExport, I18nExportError, I18nExportObsolete, I18nExportObsoleteReason};
pub use json::{I18nJsonError, I18nJsonErrorKind};
pub use language::{
    I18nLanguageChain, I18nLanguageChainError, I18nLanguageLayer, I18nRuntimeLanguage,
};
pub use manifest::{NlangInstallError, NlangManifest, NlangManifestError, NlangValidatedManifest};
pub use message::I18nMessageError;
pub use package::{
    NlangPackageEntry, NlangPackageError, NlangPackageInput, NlangPackageOutput,
    NlangPackageOutputError, NlangValidatedPackage,
};
pub use resolution::{I18nResolveError, I18nResolvedText, I18nTextOrigin};
pub use validation::is_language_tag_well_formed;
pub use validation::{I18nValidatedTemplate, I18nValidationError};

use crate::{
    expression::{Expression, ExpressionKind, VariableScope},
    hir::{HirBodyKind, HirBodyNode, HirPrint, HirStory},
    twee::Span,
};

/// 编译结构决定的文本身份，不使用原文或源码行号作为键。
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct I18nTextId(String);

/// 一次目录构建的 Core 内部身份；克隆保留身份，重新构建取得新身份。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct I18nCatalogIdentity(u64);

static NEXT_I18N_CATALOG_ID: AtomicU64 = AtomicU64::new(1);

impl I18nCatalogIdentity {
    /// 原子分配下一个目录身份。
    fn next() -> Self {
        let id: u64 = NEXT_I18N_CATALOG_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current: u64| {
                current.checked_add(1)
            })
            .expect("I18nCatalog 身份空间不应耗尽");
        Self(id)
    }
}

impl I18nTextId {
    /// 原始字符串形式的文本身份。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 默认语言的一条文本记录；目标语言只能替换 `text` 对应的译文。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct I18nMessage {
    id: I18nTextId,
    source: String,
    passage: String,
    text: String,
    placeholders: Vec<I18nPlaceholder>,
    span: Span,
}

/// 一条动态值在消息模板中的受控名称及其 HIR 来源。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct I18nPlaceholder {
    name: String,
    node_path: String,
}

impl I18nPlaceholder {
    /// 模板中使用的受控占位名称。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 该动态值的 HIR 来源路径。
    pub fn node_path(&self) -> &str {
        &self.node_path
    }
}

impl I18nMessage {
    /// 编译结构决定的文本身份。
    pub fn id(&self) -> &I18nTextId {
        &self.id
    }

    /// 消息所属的源码文件。
    pub fn source(&self) -> &str {
        &self.source
    }

    /// 消息所属的 Passage 名称。
    pub fn passage(&self) -> &str {
        &self.passage
    }

    /// 默认语言原文。
    pub fn text(&self) -> &str {
        &self.text
    }

    /// 模板中受控的动态值占位符。
    pub fn placeholders(&self) -> &[I18nPlaceholder] {
        &self.placeholders
    }

    /// 消息在源码中的位置。
    pub fn span(&self) -> Span {
        self.span
    }
}

/// 编译产物携带的默认语言文本目录。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct I18nCatalog {
    #[serde(skip, default = "I18nCatalogIdentity::next")]
    identity: I18nCatalogIdentity,
    messages: Vec<I18nMessage>,
}

impl Default for I18nCatalog {
    fn default() -> Self {
        Self {
            identity: I18nCatalogIdentity::next(),
            messages: Vec::new(),
        }
    }
}

impl I18nCatalog {
    /// 按 Passage 与 HIR 结构顺序收集可见的静态文本。
    pub fn from_hir(story: &HirStory<'_>) -> Self {
        let mut messages: Vec<I18nMessage> = Vec::new();

        for passage in &story.passages {
            collect_body(
                passage.source.as_str(),
                passage.name,
                "body",
                &passage.body,
                &mut messages,
            );
        }

        Self {
            identity: I18nCatalogIdentity::next(),
            messages,
        }
    }

    /// 按收集顺序返回全部默认语言消息。
    pub fn messages(&self) -> &[I18nMessage] {
        &self.messages
    }

    /// 建立可导出为 NMSG 与动态字典 JSON 的译者模板。
    pub fn template(&self, language: impl Into<String>) -> I18nTemplate {
        let passages: BTreeMap<String, I18nTemplateMessage> = self
            .messages
            .iter()
            .map(|message: &I18nMessage| {
                let values: BTreeMap<String, String> = message
                    .placeholders
                    .iter()
                    .map(|placeholder: &I18nPlaceholder| (placeholder.name.clone(), String::new()))
                    .collect();
                (
                    message.id.0.clone(),
                    I18nTemplateMessage {
                        source: message.text.trim_end_matches('\n').to_owned(),
                        text: String::new(),
                        values,
                    },
                )
            })
            .collect();
        I18nTemplate {
            language: language.into(),
            dictionary: BTreeMap::new(),
            passages,
        }
    }

    /// 生成新模板，或在不覆盖有效译文的前提下合并上次导出结果。
    pub fn export(
        &self,
        language: impl Into<String>,
        previous: Option<&I18nTemplate>,
    ) -> Result<I18nExport, I18nExportError> {
        export::export(self, language.into(), previous)
    }

    /// 在译文进入 Runtime 或有效构建前完成一次边界校验。
    pub fn validate(
        &self,
        template: I18nTemplate,
    ) -> Result<I18nValidatedTemplate, Vec<I18nValidationError>> {
        validation::validate(self, template)
    }

    /// 判断译文是否由当前这次目录构建校验，而非来自另一份编译结果。
    pub fn accepts(&self, translation: &I18nValidatedTemplate) -> bool {
        translation.belongs_to(self)
    }

    /// 使用已验证译文解析一条消息；目标语言缺失时回退默认文本。
    pub fn resolve(
        &self,
        translation: &I18nValidatedTemplate,
        id: &str,
        values: &BTreeMap<String, String>,
    ) -> Result<I18nResolvedText, I18nResolveError> {
        resolution::resolve(self, translation, id, values)
    }

    /// VM 保留原始 Value 类型时使用；只有集合中的 placeholder 可以查询动态字典。
    pub(crate) fn resolve_runtime(
        &self,
        translation: &I18nValidatedTemplate,
        id: &str,
        values: &BTreeMap<String, String>,
        dictionary_values: &std::collections::BTreeSet<String>,
    ) -> Result<I18nResolvedText, I18nResolveError> {
        resolution::resolve_runtime(self, translation, id, values, dictionary_values)
    }

    /// 不加载目标语言时，使用同一模板规则解析默认文本。
    pub fn resolve_default(
        &self,
        id: &str,
        values: &BTreeMap<String, String>,
    ) -> Result<I18nResolvedText, I18nResolveError> {
        resolution::resolve_default(self, id, values)
    }
}

/// Core 内部统一使用的翻译数据，不对应单个磁盘文件。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct I18nTemplate {
    language: String,
    dictionary: BTreeMap<String, BTreeMap<String, String>>,
    passages: BTreeMap<String, I18nTemplateMessage>,
}

impl I18nTemplate {
    /// 用目标语言、动态字典与消息表直接构造模板。
    pub fn new(
        language: impl Into<String>,
        dictionary: BTreeMap<String, BTreeMap<String, String>>,
        passages: BTreeMap<String, I18nTemplateMessage>,
    ) -> Self {
        Self {
            language: language.into(),
            dictionary,
            passages,
        }
    }

    /// 目标语言标签。
    pub fn language(&self) -> &str {
        &self.language
    }

    /// 动态字典：placeholder 名到（键 → 译文）的映射。
    pub fn dictionary(&self) -> &BTreeMap<String, BTreeMap<String, String>> {
        &self.dictionary
    }

    /// 消息表：文本身份到模板消息的映射。
    pub fn passages(&self) -> &BTreeMap<String, I18nTemplateMessage> {
        &self.passages
    }

    /// 生成 `.nlang` 使用的紧凑消息块。
    pub fn to_nmsg(&self) -> String {
        message::encode(self)
    }

    /// 把 NMSG 中的译文合回当前模板；未出现的消息保持原值。
    pub fn apply_nmsg(&self, input: &str) -> Result<Self, I18nMessageError> {
        message::apply(self, input)
    }
}

/// 译者可修改的消息文本和动态值字典绑定。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct I18nTemplateMessage {
    source: String,
    text: String,
    values: BTreeMap<String, String>,
}

impl I18nTemplateMessage {
    /// 用原文、译文与动态值绑定直接构造消息。
    pub fn new(
        source: impl Into<String>,
        text: impl Into<String>,
        values: BTreeMap<String, String>,
    ) -> Self {
        Self {
            source: source.into(),
            text: text.into(),
            values,
        }
    }

    /// 默认语言原文，供译文对照。
    pub fn source(&self) -> &str {
        &self.source
    }

    /// 目标语言译文。
    pub fn text(&self) -> &str {
        &self.text
    }

    /// 该消息的动态值绑定。
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

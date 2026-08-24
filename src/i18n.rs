//! 从叙事 HIR 提取宿主无关的可翻译文本目录。

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

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
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn node_path(&self) -> &str {
        &self.node_path
    }
}

impl I18nMessage {
    pub fn id(&self) -> &I18nTextId {
        &self.id
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn passage(&self) -> &str {
        &self.passage
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn placeholders(&self) -> &[I18nPlaceholder] {
        &self.placeholders
    }

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

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn dictionary(&self) -> &BTreeMap<String, BTreeMap<String, String>> {
        &self.dictionary
    }

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

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

fn collect_body(
    source: &str,
    passage: &str,
    path: &str,
    body: &[HirBodyNode<'_>],
    messages: &mut Vec<I18nMessage>,
) {
    let mut visible: Option<VisibleMessage> = None;

    for (index, node) in body.iter().enumerate() {
        let node_path: String = format!("{path}.{index}");
        match &node.kind {
            HirBodyKind::Text(text) => visible
                .get_or_insert_with(|| VisibleMessage::new(index, node.span))
                .push_text(text, node.span),
            HirBodyKind::Print(HirPrint::Literal(text)) => visible
                .get_or_insert_with(|| VisibleMessage::new(index, node.span))
                .push_text(text, node.span),
            HirBodyKind::Print(HirPrint::Expression(expression)) => visible
                .get_or_insert_with(|| VisibleMessage::new(index, node.span))
                .push_expression(expression, node_path, node.span),
            // silently 的正文不会进入 Presentation，因此不属于翻译目录。
            HirBodyKind::Silently(_) => {
                flush_visible(source, passage, path, &mut visible, messages)
            }
            HirBodyKind::If(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                for (branch_index, branch) in value.branches.iter().enumerate() {
                    collect_body(
                        source,
                        passage,
                        &format!("{node_path}.branch.{branch_index}"),
                        &branch.body,
                        messages,
                    );
                }
                if let Some(fallback) = &value.fallback {
                    collect_body(
                        source,
                        passage,
                        &format!("{node_path}.fallback"),
                        fallback,
                        messages,
                    );
                }
            }
            HirBodyKind::Switch(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                for (case_index, case) in value.cases.iter().enumerate() {
                    collect_body(
                        source,
                        passage,
                        &format!("{node_path}.case.{case_index}"),
                        &case.body,
                        messages,
                    );
                }
                if let Some(default) = &value.default {
                    collect_body(
                        source,
                        passage,
                        &format!("{node_path}.default"),
                        default,
                        messages,
                    );
                }
            }
            HirBodyKind::For(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                collect_body(
                    source,
                    passage,
                    &format!("{node_path}.body"),
                    &value.body,
                    messages,
                );
            }
            HirBodyKind::While(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                collect_body(
                    source,
                    passage,
                    &format!("{node_path}.body"),
                    &value.body,
                    messages,
                );
            }
            HirBodyKind::Widget(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                collect_body(
                    source,
                    passage,
                    &format!("{node_path}.body"),
                    &value.body,
                    messages,
                );
            }
            HirBodyKind::Capture(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                collect_body(
                    source,
                    passage,
                    &format!("{node_path}.body"),
                    &value.body,
                    messages,
                );
            }
            HirBodyKind::Macro(value) => {
                flush_visible(source, passage, path, &mut visible, messages);
                collect_body(
                    source,
                    passage,
                    &format!("{node_path}.body"),
                    &value.body,
                    messages,
                );
            }
            HirBodyKind::Break
            | HirBodyKind::Continue
            | HirBodyKind::Exit
            | HirBodyKind::Set(_)
            | HirBodyKind::Unset(_)
            | HirBodyKind::Run(_)
            | HirBodyKind::Include(_)
            | HirBodyKind::Goto(_)
            | HirBodyKind::Return(_) => {
                flush_visible(source, passage, path, &mut visible, messages);
            }
        }
    }

    flush_visible(source, passage, path, &mut visible, messages);
}

struct VisibleMessage {
    start_index: usize,
    text: String,
    placeholders: Vec<I18nPlaceholder>,
    span: Span,
    has_static_text: bool,
}

impl VisibleMessage {
    fn new(start_index: usize, span: Span) -> Self {
        Self {
            start_index,
            text: String::new(),
            placeholders: Vec::new(),
            span,
            has_static_text: false,
        }
    }

    fn push_text(&mut self, text: &str, span: Span) {
        // 模板使用单花括号标记 placeholder，源码花括号必须先转义。
        for character in text.chars() {
            match character {
                '{' => self.text.push_str("{{"),
                '}' => self.text.push_str("}}"),
                _ => self.text.push(character),
            }
        }
        self.span.end = span.end;
        self.has_static_text |= !text.trim().is_empty();
    }

    fn push_expression(&mut self, expression: &Expression<'_>, node_path: String, span: Span) {
        let ordinal: usize = self.placeholders.len() + 1;
        let name: String = placeholder_name(expression, ordinal);
        self.text.push('{');
        self.text.push_str(&name);
        self.text.push('}');
        self.placeholders.push(I18nPlaceholder { name, node_path });
        self.span.end = span.end;
    }
}

fn flush_visible(
    source: &str,
    passage: &str,
    path: &str,
    visible: &mut Option<VisibleMessage>,
    messages: &mut Vec<I18nMessage>,
) {
    let Some(visible) = visible.take() else {
        return;
    };
    // 只有动态值而没有可翻译文字时，不生成空洞的翻译条目。
    if !visible.has_static_text {
        return;
    }
    let node_path: String = format!("{path}.{}", visible.start_index);
    messages.push(I18nMessage {
        id: I18nTextId(format!("p{}:{passage}:{node_path}", passage.len())),
        source: source.to_owned(),
        passage: passage.to_owned(),
        text: visible.text,
        placeholders: visible.placeholders,
        span: visible.span,
    });
}

fn placeholder_name(expression: &Expression<'_>, ordinal: usize) -> String {
    match &expression.kind {
        ExpressionKind::Variable { scope, name } => {
            let prefix: char = match scope {
                VariableScope::Variables => '$',
                VariableScope::Temporary => '_',
                VariableScope::Local => '@',
            };
            format!("{prefix}{name}")
        }
        ExpressionKind::Global(name) => (*name).to_owned(),
        ExpressionKind::Setup => String::from("setup"),
        ExpressionKind::Group(inner) => placeholder_name(inner, ordinal),
        _ => format!("value_{ordinal}"),
    }
}

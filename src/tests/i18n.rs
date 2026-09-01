//! I18n 文本目录的稳定身份与来源测试。

use std::{collections::BTreeMap, path::Path, sync::Arc};

use crate::{
    GameIdentity,
    diagnostic::{Diagnostic, DiagnosticSeverity},
    expression::parse,
    hir::{HirBodyKind, HirBodyNode, HirPassage, HirPrint, HirStory},
    i18n::{
        I18nCatalog, I18nDiagnostic, I18nExportError, I18nExportObsoleteReason, I18nJsonErrorKind,
        I18nLanguageChain, I18nLanguageChainError, I18nMessageError, I18nResolveError,
        I18nRuntimeLanguage, I18nTemplate, I18nTemplateMessage, I18nTextOrigin,
        I18nValidationError, NlangInstallError, NlangManifest, NlangManifestError,
        NlangPackageEntry, NlangPackageError, NlangPackageInput, NlangPackageOutput,
        NlangPackageOutputError, NlangValidatedPackage,
    },
    source::Source,
    twee::Span,
};

// 大型测试集按用例边界拆成物理分片；门面保留共享导入与模块说明。
// include 保持原模块命名、私有辅助项可见性和测试发现路径不变。
include!("i18n/part_01.rs");
include!("i18n/part_02.rs");

#[test]
fn i18n_runtime_index_does_not_duplicate_message_id_strings() {
    let source: &str = include_str!("../i18n.rs");

    assert!(source.contains("message_index: RefCell<Option<HashMap<u64, usize>>>"));
    assert!(!source.contains("message_index: HashMap<String"));
}

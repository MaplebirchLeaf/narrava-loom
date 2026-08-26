//! I18n 公共错误到 Host／Logger 稳定诊断的统一边界。

use crate::diagnostic::{Diagnostic, DiagnosticSeverity};

use super::{
    I18nExportError, I18nJsonError, I18nLanguageChainError, I18nMessageError, I18nResolveError,
    I18nValidationError, NlangInstallError, NlangManifestError, NlangPackageError,
    NlangPackageOutputError,
};

/// 把 I18n 领域错误转换为稳定编号的公共 Diagnostic。
pub trait I18nDiagnostic {
    fn diagnostic(&self) -> Diagnostic;
}

/// 构造 Error 级 Diagnostic 的简写。
fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(code, DiagnosticSeverity::Error, &message.into())
}

impl I18nDiagnostic for I18nValidationError {
    fn diagnostic(&self) -> Diagnostic {
        let (code, message): (&str, String) = match self {
            Self::InvalidLanguageTag { language } => (
                "i18n.validation.invalid_language_tag",
                format!("语言标签无效: {language}"),
            ),
            Self::UnknownMessage { id } => (
                "i18n.validation.unknown_message",
                format!("译文包含未知消息: {id}"),
            ),
            Self::SourceMismatch { id } => (
                "i18n.validation.source_mismatch",
                format!("消息原文已被修改: {id}"),
            ),
            Self::InvalidPlaceholderSyntax { id } => (
                "i18n.validation.invalid_placeholder_syntax",
                format!("消息 placeholder 语法无效: {id}"),
            ),
            Self::MissingPlaceholder { id, name } => (
                "i18n.validation.missing_placeholder",
                format!("消息 {id} 缺少 placeholder {name}"),
            ),
            Self::UnknownPlaceholder { id, name } => (
                "i18n.validation.unknown_placeholder",
                format!("消息 {id} 含未知 placeholder {name}"),
            ),
            Self::UnknownDictionary {
                id,
                placeholder,
                dictionary,
            } => (
                "i18n.validation.unknown_dictionary",
                format!("消息 {id} 的 {placeholder} 引用了未知字典 {dictionary}"),
            ),
        };
        error(code, message)
    }
}

impl I18nDiagnostic for I18nResolveError {
    fn diagnostic(&self) -> Diagnostic {
        let code: &str = match self {
            Self::DifferentCatalog => "i18n.resolve.different_catalog",
            Self::UnknownMessage { .. } => "i18n.resolve.unknown_message",
            Self::MissingValue { .. } => "i18n.resolve.missing_value",
            Self::InvalidTemplate { .. } => "i18n.resolve.invalid_template",
        };
        error(code, format!("I18n 文本解析失败: {self:?}"))
    }
}

impl I18nDiagnostic for I18nLanguageChainError {
    fn diagnostic(&self) -> Diagnostic {
        let code: &str = match self {
            Self::InvalidDefaultLanguage { .. } => "i18n.language.invalid_default",
            Self::InvalidPrimaryLanguage { .. } => "i18n.language.invalid_primary",
            Self::MissingPrimaryLanguage => "i18n.language.missing_primary",
            Self::DuplicateLocale { .. } => "i18n.language.duplicate_locale",
            Self::MissingLanguagePackage { .. } => "i18n.language.missing_package",
            Self::FallbackCycle { .. } => "i18n.language.fallback_cycle",
            Self::InvalidTranslation { .. } => "i18n.language.invalid_translation",
            Self::FallbackOrderMismatch { .. } => "i18n.language.fallback_order",
            Self::MissingFallbackPackage { .. } => "i18n.language.missing_fallback",
        };
        error(code, self.to_string())
    }
}

impl I18nDiagnostic for I18nMessageError {
    fn diagnostic(&self) -> Diagnostic {
        let code: &str = match self {
            Self::InvalidSyntax { .. } => "i18n.nmsg.invalid_syntax",
            Self::UnknownMessage { .. } => "i18n.nmsg.unknown_message",
            Self::DuplicateMessage { .. } => "i18n.nmsg.duplicate_message",
            Self::SourceMismatch { .. } => "i18n.nmsg.source_mismatch",
            Self::DuplicateValue { .. } => "i18n.nmsg.duplicate_value",
        };
        error(code, self.to_string())
    }
}

impl I18nDiagnostic for I18nJsonError {
    fn diagnostic(&self) -> Diagnostic {
        error("i18n.data.invalid_json", self.to_string())
    }
}

impl I18nDiagnostic for I18nExportError {
    fn diagnostic(&self) -> Diagnostic {
        error("i18n.export.language_mismatch", self.to_string())
    }
}

impl I18nDiagnostic for NlangManifestError {
    fn diagnostic(&self) -> Diagnostic {
        error("i18n.manifest.invalid", self.to_string())
    }
}

impl I18nDiagnostic for NlangInstallError {
    fn diagnostic(&self) -> Diagnostic {
        error("i18n.install.incompatible", self.to_string())
    }
}

impl I18nDiagnostic for NlangPackageError {
    fn diagnostic(&self) -> Diagnostic {
        let code: &str = match self {
            Self::InvalidPath { .. } => "i18n.package.invalid_path",
            Self::ForbiddenPath { .. } => "i18n.package.forbidden_path",
            Self::DuplicatePath { .. } => "i18n.package.duplicate_path",
            Self::MissingFile { .. } => "i18n.package.missing_file",
            Self::InvalidUtf8 { .. } => "i18n.package.invalid_utf8",
            Self::Manifest(_) => "i18n.package.invalid_manifest",
            Self::Translation(_) => "i18n.package.invalid_translation",
            Self::Dictionary { .. } => "i18n.package.invalid_dictionary",
            Self::Install(_) => "i18n.package.incompatible",
        };
        error(code, self.to_string())
    }
}

impl I18nDiagnostic for NlangPackageOutputError {
    fn diagnostic(&self) -> Diagnostic {
        let code: &str = match self {
            Self::LocaleMismatch { .. } => "i18n.output.locale_mismatch",
            Self::Json(_) => "i18n.output.json",
        };
        error(code, self.to_string())
    }
}

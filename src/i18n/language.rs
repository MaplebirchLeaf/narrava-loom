//! 目标语言、显式 fallback 与默认原文之间的确定性解析链。

use std::{collections::BTreeMap, collections::BTreeSet, fmt};

use super::{
    I18nCatalog, I18nResolveError, I18nResolvedText, I18nTextOrigin, I18nValidatedTemplate,
    I18nValidationError, NlangValidatedPackage, is_language_tag_well_formed,
};

/// 一层已经绑定当前 I18n 目录的语言包。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I18nLanguageLayer {
    package: NlangValidatedPackage,
    translation: I18nValidatedTemplate,
}

impl I18nLanguageLayer {
    pub fn locale(&self) -> &str {
        self.translation.language()
    }

    pub fn package(&self) -> &NlangValidatedPackage {
        &self.package
    }

    pub fn translation(&self) -> &I18nValidatedTemplate {
        &self.translation
    }
}

/// 从首选目标语言依次走向默认原文的已验证回退链。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I18nLanguageChain {
    default_language: String,
    layers: Vec<I18nLanguageLayer>,
}

/// Engine 在一次事务中持有的、已经绑定编译目录的语言选择。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum I18nRuntimeLanguage {
    Translation(I18nValidatedTemplate),
    Chain(I18nLanguageChain),
}

impl I18nRuntimeLanguage {
    /// 为 Host 启动选择语言；选择游戏默认语言时无需建立目标语言对象。
    pub fn select(
        catalog: &I18nCatalog,
        default_language: &str,
        primary_language: &str,
        packages: Vec<NlangValidatedPackage>,
    ) -> Result<Option<Self>, I18nLanguageChainError> {
        if !is_language_tag_well_formed(default_language) {
            return Err(I18nLanguageChainError::InvalidDefaultLanguage {
                language: default_language.to_owned(),
            });
        }
        if !is_language_tag_well_formed(primary_language) {
            return Err(I18nLanguageChainError::InvalidPrimaryLanguage {
                language: primary_language.to_owned(),
            });
        }
        if primary_language == default_language {
            return Ok(None);
        }
        I18nLanguageChain::select(catalog, default_language, primary_language, packages)
            .map(Self::Chain)
            .map(Some)
    }

    pub fn primary_language(&self) -> &str {
        match self {
            Self::Translation(translation) => translation.language(),
            Self::Chain(chain) => chain.primary_language(),
        }
    }

    pub(crate) fn is_for(&self, catalog: &I18nCatalog) -> bool {
        match self {
            Self::Translation(translation) => catalog.accepts(translation),
            Self::Chain(chain) => chain.is_for(catalog),
        }
    }

    pub(crate) fn resolve(
        &self,
        catalog: &I18nCatalog,
        id: &str,
        values: &BTreeMap<String, String>,
        dictionary_values: &BTreeSet<String>,
    ) -> Result<I18nResolvedText, I18nResolveError> {
        match self {
            Self::Translation(translation) => {
                catalog.resolve_runtime(translation, id, values, dictionary_values)
            }
            Self::Chain(chain) => chain.resolve_runtime(catalog, id, values, dictionary_values),
        }
    }
}

impl I18nLanguageChain {
    /// 从无序安装包中按 manifest 声明自动选择首选语言与 fallback。
    pub fn select(
        catalog: &I18nCatalog,
        default_language: &str,
        primary_language: &str,
        packages: Vec<NlangValidatedPackage>,
    ) -> Result<Self, I18nLanguageChainError> {
        if !is_language_tag_well_formed(primary_language) {
            return Err(I18nLanguageChainError::InvalidPrimaryLanguage {
                language: primary_language.to_owned(),
            });
        }
        let mut available: BTreeMap<String, NlangValidatedPackage> = BTreeMap::new();
        for package in packages {
            let locale: String = package.manifest().manifest().locale().to_owned();
            if available.insert(locale.clone(), package).is_some() {
                return Err(I18nLanguageChainError::DuplicateLocale { locale });
            }
        }

        let mut selected: Vec<NlangValidatedPackage> = Vec::new();
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut locale: String = primary_language.to_owned();
        while locale != default_language {
            if !visited.insert(locale.clone()) {
                return Err(I18nLanguageChainError::FallbackCycle { locale });
            }
            let package: NlangValidatedPackage = available.remove(&locale).ok_or_else(|| {
                I18nLanguageChainError::MissingLanguagePackage {
                    locale: locale.clone(),
                }
            })?;
            let fallback: Option<String> =
                package.manifest().manifest().fallback().map(str::to_owned);
            selected.push(package);
            let Some(fallback) = fallback else {
                break;
            };
            locale = fallback;
        }
        Self::validate(catalog, default_language, selected)
    }

    pub fn validate(
        catalog: &I18nCatalog,
        default_language: &str,
        packages: Vec<NlangValidatedPackage>,
    ) -> Result<Self, I18nLanguageChainError> {
        if !is_language_tag_well_formed(default_language) {
            return Err(I18nLanguageChainError::InvalidDefaultLanguage {
                language: default_language.to_owned(),
            });
        }
        if packages.is_empty() {
            return Err(I18nLanguageChainError::MissingPrimaryLanguage);
        }

        let mut locales: BTreeSet<String> = BTreeSet::new();
        let mut layers: Vec<I18nLanguageLayer> = Vec::with_capacity(packages.len());
        for package in packages {
            let locale: String = package.manifest().manifest().locale().to_owned();
            if !locales.insert(locale.clone()) {
                return Err(I18nLanguageChainError::DuplicateLocale { locale });
            }
            let translation: I18nValidatedTemplate =
                catalog.validate(package.translation().clone()).map_err(
                    |errors: Vec<I18nValidationError>| I18nLanguageChainError::InvalidTranslation {
                        locale: locale.clone(),
                        errors,
                    },
                )?;
            layers.push(I18nLanguageLayer {
                package,
                translation,
            });
        }

        validate_fallback_order(&layers, default_language)?;
        Ok(Self {
            default_language: default_language.to_owned(),
            layers,
        })
    }

    pub fn primary_language(&self) -> &str {
        self.layers[0].locale()
    }

    pub fn default_language(&self) -> &str {
        &self.default_language
    }

    pub fn layers(&self) -> &[I18nLanguageLayer] {
        &self.layers
    }

    /// 整条链必须由同一次目录构建校验，不能跨 Story 重建复用。
    pub fn is_for(&self, catalog: &I18nCatalog) -> bool {
        self.layers
            .iter()
            .all(|layer: &I18nLanguageLayer| catalog.accepts(layer.translation()))
    }

    pub fn resolve(
        &self,
        catalog: &I18nCatalog,
        id: &str,
        values: &BTreeMap<String, String>,
    ) -> Result<I18nResolvedText, I18nResolveError> {
        for layer in &self.layers {
            let resolved: I18nResolvedText = catalog.resolve(layer.translation(), id, values)?;
            if resolved.origin() == I18nTextOrigin::Translation {
                return Ok(resolved);
            }
        }
        catalog.resolve_default(id, values)
    }

    pub(crate) fn resolve_runtime(
        &self,
        catalog: &I18nCatalog,
        id: &str,
        values: &BTreeMap<String, String>,
        dictionary_values: &BTreeSet<String>,
    ) -> Result<I18nResolvedText, I18nResolveError> {
        for layer in &self.layers {
            let resolved: I18nResolvedText =
                catalog.resolve_runtime(layer.translation(), id, values, dictionary_values)?;
            if resolved.origin() == I18nTextOrigin::Translation {
                return Ok(resolved);
            }
        }
        catalog.resolve_default(id, values)
    }
}

fn validate_fallback_order(
    layers: &[I18nLanguageLayer],
    default_language: &str,
) -> Result<(), I18nLanguageChainError> {
    for (index, layer) in layers.iter().enumerate() {
        let manifest = layer.package().manifest().manifest();
        let next: Option<&str> = layers.get(index + 1).map(I18nLanguageLayer::locale);
        match (manifest.fallback(), next) {
            (Some(fallback), Some(next_locale)) if fallback != next_locale => {
                return Err(I18nLanguageChainError::FallbackOrderMismatch {
                    locale: layer.locale().to_owned(),
                    declared: Some(fallback.to_owned()),
                    next: next_locale.to_owned(),
                });
            }
            (None, Some(next_locale)) => {
                return Err(I18nLanguageChainError::FallbackOrderMismatch {
                    locale: layer.locale().to_owned(),
                    declared: None,
                    next: next_locale.to_owned(),
                });
            }
            (Some(fallback), None) if fallback != default_language => {
                return Err(I18nLanguageChainError::MissingFallbackPackage {
                    locale: layer.locale().to_owned(),
                    fallback: fallback.to_owned(),
                });
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum I18nLanguageChainError {
    InvalidDefaultLanguage {
        language: String,
    },
    InvalidPrimaryLanguage {
        language: String,
    },
    MissingPrimaryLanguage,
    DuplicateLocale {
        locale: String,
    },
    MissingLanguagePackage {
        locale: String,
    },
    FallbackCycle {
        locale: String,
    },
    InvalidTranslation {
        locale: String,
        errors: Vec<I18nValidationError>,
    },
    FallbackOrderMismatch {
        locale: String,
        declared: Option<String>,
        next: String,
    },
    MissingFallbackPackage {
        locale: String,
        fallback: String,
    },
}

impl fmt::Display for I18nLanguageChainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDefaultLanguage { language } => {
                write!(formatter, "默认语言标签无效: {language}")
            }
            Self::InvalidPrimaryLanguage { language } => {
                write!(formatter, "首选语言标签无效: {language}")
            }
            Self::MissingPrimaryLanguage => write!(formatter, "语言回退链缺少首选语言包"),
            Self::DuplicateLocale { locale } => write!(formatter, "语言回退链重复: {locale}"),
            Self::MissingLanguagePackage { locale } => write!(formatter, "缺少语言包: {locale}"),
            Self::FallbackCycle { locale } => write!(formatter, "语言 fallback 形成循环: {locale}"),
            Self::InvalidTranslation { locale, errors } => {
                write!(formatter, "语言 {locale} 包含 {} 项无效译文", errors.len())
            }
            Self::FallbackOrderMismatch {
                locale,
                declared,
                next,
            } => write!(
                formatter,
                "语言 {locale} 声明的 fallback {declared:?} 与下一层 {next} 不一致"
            ),
            Self::MissingFallbackPackage { locale, fallback } => {
                write!(formatter, "语言 {locale} 缺少声明的 fallback 包 {fallback}")
            }
        }
    }
}

impl std::error::Error for I18nLanguageChainError {}

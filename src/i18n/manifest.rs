//! `.nlang` 的最小 manifest 与安装前语义校验。

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{GameCompatibility, GameCompatibilityError, GameIdentity};

use super::{I18nJsonError, validation::is_language_tag_well_formed};

/// 已解析且字段自身有效的语言包声明。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NlangManifest {
    locale: String,
    fallback: Option<String>,
    version: semver::Version,
    game: GameCompatibility,
}

/// JSON 形状正确后仍可能出现的 manifest 语义错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NlangManifestError {
    Json(I18nJsonError),
    InvalidLocale { locale: String },
    InvalidFallback { fallback: String },
    InvalidPackageVersion { message: String },
    InvalidGameTarget { source: GameCompatibilityError },
}

/// 与外部文件身份及当前游戏完成配对后的 manifest。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NlangValidatedManifest {
    manifest: NlangManifest,
}

/// 安装上下文与 manifest 不一致；失败不会产生部分有效状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NlangInstallError {
    LocaleMismatch {
        file: String,
        manifest: String,
    },
    IncompatibleGame {
        required_id: String,
        required_versions: String,
        actual_id: String,
        actual_version: String,
    },
}

/// manifest.json 的直接 JSON 映射。
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawNlangManifest {
    locale: String,
    #[serde(default)]
    fallback: Option<String>,
    version: String,
    game: RawNlangGame,
}

/// manifest 中 `game` 字段的直接 JSON 映射。
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawNlangGame {
    id: String,
    versions: String,
}

impl NlangManifest {
    /// 只解析 `manifest.json` 文本，不读取 ZIP 或文件路径。
    pub fn from_json(json: &str) -> Result<Self, NlangManifestError> {
        let raw: RawNlangManifest =
            serde_json::from_str(json).map_err(|error: serde_json::Error| {
                NlangManifestError::Json(I18nJsonError::decode(error))
            })?;
        if !is_language_tag_well_formed(&raw.locale) {
            return Err(NlangManifestError::InvalidLocale { locale: raw.locale });
        }
        if let Some(fallback) = &raw.fallback
            && !is_language_tag_well_formed(fallback)
        {
            return Err(NlangManifestError::InvalidFallback {
                fallback: fallback.clone(),
            });
        }

        let version: semver::Version =
            semver::Version::parse(&raw.version).map_err(|error: semver::Error| {
                NlangManifestError::InvalidPackageVersion {
                    message: error.to_string(),
                }
            })?;
        let game: GameCompatibility = GameCompatibility::new(raw.game.id, &raw.game.versions)
            .map_err(
                |source: GameCompatibilityError| NlangManifestError::InvalidGameTarget { source },
            )?;

        Ok(Self {
            locale: raw.locale,
            fallback: raw.fallback,
            version,
            game,
        })
    }

    /// 生成供 Binding 写入 `.nlang` 的 manifest 文本。
    pub fn to_json_pretty(&self) -> Result<String, I18nJsonError> {
        let raw = RawNlangManifest {
            locale: self.locale.clone(),
            fallback: self.fallback.clone(),
            version: self.version.to_string(),
            game: RawNlangGame {
                id: self.game.id().to_owned(),
                versions: self.game.versions().to_string(),
            },
        };
        serde_json::to_string_pretty(&raw).map_err(I18nJsonError::encode)
    }

    /// 语言包的主语言标签。
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// 可选的回退语言标签。
    pub fn fallback(&self) -> Option<&str> {
        self.fallback.as_deref()
    }

    /// 语言包版本。
    pub fn version(&self) -> &semver::Version {
        &self.version
    }

    /// 目标游戏兼容性声明。
    pub fn game(&self) -> &GameCompatibility {
        &self.game
    }

    /// `file_locale` 由 Binding 从 `languages/<locale>.nlang` 文件名取得。
    pub fn validate_install(
        &self,
        file_locale: &str,
        game: &GameIdentity,
    ) -> Result<NlangValidatedManifest, NlangInstallError> {
        if file_locale != self.locale {
            return Err(NlangInstallError::LocaleMismatch {
                file: file_locale.to_owned(),
                manifest: self.locale.clone(),
            });
        }
        if !self.game.matches(game) {
            return Err(NlangInstallError::IncompatibleGame {
                required_id: self.game.id().to_owned(),
                required_versions: self.game.versions().to_string(),
                actual_id: game.id().to_owned(),
                actual_version: game.version().to_string(),
            });
        }

        Ok(NlangValidatedManifest {
            manifest: self.clone(),
        })
    }
}

impl NlangValidatedManifest {
    /// 已通过安装校验的 manifest。
    pub fn manifest(&self) -> &NlangManifest {
        &self.manifest
    }
}

impl fmt::Display for NlangManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "manifest JSON 无效: {error}"),
            Self::InvalidLocale { locale } => write!(formatter, "locale 无效: {locale}"),
            Self::InvalidFallback { fallback } => write!(formatter, "fallback 无效: {fallback}"),
            Self::InvalidPackageVersion { message } => {
                write!(formatter, "语言包版本无效: {message}")
            }
            Self::InvalidGameTarget { source } => write!(formatter, "目标游戏无效: {source}"),
        }
    }
}

impl std::error::Error for NlangManifestError {}

impl fmt::Display for NlangInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocaleMismatch { file, manifest } => {
                write!(
                    formatter,
                    "文件 locale {file} 与 manifest locale {manifest} 不一致"
                )
            }
            Self::IncompatibleGame {
                required_id,
                required_versions,
                actual_id,
                actual_version,
            } => write!(
                formatter,
                "语言包要求 {required_id} {required_versions}，当前游戏为 {actual_id} {actual_version}"
            ),
        }
    }
}

impl std::error::Error for NlangInstallError {}

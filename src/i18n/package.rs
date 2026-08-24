//! Binding 解包后交给 Core 的 `.nlang` 内存文件清单。

use std::{collections::BTreeMap, fmt, str};

use crate::GameIdentity;

use super::{
    I18nJsonError, I18nMessageError, I18nTemplate, NlangInstallError, NlangManifest,
    NlangManifestError, NlangValidatedManifest, message,
};

const MANIFEST_PATH: &str = "manifest.json";
const TRANSLATIONS_PATH: &str = "translations.nmsg";
const DICTIONARY_PATH: &str = "dictionary.json";

/// 已由 Binding 解压、但尚未信任的单个文件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NlangPackageEntry {
    path: String,
    bytes: Vec<u8>,
}

impl NlangPackageEntry {
    pub fn new(path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            bytes: bytes.into(),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// 不依赖具体 ZIP 实现的语言包输入。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NlangPackageInput {
    entries: Vec<NlangPackageEntry>,
}

/// Core 生成的确定性内存清单；Binding 只负责 ZIP 编码与落盘。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NlangPackageOutput {
    locale: String,
    entries: Vec<NlangPackageEntry>,
}

impl NlangPackageOutput {
    pub fn build(
        manifest: &NlangManifest,
        translation: &I18nTemplate,
    ) -> Result<Self, NlangPackageOutputError> {
        if manifest.locale() != translation.language() {
            return Err(NlangPackageOutputError::LocaleMismatch {
                manifest: manifest.locale().to_owned(),
                translation: translation.language().to_owned(),
            });
        }

        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        files.insert(
            String::from(MANIFEST_PATH),
            manifest
                .to_json_pretty()
                .map_err(NlangPackageOutputError::Json)?
                .into_bytes(),
        );
        files.insert(
            String::from(TRANSLATIONS_PATH),
            translation.to_nmsg().into_bytes(),
        );
        files.insert(
            String::from(DICTIONARY_PATH),
            serde_json::to_vec_pretty(translation.dictionary()).map_err(
                |error: serde_json::Error| {
                    NlangPackageOutputError::Json(I18nJsonError::encode(error))
                },
            )?,
        );

        Ok(Self {
            locale: manifest.locale().to_owned(),
            entries: files
                .into_iter()
                .map(|(path, bytes): (String, Vec<u8>)| NlangPackageEntry::new(path, bytes))
                .collect(),
        })
    }

    pub fn file_name(&self) -> String {
        format!("{}.nlang", self.locale)
    }

    pub fn entries(&self) -> &[NlangPackageEntry] {
        &self.entries
    }

    pub fn into_entries(self) -> Vec<NlangPackageEntry> {
        self.entries
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NlangPackageOutputError {
    LocaleMismatch {
        manifest: String,
        translation: String,
    },
    Json(I18nJsonError),
}

impl fmt::Display for NlangPackageOutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocaleMismatch {
                manifest,
                translation,
            } => write!(
                formatter,
                "manifest locale {manifest} 与译文 language {translation} 不一致"
            ),
            Self::Json(error) => write!(formatter, "语言包 JSON 生成失败: {error}"),
        }
    }
}

impl std::error::Error for NlangPackageOutputError {}

impl NlangPackageInput {
    pub fn new(entries: Vec<NlangPackageEntry>) -> Self {
        Self { entries }
    }

    /// 在产生验证结果前完成路径、必需文件、JSON 和安装上下文校验。
    pub fn validate(
        self,
        file_locale: &str,
        game: &GameIdentity,
    ) -> Result<NlangValidatedPackage, NlangPackageError> {
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for entry in self.entries {
            if !is_normalized_package_path(&entry.path) {
                return Err(NlangPackageError::InvalidPath { path: entry.path });
            }
            if !is_allowed_package_path(&entry.path) {
                return Err(NlangPackageError::ForbiddenPath { path: entry.path });
            }
            if files.insert(entry.path.clone(), entry.bytes).is_some() {
                return Err(NlangPackageError::DuplicatePath { path: entry.path });
            }
        }

        let manifest_bytes: &[u8] = required_file(&files, MANIFEST_PATH)?;
        let translation_bytes: &[u8] = required_file(&files, TRANSLATIONS_PATH)?;
        let dictionary_bytes: &[u8] = required_file(&files, DICTIONARY_PATH)?;
        let manifest_json: &str =
            str::from_utf8(manifest_bytes).map_err(|_| NlangPackageError::InvalidUtf8 {
                path: String::from(MANIFEST_PATH),
            })?;
        let translation_nmsg: &str =
            str::from_utf8(translation_bytes).map_err(|_| NlangPackageError::InvalidUtf8 {
                path: String::from(TRANSLATIONS_PATH),
            })?;
        let dictionary_json: &str =
            str::from_utf8(dictionary_bytes).map_err(|_| NlangPackageError::InvalidUtf8 {
                path: String::from(DICTIONARY_PATH),
            })?;

        let manifest: NlangManifest =
            NlangManifest::from_json(manifest_json).map_err(NlangPackageError::Manifest)?;
        let dictionary: BTreeMap<String, BTreeMap<String, String>> =
            serde_json::from_str(dictionary_json).map_err(|error: serde_json::Error| {
                NlangPackageError::Dictionary {
                    error: I18nJsonError::decode(error),
                }
            })?;
        let translation: I18nTemplate =
            message::decode(manifest.locale(), translation_nmsg, dictionary)
                .map_err(NlangPackageError::Translation)?;
        let manifest: NlangValidatedManifest = manifest
            .validate_install(file_locale, game)
            .map_err(NlangPackageError::Install)?;

        Ok(NlangValidatedPackage {
            manifest,
            translation,
            files,
        })
    }
}

/// 路径与包级语义全部通过，但译文尚未绑定具体 `I18nCatalog`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NlangValidatedPackage {
    manifest: NlangValidatedManifest,
    translation: I18nTemplate,
    files: BTreeMap<String, Vec<u8>>,
}

impl NlangValidatedPackage {
    pub fn manifest(&self) -> &NlangValidatedManifest {
        &self.manifest
    }

    pub fn translation(&self) -> &I18nTemplate {
        &self.translation
    }

    pub fn file(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NlangPackageError {
    InvalidPath { path: String },
    ForbiddenPath { path: String },
    DuplicatePath { path: String },
    MissingFile { path: String },
    InvalidUtf8 { path: String },
    Manifest(NlangManifestError),
    Translation(I18nMessageError),
    Dictionary { error: I18nJsonError },
    Install(NlangInstallError),
}

fn required_file<'a>(
    files: &'a BTreeMap<String, Vec<u8>>,
    path: &str,
) -> Result<&'a [u8], NlangPackageError> {
    files
        .get(path)
        .map(Vec::as_slice)
        .ok_or_else(|| NlangPackageError::MissingFile {
            path: path.to_owned(),
        })
}

fn is_normalized_package_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|segment: &str| !segment.is_empty() && segment != "." && segment != "..")
}

fn is_allowed_package_path(path: &str) -> bool {
    matches!(path, MANIFEST_PATH | TRANSLATIONS_PATH | DICTIONARY_PATH)
        || (path.starts_with("resources/") && path.ends_with(".nres"))
}

impl fmt::Display for NlangPackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath { path } => write!(formatter, "语言包路径不安全: {path}"),
            Self::ForbiddenPath { path } => write!(formatter, "语言包文件不被允许: {path}"),
            Self::DuplicatePath { path } => write!(formatter, "语言包文件重复: {path}"),
            Self::MissingFile { path } => write!(formatter, "语言包缺少必需文件: {path}"),
            Self::InvalidUtf8 { path } => write!(formatter, "语言包文本不是 UTF-8: {path}"),
            Self::Manifest(error) => write!(formatter, "manifest.json 无效: {error}"),
            Self::Translation(error) => write!(formatter, "translations.nmsg 无效: {error}"),
            Self::Dictionary { error } => write!(formatter, "dictionary.json 无效: {error}"),
            Self::Install(error) => write!(formatter, "语言包不适用于当前安装: {error}"),
        }
    }
}

impl std::error::Error for NlangPackageError {}

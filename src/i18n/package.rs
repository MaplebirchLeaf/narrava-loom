//! Binding 解包后交给 Core 的 `.nlang` 内存文件清单。

use std::{collections::BTreeMap, fmt, str};

use crate::GameIdentity;

use super::{
    I18nJsonError, I18nMessageError, I18nTemplate, NlangInstallError, NlangManifest,
    NlangManifestError, NlangValidatedManifest, message,
};

/// `.nlang` 内的 manifest 文件名。
const MANIFEST_PATH: &str = "manifest.json";
/// `.nlang` 内的译文文件名。
const TRANSLATIONS_PATH: &str = "translations.nmsg";
/// `.nlang` 内的动态字典文件名。
const DICTIONARY_PATH: &str = "dictionary.json";

/// 已由 Binding 解压、但尚未信任的单个文件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NlangPackageEntry {
    path: String,
    bytes: Vec<u8>,
}

impl NlangPackageEntry {
    /// 用包内路径与原始字节直接构造条目。
    pub fn new(path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            bytes: bytes.into(),
        }
    }

    /// 包内相对路径。
    pub fn path(&self) -> &str {
        &self.path
    }

    /// 文件的原始内容。
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
    /// 由 manifest 与译文生成确定性的 `.nlang` 文件清单。
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

    /// 建议的落盘文件名，如 `zh-Hans.nlang`。
    pub fn file_name(&self) -> String {
        format!("{}.nlang", self.locale)
    }

    /// 只读访问生成的文件清单。
    pub fn entries(&self) -> &[NlangPackageEntry] {
        &self.entries
    }

    /// 消耗输出，取得生成的文件清单。
    pub fn into_entries(self) -> Vec<NlangPackageEntry> {
        self.entries
    }
}

/// `.nlang` 输出生成阶段的稳定失败原因。
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
    /// 用解压得到的文件清单建立输入。
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
    /// 已通过安装校验的 manifest。
    pub fn manifest(&self) -> &NlangValidatedManifest {
        &self.manifest
    }

    /// 包内携带的译文模板。
    pub fn translation(&self) -> &I18nTemplate {
        &self.translation
    }

    /// 按路径读取包内任意原始文件。
    pub fn file(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }
}

/// 语言包验证阶段的稳定失败原因。
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

/// 读取必需文件；缺失时返回 `MissingFile`。
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

/// 路径必须为规范化相对路径：无前缀斜杠、无空段、无 `..` 与平台分隔符。
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

/// 只允许三个固定文件与 `resources/*.nres`。
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

//! `.nar` 发布容器中由 Core 拥有的数据契约。
//!
//! ZIP 读写属于构建工具或 Binding；Core 只接收已经解包的、不可信内存数据并完成校验。

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    GameIdentity, ProjectConfig, Source, SourceKind, SourceList,
    bytecode::{BytecodeDecodeError, BytecodeProgram},
    hir::HirStory,
    lir::LirProgram,
    mir::MirStory,
    resource::{ResourceCatalog, ResourceError, ResourceInput, ResourcePath},
    script::ScriptBundle,
    twee,
};

/// 当前 `.nar` 容器格式版本；校验时与 manifest 中的版本比较。
pub const NAR_FORMAT_VERSION: u16 = 1;
/// `.nar` 字节容器的魔数头：位于 ZIP 负载之前，Host 加载时校验。
/// 魔数让 `.nar` 不再是“改后缀的 ZIP”，杜绝把任意 ZIP 伪装成游戏包。
pub const NAR_MAGIC: &[u8] = b"NAR1";
/// 魔数头的字节长度。
pub const NAR_MAGIC_LEN: usize = NAR_MAGIC.len();
/// `.nar` manifest 声明的包类型。
const NAR_PACKAGE_TYPE: &str = "narrava-game";
/// `.nar` 内 manifest 文件名。
const MANIFEST_PATH: &str = "manifest.json";
/// `.nar` 内源码归档文件名。
const SOURCES_PATH: &str = "sources.json";
/// `.nar` 内编译产物文件名。
const BYTECODE_PATH: &str = "bytecode.json";
/// 发行 NAR 内的作者配置文件名。
const CONFIG_PATH: &str = "config.toml";
/// `.nar` 内资源数据文件的前缀。
const RESOURCES_PREFIX: &str = "resources/";

/// 源码在 `.nar` 中的小写标签类型。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum NarSourceKind {
    Twee,
    TypeScript,
    JavaScript,
}

impl From<SourceKind> for NarSourceKind {
    fn from(value: SourceKind) -> Self {
        match value {
            SourceKind::Twee => Self::Twee,
            SourceKind::TypeScript => Self::TypeScript,
            SourceKind::JavaScript => Self::JavaScript,
        }
    }
}

/// 归档中的一条源码记录：路径、类型与完整内容。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct NarSourceRecord {
    path: String,
    kind: NarSourceKind,
    content: String,
}

/// `.nar` 中保留的拥有型基础源码记录。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NarSourceArchive {
    sources: Vec<NarSourceRecord>,
}

/// 源码归档序列化或还原阶段的稳定失败原因。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NarSourceError {
    InvalidJson(String),
    InvalidPath(String),
    DuplicatePath(String),
    KindMismatch(String),
}

impl NarSourceArchive {
    /// 从源码列表生成归档。
    pub fn from_sources(sources: &SourceList) -> Self {
        Self {
            sources: sources
                .items
                .iter()
                .map(|source| NarSourceRecord {
                    path: source.path.as_str().to_owned(),
                    kind: source.kind.into(),
                    content: source.content.clone(),
                })
                .collect(),
        }
    }

    /// 序列化为 JSON 字符串。
    pub fn to_json(&self) -> Result<String, NarSourceError> {
        serde_json::to_string(self).map_err(|error| NarSourceError::InvalidJson(error.to_string()))
    }

    /// 从 JSON 字符串解码归档。
    pub fn from_json(input: &str) -> Result<Self, NarSourceError> {
        serde_json::from_str(input).map_err(|error| NarSourceError::InvalidJson(error.to_string()))
    }

    /// 还原为源码列表；路径非法、重复或类型与路径不一致时失败。
    pub fn into_sources(self) -> Result<SourceList, NarSourceError> {
        let mut paths = BTreeSet::new();
        let mut items = Vec::with_capacity(self.sources.len());
        for record in self.sources {
            if !paths.insert(record.path.clone()) {
                return Err(NarSourceError::DuplicatePath(record.path));
            }
            let source = Source::from_saved_path(&record.path, record.content)
                .map_err(|_| NarSourceError::InvalidPath(record.path.clone()))?;
            if NarSourceKind::from(source.kind) != record.kind {
                return Err(NarSourceError::KindMismatch(record.path));
            }
            items.push(source);
        }
        Ok(SourceList { items })
    }
}

impl fmt::Display for NarSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(formatter, "NAR 源码记录 JSON 无效: {message}"),
            Self::InvalidPath(path) => write!(formatter, "NAR 源码路径无效: {path}"),
            Self::DuplicatePath(path) => write!(formatter, "NAR 源码路径重复: {path}"),
            Self::KindMismatch(path) => write!(formatter, "NAR 源码类型与路径不一致: {path}"),
        }
    }
}

impl Error for NarSourceError {}

/// `.nar` manifest 的直接 JSON 映射；哈希字段保护对应文件内容。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct NarManifest {
    package_type: String,
    format_version: u16,
    game_id: String,
    game_version: String,
    source_hash: String,
    bytecode_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    config_hash: Option<String>,
    #[serde(default)]
    resources: BTreeMap<String, NarResourceRecord>,
}

/// manifest 中单个资源的媒体类型与 SHA-256 摘要。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct NarResourceRecord {
    media_type: String,
    hash: String,
}

/// Binding 从 ZIP 中解包后交给 Core 的内存文件集。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NarPackage {
    files: Vec<(String, Vec<u8>)>,
}

/// 通过全部校验、可交给 Engine 启动的 `.nar` 内容。
#[derive(Debug)]
pub struct ValidatedNarPackage {
    game: GameIdentity,
    sources: SourceList,
    bytecode: BytecodeProgram,
    resources: ResourceCatalog,
}

/// `.nar` 构建或校验阶段的稳定失败原因。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NarPackageError {
    InvalidPath(String),
    DuplicatePath(String),
    UnknownFile(String),
    MissingManifest,
    MissingSources,
    MissingBytecode,
    InvalidManifest,
    WrongPackageType,
    UnsupportedFormat(u16),
    InvalidGameIdentity,
    InvalidSources(NarSourceError),
    InvalidBytecode(BytecodeDecodeError),
    Compile(NarCompileError),
    InvalidResource(ResourceError),
    ResourceHashMismatch(String),
    UnexpectedResource(String),
    HashMismatch,
}

/// 从源码编译 Bytecode 过程中某一阶段的失败。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NarCompileError {
    stage: &'static str,
    message: String,
}

impl NarPackage {
    /// 接收 Binding 已解包的内存文件；路径集合在读取任何内容前完成校验。
    pub fn from_files(
        files: impl IntoIterator<Item = (String, Vec<u8>)>,
    ) -> Result<Self, NarPackageError> {
        let mut names = BTreeSet::new();
        let mut collected = Vec::new();
        for (path, data) in files {
            if path.is_empty()
                || path.starts_with('/')
                || path.contains('\\')
                || path
                    .split('/')
                    .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
            {
                return Err(NarPackageError::InvalidPath(path));
            }
            if !matches!(
                path.as_str(),
                MANIFEST_PATH | SOURCES_PATH | BYTECODE_PATH | CONFIG_PATH
            ) {
                let Some(resource_path) = path.strip_prefix(RESOURCES_PREFIX) else {
                    return Err(NarPackageError::UnknownFile(path));
                };
                ResourcePath::parse(resource_path)
                    .map_err(|_| NarPackageError::InvalidPath(path.clone()))?;
            }
            if !names.insert(path.clone()) {
                return Err(NarPackageError::DuplicatePath(path));
            }
            collected.push((path, data));
        }
        Ok(Self { files: collected })
    }

    /// 构建不携带资源、不可独立启动的开发 NAR。
    pub fn build(config: &ProjectConfig, sources: &SourceList) -> Result<Self, NarPackageError> {
        Self::build_with_resources(config, sources, &ResourceCatalog::default())
    }

    /// 构建携带资源的开发 NAR。
    pub fn build_with_resources(
        config: &ProjectConfig,
        sources: &SourceList,
        resources: &ResourceCatalog,
    ) -> Result<Self, NarPackageError> {
        Self::build_files(config, sources, resources, None)
    }

    /// 构建可独立启动的发行 NAR，并把完整作者配置纳入哈希保护。
    pub fn build_release(
        config: &ProjectConfig,
        sources: &SourceList,
        resources: &ResourceCatalog,
        config_toml: &str,
    ) -> Result<Self, NarPackageError> {
        Self::build_files(config, sources, resources, Some(config_toml))
    }

    /// 共享的构建流程：编译源码、归档源码与资源、生成带哈希的 manifest。
    fn build_files(
        config: &ProjectConfig,
        sources: &SourceList,
        resources: &ResourceCatalog,
        config_toml: Option<&str>,
    ) -> Result<Self, NarPackageError> {
        let source_json = NarSourceArchive::from_sources(sources)
            .to_json()
            .map_err(NarPackageError::InvalidSources)?;
        let bytecode = compile_sources(sources).map_err(NarPackageError::Compile)?;
        let bytecode_json = bytecode
            .to_json()
            .map_err(NarPackageError::InvalidBytecode)?;
        let mut resource_manifest = BTreeMap::new();
        let mut resource_files = Vec::with_capacity(resources.len());
        for input in resources
            .inputs()
            .map_err(NarPackageError::InvalidResource)?
        {
            let media_type = input
                .media_type()
                .expect("目录导出的 ResourceInput 必须携带媒体类型")
                .to_owned();
            let path = input.path().to_owned();
            let bytes = input.into_bytes();
            resource_manifest.insert(
                path.clone(),
                NarResourceRecord {
                    media_type,
                    hash: sha256_hex(&bytes),
                },
            );
            resource_files.push((format!("{RESOURCES_PREFIX}{path}"), bytes));
        }
        let manifest = NarManifest {
            package_type: NAR_PACKAGE_TYPE.to_owned(),
            format_version: NAR_FORMAT_VERSION,
            game_id: config.game.id.clone(),
            game_version: config.game.version.clone(),
            source_hash: sha256_hex(source_json.as_bytes()),
            bytecode_hash: sha256_hex(&bytecode_json),
            config_hash: config_toml.map(|text| sha256_hex(text.as_bytes())),
            resources: resource_manifest,
        };
        let manifest =
            serde_json::to_vec(&manifest).map_err(|_| NarPackageError::InvalidManifest)?;
        let mut files = vec![
            (MANIFEST_PATH.to_owned(), manifest),
            (SOURCES_PATH.to_owned(), source_json.into_bytes()),
            (BYTECODE_PATH.to_owned(), bytecode_json),
        ];
        files.extend(resource_files);
        if let Some(config_toml) = config_toml {
            files.push((CONFIG_PATH.to_owned(), config_toml.as_bytes().to_vec()));
        }
        Ok(Self { files })
    }

    /// 按路径取得可变字节引用；供构建工具写入自定义文件。
    pub fn file_mut(&mut self, path: &str) -> Option<&mut Vec<u8>> {
        self.files
            .iter_mut()
            .find_map(|(name, data)| (name == path).then_some(data))
    }

    /// 供 ZIP 构建工具读取 Core 拥有的规范文件集。
    pub fn files(&self) -> impl ExactSizeIterator<Item = (&str, &[u8])> {
        self.files
            .iter()
            .map(|(path, data)| (path.as_str(), data.as_slice()))
    }

    /// 消耗包，取得全部（路径，字节）文件。
    pub fn into_files(self) -> Vec<(String, Vec<u8>)> {
        self.files
    }

    /// 校验格式、哈希、游戏身份与资源，成功时返回可启动的包内容。
    pub fn validate(&self) -> Result<ValidatedNarPackage, NarPackageError> {
        let manifest = self
            .file(MANIFEST_PATH)
            .ok_or(NarPackageError::MissingManifest)?;
        let manifest: NarManifest =
            serde_json::from_slice(manifest).map_err(|_| NarPackageError::InvalidManifest)?;
        if manifest.package_type != NAR_PACKAGE_TYPE {
            return Err(NarPackageError::WrongPackageType);
        }
        if manifest.format_version != NAR_FORMAT_VERSION {
            return Err(NarPackageError::UnsupportedFormat(manifest.format_version));
        }
        let game = GameIdentity::new(manifest.game_id, &manifest.game_version)
            .map_err(|_| NarPackageError::InvalidGameIdentity)?;
        let source_json = self
            .file(SOURCES_PATH)
            .ok_or(NarPackageError::MissingSources)?;
        if sha256_hex(source_json) != manifest.source_hash {
            return Err(NarPackageError::HashMismatch);
        }
        let source_json = std::str::from_utf8(source_json).map_err(|_| {
            NarPackageError::InvalidSources(NarSourceError::InvalidJson(String::from("UTF-8 无效")))
        })?;
        let sources = NarSourceArchive::from_json(source_json)
            .and_then(NarSourceArchive::into_sources)
            .map_err(NarPackageError::InvalidSources)?;
        let bytecode_json = self
            .file(BYTECODE_PATH)
            .ok_or(NarPackageError::MissingBytecode)?;
        if sha256_hex(bytecode_json) != manifest.bytecode_hash {
            return Err(NarPackageError::HashMismatch);
        }
        let bytecode =
            BytecodeProgram::from_json(bytecode_json).map_err(NarPackageError::InvalidBytecode)?;
        match (manifest.config_hash.as_deref(), self.file(CONFIG_PATH)) {
            (Some(expected), Some(config)) if sha256_hex(config) == expected => {}
            (None, None) => {}
            _ => return Err(NarPackageError::HashMismatch),
        }
        let actual_resources: BTreeSet<&str> = self
            .files
            .iter()
            .filter_map(|(path, _)| path.strip_prefix(RESOURCES_PREFIX))
            .collect();
        let expected_resources: BTreeSet<&str> =
            manifest.resources.keys().map(String::as_str).collect();
        if let Some(path) = actual_resources.difference(&expected_resources).next() {
            return Err(NarPackageError::UnexpectedResource((*path).to_owned()));
        }
        let mut resource_inputs = Vec::with_capacity(manifest.resources.len());
        for (path, record) in manifest.resources {
            let bytes = self
                .file(&format!("{RESOURCES_PREFIX}{path}"))
                .ok_or_else(|| NarPackageError::ResourceHashMismatch(path.clone()))?;
            if sha256_hex(bytes) != record.hash {
                return Err(NarPackageError::ResourceHashMismatch(path));
            }
            resource_inputs.push(ResourceInput::with_media_type(
                path,
                record.media_type,
                bytes.to_vec(),
            ));
        }
        let resources =
            ResourceCatalog::new(resource_inputs).map_err(NarPackageError::InvalidResource)?;
        Ok(ValidatedNarPackage {
            game,
            sources,
            bytecode,
            resources,
        })
    }

    /// 按文件名读取原始内容。
    fn file(&self, path: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find_map(|(name, data)| (name == path).then_some(data.as_slice()))
    }

    /// 读取发行 NAR 内嵌的作者配置文本。
    pub fn config_toml(&self) -> Option<&str> {
        std::str::from_utf8(self.file(CONFIG_PATH)?).ok()
    }
}

impl ValidatedNarPackage {
    /// 包声明的游戏身份。
    pub fn game(&self) -> &GameIdentity {
        &self.game
    }

    /// 包内源码列表。
    pub fn sources(&self) -> &SourceList {
        &self.sources
    }

    /// 包内编译产物。
    pub fn bytecode(&self) -> &BytecodeProgram {
        &self.bytecode
    }

    /// 包内资源目录。
    pub fn resources(&self) -> &ResourceCatalog {
        &self.resources
    }

    /// 使用已反序列化的拥有型 Bytecode，并把借用 Script Bundle 限定在回调内。
    pub fn with_bytecode<R>(
        &self,
        use_program: impl FnOnce(&BytecodeProgram, &ScriptBundle<'_>) -> R,
    ) -> Result<R, NarCompileError> {
        let scripts = ScriptBundle::from_sources(&self.sources);
        Ok(use_program(&self.bytecode, &scripts))
    }
}

impl NarCompileError {
    /// 失败阶段（twee/hir/mir/lir）。
    pub fn stage(&self) -> &'static str {
        self.stage
    }

    /// 阶段内错误说明。
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 构造带阶段的编译错误。
    fn new(stage: &'static str, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }
}

impl fmt::Display for NarPackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "NAR 包校验失败: {self:?}")
    }
}

impl Error for NarPackageError {}

impl fmt::Display for NarCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "NAR {} 编译失败: {}", self.stage, self.message)
    }
}

impl Error for NarCompileError {}

/// 计算内容 SHA-256 摘要的十六进制字符串。
fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 沿 twee → HIR → MIR → LIR 管线编译源码到 Bytecode。
fn compile_sources(sources: &SourceList) -> Result<BytecodeProgram, NarCompileError> {
    let ast = twee::Story::build(&sources.items)
        .map_err(|error| NarCompileError::new("twee", error.to_string()))?;
    let hir = HirStory::lower(&ast)
        .map_err(|error| NarCompileError::new("hir", error.diagnostic.message))?;
    let mir = MirStory::lower(&hir).map_err(|error| NarCompileError::new("mir", error.kind))?;
    let lir = LirProgram::lower(&mir).map_err(|error| {
        NarCompileError::new(
            "lir",
            format!(
                "{} 的指令 {:?} 无法降低: {:?}",
                error.passage(),
                error.instruction(),
                error.kind()
            ),
        )
    })?;
    Ok(BytecodeProgram::compile(&lir))
}

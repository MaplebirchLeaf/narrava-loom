//! Host 无关的基础游戏资源目录。

use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use serde::{Deserialize, Serialize};

mod package;
pub use package::{NresPackage, NresPackageError};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourcePath(String);

impl ResourcePath {
    pub fn parse(path: &str) -> Result<Self, ResourceError> {
        if path.is_empty()
            || path.starts_with('/')
            || path.contains('\\')
            || path
                .split('/')
                .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
        {
            return Err(ResourceError::InvalidPath(path.to_owned()));
        }
        Ok(Self(path.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceInput {
    path: String,
    media_type: Option<String>,
    bytes: Vec<u8>,
}

impl ResourceInput {
    pub fn new(path: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            media_type: None,
            bytes,
        }
    }

    pub fn with_media_type(
        path: impl Into<String>,
        media_type: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            path: path.into(),
            media_type: Some(media_type.into()),
            bytes,
        }
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[derive(Clone, Debug)]
struct ResourceEntry {
    media_type: String,
    size: usize,
    backing: ResourceBacking,
}

#[derive(Clone, Debug)]
enum ResourceBacking {
    Memory(Arc<[u8]>),
    File {
        path: PathBuf,
        cache: Arc<OnceLock<Arc<[u8]>>>,
    },
}

impl ResourceBacking {
    fn read(&self) -> Result<&[u8], ResourceError> {
        match self {
            Self::Memory(bytes) => Ok(bytes),
            Self::File { path, cache } => {
                if let Some(bytes) = cache.get() {
                    return Ok(bytes);
                }
                let loaded: Arc<[u8]> = fs::read(path)
                    .map(Arc::from)
                    .map_err(|error| read_error(path, error))?;
                // 并发首次读取可能重复 I/O，但所有调用最终共享同一份缓存。
                let _ = cache.set(loaded);
                Ok(cache.get().expect("成功读取后 Resource 缓存必须存在"))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceInfo<'resource> {
    path: &'resource str,
    media_type: &'resource str,
    size: usize,
}

impl ResourceInfo<'_> {
    pub fn path(&self) -> &str {
        self.path
    }

    pub fn media_type(&self) -> &str {
        self.media_type
    }

    pub fn size(&self) -> usize {
        self.size
    }
}

#[derive(Clone, Debug, Default)]
pub struct ResourceCatalog {
    entries: BTreeMap<ResourcePath, ResourceEntry>,
}

impl ResourceCatalog {
    pub fn new(inputs: impl IntoIterator<Item = ResourceInput>) -> Result<Self, ResourceError> {
        let mut entries = BTreeMap::new();
        for input in inputs {
            let path = ResourcePath::parse(&input.path)?;
            let media_type = match input.media_type {
                Some(media_type) => validate_media_type(media_type)?,
                None => infer_media_type(path.as_str()).to_owned(),
            };
            if entries
                .insert(
                    path.clone(),
                    ResourceEntry {
                        media_type,
                        size: input.bytes.len(),
                        backing: ResourceBacking::Memory(input.bytes.into()),
                    },
                )
                .is_some()
            {
                return Err(ResourceError::DuplicatePath(path.0));
            }
        }
        Ok(Self { entries })
    }

    /// 递归发现游戏目录中可选的 `resources/`，不跟随符号链接。
    ///
    /// 此阶段只读取目录项与文件大小；文件内容在首次 [`Self::read`] 时加载。
    pub fn discover(project: impl AsRef<Path>) -> Result<Self, ResourceError> {
        let root = project.as_ref().join("resources");
        if !root.exists() {
            return Ok(Self::default());
        }
        let mut entries = BTreeMap::new();
        collect_resources(&root, &root, &mut entries)?;
        Ok(Self { entries })
    }

    pub fn paths(&self) -> impl ExactSizeIterator<Item = &str> {
        self.entries.keys().map(ResourcePath::as_str)
    }

    pub fn has(&self, path: &str) -> bool {
        ResourcePath::parse(path)
            .ok()
            .is_some_and(|path| self.entries.contains_key(&path))
    }

    pub fn pick<I, P>(&self, candidates: I) -> Option<&str>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<str>,
    {
        candidates.into_iter().find_map(|candidate| {
            let path = ResourcePath::parse(candidate.as_ref()).ok()?;
            self.entries
                .get_key_value(&path)
                .map(|(path, _)| path.as_str())
        })
    }

    pub fn info(&self, path: &str) -> Option<ResourceInfo<'_>> {
        let path = ResourcePath::parse(path).ok()?;
        let (path, entry) = self.entries.get_key_value(&path)?;
        Some(ResourceInfo {
            path: path.as_str(),
            media_type: &entry.media_type,
            size: entry.size,
        })
    }

    /// 按逻辑路径打开资源。磁盘资源在首次调用时读取，成功结果由目录缓存。
    pub fn read(&self, path: &str) -> Result<Option<&[u8]>, ResourceError> {
        let path = ResourcePath::parse(path)?;
        self.entries
            .get(&path)
            .map(|entry| entry.backing.read())
            .transpose()
    }

    pub fn text(&self, path: &str) -> Result<Option<&str>, ResourceError> {
        let Some(bytes) = self.read(path)? else {
            return Ok(None);
        };
        std::str::from_utf8(bytes)
            .map(Some)
            .map_err(|_| ResourceError::InvalidUtf8(path.to_owned()))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn inputs(&self) -> Result<Vec<ResourceInput>, ResourceError> {
        self.entries
            .iter()
            .map(|(path, entry)| {
                Ok(ResourceInput {
                    path: path.0.clone(),
                    media_type: Some(entry.media_type.clone()),
                    bytes: entry.backing.read()?.to_vec(),
                })
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceError {
    InvalidPath(String),
    DuplicatePath(String),
    InvalidMediaType(String),
    InvalidUtf8(String),
    Read { path: PathBuf, message: String },
}

impl fmt::Display for ResourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Resource 失败: {self:?}")
    }
}

impl Error for ResourceError {}

fn collect_resources(
    root: &Path,
    directory: &Path,
    resources: &mut BTreeMap<ResourcePath, ResourceEntry>,
) -> Result<(), ResourceError> {
    let entries = fs::read_dir(directory).map_err(|error| read_error(directory, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| read_error(directory, error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| read_error(&entry.path(), error))?;
        if file_type.is_dir() {
            collect_resources(root, &entry.path(), resources)?;
        } else if file_type.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .expect("资源路径必须位于发现根目录")
                .to_path_buf();
            let logical_path = relative
                .components()
                .map(|component| {
                    component
                        .as_os_str()
                        .to_str()
                        .ok_or_else(|| ResourceError::InvalidPath(relative.display().to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?
                .join("/");
            let path = ResourcePath::parse(&logical_path)?;
            let size = entry
                .metadata()
                .map_err(|error| read_error(&entry.path(), error))?
                .len()
                .try_into()
                .map_err(|_| ResourceError::Read {
                    path: entry.path(),
                    message: String::from("资源大小超出当前平台可表示范围"),
                })?;
            let resource = ResourceEntry {
                media_type: infer_media_type(path.as_str()).to_owned(),
                size,
                backing: ResourceBacking::File {
                    path: entry.path(),
                    cache: Arc::new(OnceLock::new()),
                },
            };
            if resources.insert(path.clone(), resource).is_some() {
                return Err(ResourceError::DuplicatePath(path.0));
            }
        }
    }
    Ok(())
}

fn read_error(path: &Path, error: io::Error) -> ResourceError {
    ResourceError::Read {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

fn validate_media_type(media_type: String) -> Result<String, ResourceError> {
    let valid = !media_type.is_empty()
        && !media_type.chars().any(char::is_whitespace)
        && media_type.split_once('/').is_some_and(|(kind, subtype)| {
            !kind.is_empty() && !subtype.is_empty() && !subtype.contains('/')
        });
    if valid {
        Ok(media_type)
    } else {
        Err(ResourceError::InvalidMediaType(media_type))
    }
}

fn infer_media_type(path: &str) -> &'static str {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("avif") => "image/avif",
        Some("gif") => "image/gif",
        Some("jpeg" | "jpg") => "image/jpeg",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("aac") => "audio/aac",
        Some("flac") => "audio/flac",
        Some("m4a") => "audio/mp4",
        Some("mp3") => "audio/mpeg",
        Some("oga" | "ogg") => "audio/ogg",
        Some("wav") => "audio/wav",
        Some("mp4") => "video/mp4",
        Some("ogv") => "video/ogg",
        Some("webm") => "video/webm",
        Some("otf") => "font/otf",
        Some("ttf") => "font/ttf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("css") => "text/css",
        Some("csv") => "text/csv",
        Some("html") => "text/html",
        Some("js") => "text/javascript",
        Some("json") => "application/json",
        Some("txt") => "text/plain",
        Some("yaml" | "yml") => "application/yaml",
        Some("glb") => "model/gltf-binary",
        Some("gltf") => "model/gltf+json",
        _ => "application/octet-stream",
    }
}

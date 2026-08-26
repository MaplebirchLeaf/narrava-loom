//! `.nres` 的规范文件清单与完整性验证。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{ResourceCatalog, ResourceInput};

/// `.nres` 内的 manifest 文件名。
const MANIFEST: &str = "manifest.json";
/// `.nres` 内资源数据文件的前缀。
const DATA_PREFIX: &str = "data/";

/// `.nres` manifest 的直接 JSON 映射。
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Manifest {
    package_type: String,
    format_version: u16,
    resources: BTreeMap<String, Entry>,
}

/// manifest 中单个资源的媒体类型与 SHA-256 摘要。
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    media_type: String,
    hash: String,
}

/// `.nres` 的规范内存文件清单：`manifest.json` 加 `data/<path>` 数据文件。
#[derive(Clone, Debug)]
pub struct NresPackage {
    files: Vec<(String, Vec<u8>)>,
}

/// `.nres` 构建或验证阶段的稳定失败原因。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NresPackageError {
    InvalidManifest,
    InvalidPath(String),
    DuplicatePath(String),
    MissingResource(String),
    UnexpectedResource(String),
    HashMismatch(String),
    Catalog(String),
}

impl NresPackage {
    /// 由资源目录生成清单与数据文件；内容哈希写入 manifest。
    pub fn build(catalog: &ResourceCatalog) -> Result<Self, NresPackageError> {
        let mut resources = BTreeMap::new();
        let mut files = Vec::new();
        for input in catalog
            .inputs()
            .map_err(|error| NresPackageError::Catalog(error.to_string()))?
        {
            let path = input.path().to_owned();
            let media_type = input
                .media_type()
                .unwrap_or("application/octet-stream")
                .to_owned();
            let bytes = input.into_bytes();
            resources.insert(
                path.clone(),
                Entry {
                    media_type,
                    hash: hash(&bytes),
                },
            );
            files.push((format!("{DATA_PREFIX}{path}"), bytes));
        }
        let manifest = Manifest {
            package_type: String::from("narrava-resource"),
            format_version: 1,
            resources,
        };
        files.push((
            String::from(MANIFEST),
            serde_json::to_vec(&manifest).map_err(|_| NresPackageError::InvalidManifest)?,
        ));
        files.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(Self { files })
    }

    /// 接收已解包的内存文件；先完成路径与重复检查。
    pub fn from_files(files: Vec<(String, Vec<u8>)>) -> Result<Self, NresPackageError> {
        let mut names = BTreeSet::new();
        for (path, _) in &files {
            if path.is_empty()
                || path.starts_with('/')
                || path.contains('\\')
                || path
                    .split('/')
                    .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
            {
                return Err(NresPackageError::InvalidPath(path.clone()));
            }
            if !names.insert(path.clone()) {
                return Err(NresPackageError::DuplicatePath(path.clone()));
            }
        }
        Ok(Self { files })
    }

    /// 以（路径，字节）对的形式遍历全部文件。
    pub fn files(&self) -> impl Iterator<Item = (String, Vec<u8>)> + '_ {
        self.files.iter().cloned()
    }

    /// 校验 manifest、文件集与内容哈希，成功时重建资源目录。
    pub fn validate(&self) -> Result<ResourceCatalog, NresPackageError> {
        let manifest = self
            .file(MANIFEST)
            .ok_or(NresPackageError::InvalidManifest)?;
        let manifest: Manifest =
            serde_json::from_slice(manifest).map_err(|_| NresPackageError::InvalidManifest)?;
        if manifest.package_type != "narrava-resource" || manifest.format_version != 1 {
            return Err(NresPackageError::InvalidManifest);
        }
        for path in self
            .files
            .iter()
            .filter_map(|(path, _)| path.strip_prefix(DATA_PREFIX))
        {
            if !manifest.resources.contains_key(path) {
                return Err(NresPackageError::UnexpectedResource(path.to_owned()));
            }
        }
        let mut inputs = Vec::new();
        for (path, entry) in manifest.resources {
            let bytes = self
                .file(&format!("{DATA_PREFIX}{path}"))
                .ok_or_else(|| NresPackageError::MissingResource(path.clone()))?;
            if hash(bytes) != entry.hash {
                return Err(NresPackageError::HashMismatch(path));
            }
            inputs.push(ResourceInput::with_media_type(
                path,
                entry.media_type,
                bytes.to_vec(),
            ));
        }
        ResourceCatalog::new(inputs).map_err(|error| NresPackageError::Catalog(error.to_string()))
    }

    /// 按文件名读取原始内容。
    fn file(&self, name: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find_map(|(path, bytes)| (path == name).then_some(bytes.as_slice()))
    }
}

/// 计算内容 SHA-256 摘要的十六进制字符串。
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::NresPackage;
    use crate::resource::{ResourceCatalog, ResourceInput};

    #[test]
    fn nres_round_trips_resource_bytes_and_media_type() {
        let catalog = ResourceCatalog::new([ResourceInput::with_media_type(
            "guide.txt",
            "text/plain",
            b"hello".to_vec(),
        )])
        .unwrap();
        let package = NresPackage::build(&catalog).unwrap();
        let decoded = NresPackage::from_files(package.files().collect())
            .unwrap()
            .validate()
            .unwrap();
        assert_eq!(decoded.text("guide.txt").unwrap(), Some("hello"));
        assert_eq!(
            decoded.info("guide.txt").unwrap().media_type(),
            "text/plain"
        );
    }
}

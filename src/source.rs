//! 源码发现、路径规范化与 UTF-8 内容读取。

use std::{
    error::Error,
    fmt, fs, io,
    path::{Component, Path, PathBuf},
};

/// 一份已读取的 UTF-8 源文件。
#[derive(Clone, Debug)]
pub struct Source {
    pub path: SourcePath,
    pub kind: SourceKind,
    pub content: String,
}

/// 项目源码集合。
#[derive(Clone, Debug)]
pub struct SourceList {
    pub items: Vec<Source>,
}

impl SourceList {
    /// 按调用方给出的顺序读取源码。
    pub fn load(project: impl AsRef<Path>, paths: &[PathBuf]) -> Result<Self, SourceError> {
        let project: &Path = project.as_ref();
        let mut items: Vec<Source> = Vec::with_capacity(paths.len());

        for path in paths {
            let source: Source = Source::load(project, path)?;
            items.push(source);
        }

        Ok(Self { items })
    }

    /// 从 `contents/` 递归发现源码，并按保存路径稳定排序。
    pub fn discover(project: impl AsRef<Path>) -> Result<Self, SourceError> {
        let project: PathBuf = project.as_ref().to_path_buf();

        if !is_relative_path(&project) {
            let source: io::Error =
                io::Error::new(io::ErrorKind::InvalidInput, "游戏目录必须是普通相对路径");
            return Err(SourceError {
                path: project,
                source,
            });
        }

        let contents: PathBuf = project.join("contents");
        let mut paths: Vec<PathBuf> = Vec::new();
        Self::collect_paths(&contents, Path::new(""), &mut paths)?;

        let mut sources: Self = Self::load(&project, &paths)?;
        sources
            .items
            .sort_by(|left: &Source, right: &Source| left.path.as_str().cmp(right.path.as_str()));

        Ok(sources)
    }

    /// 递归收集受支持类型文件的相对保存路径。
    fn collect_paths(
        directory: &Path,
        relative: &Path,
        paths: &mut Vec<PathBuf>,
    ) -> Result<(), SourceError> {
        let entries: fs::ReadDir =
            fs::read_dir(directory).map_err(|source: io::Error| SourceError {
                path: directory.to_path_buf(),
                source,
            })?;

        for entry in entries {
            let entry: fs::DirEntry = entry.map_err(|source: io::Error| SourceError {
                path: directory.to_path_buf(),
                source,
            })?;
            let path: PathBuf = relative.join(entry.file_name());
            let file_type: fs::FileType =
                entry.file_type().map_err(|source: io::Error| SourceError {
                    path: entry.path(),
                    source,
                })?;

            if file_type.is_dir() {
                Self::collect_paths(&entry.path(), &path, paths)?;
            } else if file_type.is_file() && SourceKind::supports_path(&path) {
                paths.push(path);
            }
        }

        Ok(())
    }
}

/// 当前能够进入编译链路的源码类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Twee,
    TypeScript,
    JavaScript,
}

impl SourceKind {
    /// 按保存路径扩展名推断类型；仅 crate 内部使用。
    pub(crate) fn from_path(path: &SourcePath) -> io::Result<Self> {
        let extension: Option<&str> = Path::new(path.as_str())
            .extension()
            .and_then(|value| value.to_str());

        Self::from_extension(extension)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "不支持的源码类型"))
    }

    /// 路径是否带受支持类型的扩展名。
    fn supports_path(path: &Path) -> bool {
        let extension: Option<&str> = path.extension().and_then(|value| value.to_str());
        Self::from_extension(extension).is_some()
    }

    /// 扩展名到源码类型的映射；不认识的返回 `None`。
    fn from_extension(extension: Option<&str>) -> Option<Self> {
        match extension {
            Some("twee") => Some(Self::Twee),
            Some("ts") => Some(Self::TypeScript),
            Some("js") => Some(Self::JavaScript),
            _ => None,
        }
    }
}

/// 平台无关的源码保存路径。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourcePath(String);

impl SourcePath {
    /// 返回省略 `contents/` 的平台无关保存路径。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 动态 Twee 片段（如 `Macro.parse()` 输入）使用的虚拟来源，无真实文件。
    pub fn fragment() -> Self {
        Self(String::from("<fragment>"))
    }

    /// 把磁盘相对路径规范化为平台无关保存路径；仅 crate 内部使用。
    pub(crate) fn from_path(path: &Path) -> io::Result<Self> {
        if !is_relative_path(path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "保存路径必须是普通相对路径",
            ));
        }

        let mut segments: Vec<&str> = Vec::new();

        for component in path.components() {
            let Component::Normal(segment) = component else {
                unreachable!("路径已经完成验证");
            };
            let segment: &str = segment.to_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "保存路径必须是 UTF-8")
            })?;

            if segment.contains('\\') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "保存路径不能包含反斜杠",
                ));
            }

            segments.push(segment);
        }

        Ok(Self(segments.join("/")))
    }
}

/// 带实际读取路径的 Source 错误。
#[derive(Debug)]
pub struct SourceError {
    pub path: PathBuf,
    pub source: io::Error,
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "无法读取 {}: {}",
            self.path.display(),
            self.source
        )
    }
}

impl Error for SourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

impl Source {
    /// 从保存路径与内容重建 Source（NAR 还原用）；仅 crate 内部使用。
    pub(crate) fn from_saved_path(path: &str, content: String) -> io::Result<Self> {
        let path: SourcePath = SourcePath::from_path(Path::new(path))?;
        let kind: SourceKind = SourceKind::from_path(&path)?;
        Ok(Self {
            path,
            kind,
            content,
        })
    }

    /// 从游戏目录的 `contents/` 下读取一个相对路径。
    pub fn load(project: impl AsRef<Path>, path: impl AsRef<Path>) -> Result<Self, SourceError> {
        let project: PathBuf = project.as_ref().to_path_buf();
        let path: PathBuf = path.as_ref().to_path_buf();

        if !is_relative_path(&project) {
            let source: io::Error =
                io::Error::new(io::ErrorKind::InvalidInput, "游戏目录必须是普通相对路径");
            return Err(SourceError {
                path: project,
                source,
            });
        }

        let saved_path: SourcePath =
            SourcePath::from_path(&path).map_err(|source: io::Error| SourceError {
                path: path.clone(),
                source,
            })?;
        let kind: SourceKind =
            SourceKind::from_path(&saved_path).map_err(|source: io::Error| SourceError {
                path: path.clone(),
                source,
            })?;
        let disk_path: PathBuf = project.join("contents").join(&path);
        let content: String =
            fs::read_to_string(&disk_path).map_err(|source: io::Error| SourceError {
                path: disk_path.clone(),
                source,
            })?;

        Ok(Self {
            path: saved_path,
            kind,
            content,
        })
    }
}

/// 路径是否只由普通段组成（无绝对前缀、无 `..` 等特殊组件）。
fn is_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component: Component<'_>| matches!(component, Component::Normal(_)))
}

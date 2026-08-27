//! Host 启动资产：样式表与 Resource 元数据的发现，以及打包发行时从保留命名空间恢复。

use std::{fs, io, path::Path};

use narrava_loom_core::resource::ResourceCatalog;
use serde::Serialize;

use crate::HostErrorDto;

/// 发行包内 Host 样式表占用的保留 Resource 前缀。
const PACKAGED_STYLE_PREFIX: &str = "__narrava/styles/";

/// 一条可注入 WebView 的样式表。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HostStyleDto {
    /// 相对样式根目录的逻辑路径。
    pub path: String,
    /// CSS 文本。
    pub css: String,
}

/// 单个 Resource 的元数据（不含字节，避免整包随启动 IPC 传输）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HostResourceDto {
    /// 逻辑路径。
    pub path: String,
    /// MIME 类型。
    pub media_type: String,
    /// 字节数。
    pub size: usize,
}

/// Host 启动时一次性下发给前端的资产清单。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct HostAssetsDto {
    /// 窗口标题。
    pub title: String,
    /// 样式表（发行包优先，其次开发目录 `styles/`）。
    pub styles: Vec<HostStyleDto>,
    /// Resource 元数据清单。
    pub resources: Vec<HostResourceDto>,
}

impl HostAssetsDto {
    /// 从 Resource 目录整理资产：保留前缀的样式表还原为 styles，其余列为元数据。
    pub fn from_catalog(resources: &ResourceCatalog) -> Result<Self, HostErrorDto> {
        let mut styles: Vec<HostStyleDto> = Vec::new();
        let mut metadata: Vec<HostResourceDto> = Vec::new();
        for path in resources.paths() {
            if let Some(style_path) = path.strip_prefix(PACKAGED_STYLE_PREFIX) {
                let css: &str = resources
                    .text(path)
                    .map_err(|error| HostErrorDto::new("tauri_host.style_read", error.to_string()))?
                    .ok_or_else(|| {
                        HostErrorDto::new("tauri_host.style_read", "打包 Style 不存在")
                    })?;
                styles.push(HostStyleDto {
                    path: style_path.to_owned(),
                    css: css.to_owned(),
                });
            } else {
                let info = resources.info(path).expect("Resource 路径必须有元数据");
                metadata.push(HostResourceDto {
                    path: path.to_owned(),
                    media_type: info.media_type().to_owned(),
                    size: info.size(),
                });
            }
        }
        Ok(Self {
            title: String::new(),
            styles,
            resources: metadata,
        })
    }

    /// 从开发目录发现资源并补充 `styles/` 下的样式表。
    pub fn discover(game_path: &Path) -> Result<Self, HostErrorDto> {
        let resources = ResourceCatalog::discover(game_path)
            .map_err(|error| HostErrorDto::new("tauri_host.resource", error.to_string()))?;
        Self::discover_with_catalog(game_path, &resources)
    }

    /// 用已有 Resource 目录整理资产，并叠加开发目录样式表。
    pub fn discover_with_catalog(
        game_path: &Path,
        resources: &ResourceCatalog,
    ) -> Result<Self, HostErrorDto> {
        let mut assets = Self::from_catalog(resources)?;
        let development_styles = discover_styles(game_path)?;
        if !development_styles.is_empty() {
            assets.styles = development_styles;
        }
        Ok(assets)
    }
}

/// 递归发现开发目录 `styles/` 下的全部 `.css` 文件。
fn discover_styles(game_path: &Path) -> Result<Vec<HostStyleDto>, HostErrorDto> {
    let root = game_path.join("styles");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut styles = Vec::new();
    collect_styles(&root, &root, &mut styles)?;
    styles.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(styles)
}

/// 递归收集目录树内的 `.css`，路径转为 `/` 分隔的逻辑路径。
fn collect_styles(
    root: &Path,
    directory: &Path,
    styles: &mut Vec<HostStyleDto>,
) -> Result<(), HostErrorDto> {
    let entries = fs::read_dir(directory).map_err(|error| read_error(directory, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| read_error(directory, error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| read_error(&entry.path(), error))?;
        if file_type.is_dir() {
            collect_styles(root, &entry.path(), styles)?;
        } else if file_type.is_file()
            && entry.path().extension().is_some_and(|value| value == "css")
        {
            let relative = entry
                .path()
                .strip_prefix(root)
                .expect("Style 必须位于发现根目录")
                .components()
                .map(|component| {
                    component.as_os_str().to_str().ok_or_else(|| {
                        HostErrorDto::new("tauri_host.style_path", "Style 逻辑路径必须是 UTF-8")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
                .join("/");
            let css = fs::read_to_string(entry.path())
                .map_err(|error| read_error(&entry.path(), error))?;
            styles.push(HostStyleDto {
                path: relative,
                css,
            });
        }
    }
    Ok(())
}

/// 统一资产读取错误。
fn read_error(path: &Path, error: io::Error) -> HostErrorDto {
    HostErrorDto::new(
        "tauri_host.asset_read",
        format!("{}：{error}", path.display()),
    )
}

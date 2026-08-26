//! 开发目录与 `game.nar` 发行包之间的只读装载边界。

use std::{fs, path::Path};

use narrava_loom_core::{
    GameIdentity, ProjectConfig,
    i18n::{NlangPackageEntry, NlangPackageInput, NlangValidatedPackage},
    nar::{NarPackage, ValidatedNarPackage},
    package_zip,
};

use crate::{HostErrorDto, TauriConfigError, TauriProjectConfig};

const PACKAGE_LIMIT: usize = 512 * 1024 * 1024;
type PackageFiles = Vec<(String, Vec<u8>)>;

/// 读取并解包 `game.nar`；文件不存在时返回 `None`（视为开发目录）。
fn load_release_files(game_path: &Path) -> Result<Option<PackageFiles>, HostErrorDto> {
    let path = game_path.join("game.nar");
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|error| {
        HostErrorDto::new(
            "tauri_host.package_read",
            format!("无法读取 {}：{error}", path.display()),
        )
    })?;
    package_zip::decode(&bytes, PACKAGE_LIMIT)
        .map(Some)
        .map_err(|error| HostErrorDto::new("tauri_host.package_zip", error))
}

/// 加载并校验发行包；无 `game.nar` 时返回 `None`。
pub(crate) fn load_release_package(
    game_path: &Path,
) -> Result<Option<ValidatedNarPackage>, HostErrorDto> {
    let Some(files) = load_release_files(game_path)? else {
        return Ok(None);
    };
    let package = NarPackage::from_files(files)
        .and_then(|package| package.validate())
        .map_err(|error| HostErrorDto::new("tauri_host.package", error.to_string()))?;
    Ok(Some(package))
}

/// 只取发行包内的 `config.toml` 文本，用于无需资源目录的配置解析。
pub(crate) fn load_release_config_text(game_path: &Path) -> Result<Option<String>, HostErrorDto> {
    let Some(files) = load_release_files(game_path)? else {
        return Ok(None);
    };
    let package = NarPackage::from_files(files)
        .map_err(|error| HostErrorDto::new("tauri_host.package", error.to_string()))?;
    let text = package
        .config_toml()
        .ok_or_else(|| HostErrorDto::new("tauri_host.config", "game.nar 缺少 config.toml"))?;
    Ok(Some(text.to_owned()))
}

/// 加载 Tauri 配置：优先解析发行包内配置，否则读开发目录 `config.toml`。
pub(crate) fn load_tauri_config(game_path: &Path) -> Result<TauriProjectConfig, HostErrorDto> {
    if let Some(text) = load_release_config_text(game_path)? {
        TauriProjectConfig::parse(&text)
    } else {
        TauriProjectConfig::load(game_path)
    }
    .map_err(|error: TauriConfigError| HostErrorDto::new("tauri_host.config", error.to_string()))
}

/// 加载语言包：开发目录接受 `languages/<locale>/`，发行目录接受 `*.nlang`。
///
/// 两种输入最终都走同一个 Core 校验器，避免开发预览与发行包产生不同的语言语义。
pub(crate) fn load_language_packages(
    game_path: &Path,
    config: &ProjectConfig,
) -> Result<Vec<NlangValidatedPackage>, HostErrorDto> {
    let directory = game_path.join("languages");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let identity: GameIdentity = config
        .identity()
        .map_err(|error| HostErrorDto::new("tauri_host.language_identity", error.to_string()))?;
    let mut paths: Vec<std::path::PathBuf> = fs::read_dir(&directory)
        .map_err(|error| HostErrorDto::new("tauri_host.language_read", error.to_string()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir() || path.extension().and_then(|value| value.to_str()) == Some("nlang")
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let locale: &str = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    HostErrorDto::new("tauri_host.language_name", "语言包文件名不是 UTF-8")
                })?;
            let entries: Vec<NlangPackageEntry> = if path.is_dir() {
                read_language_directory(&path)?
            } else {
                let bytes: Vec<u8> = fs::read(&path).map_err(|error| {
                    HostErrorDto::new("tauri_host.language_read", error.to_string())
                })?;
                package_zip::decode(&bytes, PACKAGE_LIMIT)
                    .map_err(|error| HostErrorDto::new("tauri_host.language_zip", error))?
                    .into_iter()
                    .map(|(name, bytes)| NlangPackageEntry::new(name, bytes))
                    .collect()
            };
            NlangPackageInput::new(entries)
                .validate(locale, &identity)
                .map_err(|error| HostErrorDto::new("tauri_host.language", error.to_string()))
        })
        .collect()
}

/// 把解包语言目录转换成与 `.nlang` 解码结果相同的逻辑文件列表。
fn read_language_directory(root: &Path) -> Result<Vec<NlangPackageEntry>, HostErrorDto> {
    let mut entries: Vec<NlangPackageEntry> = Vec::new();
    collect_language_files(root, root, &mut entries)?;
    entries.sort_by(|left: &NlangPackageEntry, right: &NlangPackageEntry| {
        left.path().cmp(right.path())
    });
    Ok(entries)
}

/// 递归保留相对路径，使语言资源目录与打包后的 ZIP 具有相同结构。
fn collect_language_files(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<NlangPackageEntry>,
) -> Result<(), HostErrorDto> {
    let mut paths: Vec<std::path::PathBuf> = fs::read_dir(directory)
        .map_err(|error| HostErrorDto::new("tauri_host.language_read", error.to_string()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_language_files(root, &path, entries)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let relative: &Path = path
            .strip_prefix(root)
            .expect("递归收集的语言文件必须位于语言根目录");
        let name: String = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let bytes: Vec<u8> = fs::read(&path)
            .map_err(|error| HostErrorDto::new("tauri_host.language_read", error.to_string()))?;
        entries.push(NlangPackageEntry::new(name, bytes));
    }
    Ok(())
}

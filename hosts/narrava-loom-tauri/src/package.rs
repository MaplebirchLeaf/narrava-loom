//! 开发目录与 `game.nar` 发行包之间的只读装载边界。

use std::{fs, path::Path};

use narrava_loom_core::{
    ProjectConfig,
    i18n::{NlangPackageEntry, NlangPackageInput, NlangValidatedPackage},
    nar::{NarPackage, ValidatedNarPackage},
    package_zip,
};

use crate::{HostErrorDto, TauriConfigError, TauriProjectConfig};

const PACKAGE_LIMIT: usize = 512 * 1024 * 1024;
type PackageFiles = Vec<(String, Vec<u8>)>;

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

pub(crate) fn load_tauri_config(game_path: &Path) -> Result<TauriProjectConfig, HostErrorDto> {
    if let Some(text) = load_release_config_text(game_path)? {
        TauriProjectConfig::parse(&text)
    } else {
        TauriProjectConfig::load(game_path)
    }
    .map_err(|error: TauriConfigError| HostErrorDto::new("tauri_host.config", error.to_string()))
}

/// Loads external language packages; ZIP and filesystem details stay in the Binding.
pub(crate) fn load_language_packages(
    game_path: &Path,
    config: &ProjectConfig,
) -> Result<Vec<NlangValidatedPackage>, HostErrorDto> {
    let directory = game_path.join("languages");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let identity = config
        .identity()
        .map_err(|error| HostErrorDto::new("tauri_host.language_identity", error.to_string()))?;
    let mut paths = fs::read_dir(&directory)
        .map_err(|error| HostErrorDto::new("tauri_host.language_read", error.to_string()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("nlang"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let locale = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    HostErrorDto::new("tauri_host.language_name", "语言包文件名不是 UTF-8")
                })?;
            let bytes = fs::read(&path).map_err(|error| {
                HostErrorDto::new("tauri_host.language_read", error.to_string())
            })?;
            let entries = package_zip::decode(&bytes, PACKAGE_LIMIT)
                .map_err(|error| HostErrorDto::new("tauri_host.language_zip", error))?
                .into_iter()
                .map(|(name, bytes)| NlangPackageEntry::new(name, bytes))
                .collect();
            NlangPackageInput::new(entries)
                .validate(locale, &identity)
                .map_err(|error| HostErrorDto::new("tauri_host.language", error.to_string()))
        })
        .collect()
}

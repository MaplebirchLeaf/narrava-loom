//! TUI Host 的文件系统平台服务。

use std::{
    fs,
    path::{Path, PathBuf},
};

use narrava_loom_core::{
    GameIdentity, ProjectConfig,
    i18n::{NlangPackageEntry, NlangPackageInput, NlangValidatedPackage},
    package_zip,
};
use narrava_loom_protocol::HostErrorDto;

const MAX_PACKAGE_BYTES: usize = 512 << 20;

pub(crate) fn load_languages(
    game_path: &Path,
    config: &ProjectConfig,
) -> Result<Vec<NlangValidatedPackage>, HostErrorDto> {
    let directory: PathBuf = game_path.join("languages");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let identity: GameIdentity = config
        .identity()
        .map_err(|error| language_error("identity", error.to_string()))?;
    let mut paths: Vec<PathBuf> = fs::read_dir(&directory)
        .map_err(|error| language_error("read", error.to_string()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path: &PathBuf| {
            path.is_dir() || path.extension().and_then(|value| value.to_str()) == Some("nlang")
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path: PathBuf| load_language(&path, &identity))
        .collect()
}

fn load_language(
    path: &Path,
    identity: &GameIdentity,
) -> Result<NlangValidatedPackage, HostErrorDto> {
    let locale: &str = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| language_error("name", "语言包名称不是 UTF-8"))?;
    let entries: Vec<NlangPackageEntry> = if path.is_dir() {
        read_language_directory(path)?
    } else {
        let bytes: Vec<u8> =
            fs::read(path).map_err(|error| language_error("read", error.to_string()))?;
        package_zip::decode(&bytes, MAX_PACKAGE_BYTES)
            .map_err(|error| language_error("zip", error))?
            .into_iter()
            .map(|(name, bytes)| NlangPackageEntry::new(name, bytes))
            .collect()
    };
    NlangPackageInput::new(entries)
        .validate(locale, identity)
        .map_err(|error| language_error("validate", error.to_string()))
}

fn read_language_directory(root: &Path) -> Result<Vec<NlangPackageEntry>, HostErrorDto> {
    let mut entries: Vec<NlangPackageEntry> = Vec::new();
    collect_language_files(root, root, &mut entries)?;
    entries.sort_by(|left: &NlangPackageEntry, right: &NlangPackageEntry| {
        left.path().cmp(right.path())
    });
    Ok(entries)
}

fn collect_language_files(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<NlangPackageEntry>,
) -> Result<(), HostErrorDto> {
    let mut paths: Vec<PathBuf> = fs::read_dir(directory)
        .map_err(|error| language_error("read", error.to_string()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_language_files(root, &path, entries)?;
        } else if path.is_file() {
            let relative: &Path = path.strip_prefix(root).expect("语言文件必须位于语言目录内");
            let name: String = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let bytes: Vec<u8> =
                fs::read(&path).map_err(|error| language_error("read", error.to_string()))?;
            entries.push(NlangPackageEntry::new(name, bytes));
        }
    }
    Ok(())
}

fn language_error(kind: &str, message: impl Into<String>) -> HostErrorDto {
    HostErrorDto::new(&format!("tui_host.language_{kind}"), message)
}

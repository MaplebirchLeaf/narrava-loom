//! TUI Host 的文件系统平台服务。

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use narrava_loom_core::{
    GameIdentity, ProjectConfig,
    i18n::{NlangPackageEntry, NlangPackageInput, NlangValidatedPackage},
    package_zip,
};
use narrava_loom_protocol::{HostErrorDto, SaveOperation};

const MAX_PACKAGE_BYTES: usize = 512 << 20;
const MAX_SAVE_BYTES: u64 = 16 << 20;

pub(crate) fn process_save(
    game_path: &Path,
    operation: SaveOperation,
    target: &str,
    document: Option<Vec<u8>>,
) -> Result<Option<Vec<u8>>, HostErrorDto> {
    let file_name: String = save_file_name(target)?;
    let save_directory: PathBuf = game_path.join("save");
    let path: PathBuf = save_directory.join(file_name);
    match operation {
        SaveOperation::Export => {
            fs::create_dir_all(&save_directory).map_err(|error| save_error(error.to_string()))?;
            let bytes: &[u8] = document
                .as_deref()
                .ok_or_else(|| save_error("Save export 缺少存档内容"))?;
            write_atomically(&path, bytes).map_err(|error| save_error(error.to_string()))?;
            Ok(None)
        }
        SaveOperation::Import => read_limited(&path)
            .map(Some)
            .map_err(|error| save_error(error.to_string())),
    }
}

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

fn save_file_name(target: &str) -> Result<String, HostErrorDto> {
    if target.is_empty()
        || target.len() > 80
        || !target
            .chars()
            .all(|value: char| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
    {
        return Err(HostErrorDto::new(
            "tui_host.save_target",
            "Save target 只允许 1 至 80 个 ASCII 字母、数字、连字符或下划线",
        ));
    }
    Ok(format!("{target}.nsave"))
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary: PathBuf = path.with_extension("nsave.tmp");
    let mut file: File = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    replace_file(&temporary, path)
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(windows)]
fn replace_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    let backup: PathBuf = path.with_extension("nsave.bak");
    if path.exists() {
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(temporary, path) {
        if backup.exists() {
            let _restored: Result<(), _> = fs::rename(&backup, path);
        }
        return Err(error);
    }
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    Ok(())
}

fn read_limited(path: &Path) -> std::io::Result<Vec<u8>> {
    let file: File = File::open(path)?;
    let length: u64 = file.metadata()?.len();
    if length > MAX_SAVE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "存档超过 16 MiB 上限",
        ));
    }
    let mut bytes: Vec<u8> = Vec::with_capacity(usize::try_from(length).unwrap_or(0));
    file.take(MAX_SAVE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SAVE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "存档超过 16 MiB 上限",
        ));
    }
    Ok(bytes)
}

fn save_error(message: impl Into<String>) -> HostErrorDto {
    HostErrorDto::new("tui_host.save", message)
}

fn language_error(kind: &str, message: impl Into<String>) -> HostErrorDto {
    HostErrorDto::new(&format!("tui_host.language_{kind}"), message)
}

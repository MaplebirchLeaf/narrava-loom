//! `Save.export/import` 与发行目录 `save/` 的平台落盘边界。

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

use narrava_loom_protocol::{HostErrorDto, SaveOperation};

const MAX_SAVE_BYTES: u64 = 16 << 20;

/// Host 只执行文件 IO；存档捕获、验证与恢复均由 Runtime 完成。
pub(crate) fn process_save_io(
    game_path: &Path,
    operation: SaveOperation,
    target: &str,
    document: Option<Vec<u8>>,
    host: &str,
) -> Result<Option<Vec<u8>>, HostErrorDto> {
    let file_name: String = save_file_name(target)
        .map_err(|message| HostErrorDto::new(&format!("{host}.save_target"), message))?;
    let save_directory = game_path.join("save");
    let path = save_directory.join(file_name);
    let save_error = |message: String| HostErrorDto::new(&format!("{host}.save"), message);
    match operation {
        SaveOperation::Export => {
            fs::create_dir_all(&save_directory).map_err(|error| save_error(error.to_string()))?;
            let bytes: &[u8] = document
                .as_deref()
                .ok_or_else(|| save_error(String::from("Save export 缺少存档内容")))?;
            write_atomically(&path, bytes).map_err(|error| save_error(error.to_string()))?;
            Ok(None)
        }
        SaveOperation::Import => read_limited(&path)
            .map(Some)
            .map_err(|error| save_error(error.to_string())),
    }
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("nsave.tmp");
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
    let backup = path.with_extension("nsave.bak");
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

/// 校验并规范化存档目标为 `<target>.nsave` 文件名（禁止路径逃逸）。
pub(crate) fn save_file_name(target: &str) -> Result<String, &'static str> {
    if target.is_empty()
        || target.len() > 80
        || !target
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
    {
        return Err("Save target 只允许 1 至 80 个 ASCII 字母、数字、连字符或下划线");
    }
    Ok(format!("{target}.nsave"))
}

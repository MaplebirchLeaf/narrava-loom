//! `.nar`、`.nlang` 与 `.nres` 共用的确定性 ZIP 边界。
//!
//! 领域模块只拥有规范文件清单；这里负责字节容器，不把 ZIP 类型泄漏到 Core
//! 编译、I18n 或 Resource API。

use std::io::{Cursor, Read, Write};

use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

/// 把已经规范化并排序的文件编码为 ZIP；固定时间和权限保证相同输入得到相同字节。
pub fn encode(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Result<Vec<u8>, String> {
    let mut files: Vec<(String, Vec<u8>)> = files.into_iter().collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (path, bytes) in files {
        writer
            .start_file(path, options)
            .map_err(|error| error.to_string())?;
        writer
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
    }
    writer
        .finish()
        .map(Cursor::into_inner)
        .map_err(|error| error.to_string())
}

/// 解码不可信 ZIP，并拒绝目录项、非规范路径、重复项和过大的展开内容。
pub fn decode(bytes: &[u8], expanded_limit: usize) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let mut files = Vec::with_capacity(archive.len());
    let mut total = 0_usize;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| error.to_string())?;
        let path = file.name().to_owned();
        if file.is_dir()
            || path.is_empty()
            || path.starts_with('/')
            || path.contains('\\')
            || path
                .split('/')
                .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
        {
            return Err(format!("ZIP 包含不安全路径：{path}"));
        }
        total = total
            .checked_add(file.size() as usize)
            .ok_or_else(|| String::from("ZIP 展开大小溢出"))?;
        if total > expanded_limit {
            return Err(format!("ZIP 展开内容超过限制：{expanded_limit}"));
        }
        let mut data = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut data)
            .map_err(|error| error.to_string())?;
        files.push((path, data));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    if files.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(String::from("ZIP 包含重复路径"));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn zip_is_deterministic_and_rejects_unsafe_paths() {
        let files = vec![
            (String::from("b.txt"), vec![2]),
            (String::from("a.txt"), vec![1]),
        ];
        let first = encode(files.clone()).unwrap();
        assert_eq!(first, encode(files).unwrap());
        assert_eq!(decode(&first, 2).unwrap()[0].0, "a.txt");

        let unsafe_zip = encode(vec![(String::from("../bad"), vec![1])]).unwrap();
        assert!(decode(&unsafe_zip, 10).is_err());
    }
}

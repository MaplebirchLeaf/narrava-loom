//! 面向游戏作者的可移动发行目录构建器。

use std::{fs, path::Path};

use crate::{
    ProjectConfig, SourceList,
    i18n::{NlangPackageEntry, NlangPackageInput},
    nar::NarPackage,
    package_zip,
    resource::{NresPackage, ResourceCatalog, ResourceInput},
};

/// 构建可直接分发的 `NarravaGame/`。目标必须尚不存在，避免覆盖作者文件。
pub fn build_directory(project: &Path, output: &Path, host_binary: &Path) -> Result<(), String> {
    if output.exists() {
        return Err(format!("发行目录已经存在：{}", output.display()));
    }
    if !host_binary.is_file() {
        return Err(format!("找不到 Tauri Host：{}", host_binary.display()));
    }

    let config_text = fs::read_to_string(project.join("config.toml"))
        .map_err(|error| format!("读取 config.toml 失败：{error}"))?;
    let config = ProjectConfig::parse(&project.join("config.toml"), &config_text)
        .map_err(|error| error.to_string())?;
    let sources =
        SourceList::discover(project).map_err(|error| format!("读取源码失败：{error}"))?;
    let resources = ResourceCatalog::discover(project).map_err(|error| error.to_string())?;
    let mut resources = embed_author_styles(project, resources)?;
    if let Some(icon) = tauri_icon(&config_text)?
        && !resources.has(&icon)
    {
        let mut inputs: Vec<ResourceInput> =
            resources.inputs().map_err(|error| error.to_string())?;
        let bytes = fs::read(project.join(&icon))
            .map_err(|error| format!("读取 Tauri 图标 {icon} 失败：{error}"))?;
        inputs.push(ResourceInput::new(icon, bytes));
        resources = ResourceCatalog::new(inputs).map_err(|error| error.to_string())?;
    }
    let nar = NarPackage::build_release(&config, &sources, &resources, &config_text)
        .map_err(|error| error.to_string())?;
    let nar_bytes = package_zip::encode(nar.into_files())?;
    let resource_bytes = if resources.is_empty() {
        None
    } else {
        let package = NresPackage::build(&resources).map_err(|error| format!("{error:?}"))?;
        Some(package_zip::encode(package.files())?)
    };
    let languages = build_languages(project, &config)?;

    fs::create_dir_all(output.join("languages")).map_err(io_error)?;
    fs::create_dir_all(output.join("resources")).map_err(io_error)?;
    fs::create_dir_all(output.join("mods")).map_err(io_error)?;
    fs::create_dir_all(output.join("save")).map_err(io_error)?;
    fs::copy(host_binary, output.join(host_name())).map_err(io_error)?;
    make_executable(&output.join(host_name()))?;
    fs::write(output.join("game.nar"), nar_bytes).map_err(io_error)?;
    if let Some(bytes) = resource_bytes {
        fs::write(output.join("resources/base.nres"), bytes).map_err(io_error)?;
    }
    for (name, bytes) in languages {
        fs::write(output.join("languages").join(name), bytes).map_err(io_error)?;
    }
    copy_packages(&project.join("mods"), &output.join("mods"), "nmod")?;
    Ok(())
}

/// 把 `languages/` 下每个语言目录打包为 `<locale>.nlang`，并在打包前完成安装校验。
fn build_languages(
    project: &Path,
    config: &ProjectConfig,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let root = project.join("languages");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut packages = Vec::new();
    for entry in fs::read_dir(&root).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if !entry.file_type().map_err(io_error)?.is_dir() {
            continue;
        }
        let locale = entry
            .file_name()
            .into_string()
            .map_err(|_| String::from("语言目录名必须是 UTF-8"))?;
        let mut entries = Vec::new();
        for file in fs::read_dir(entry.path()).map_err(io_error)? {
            let file = file.map_err(io_error)?;
            if file.file_type().map_err(io_error)?.is_file() {
                let name = file
                    .file_name()
                    .into_string()
                    .map_err(|_| String::from("语言包文件名必须是 UTF-8"))?;
                entries.push(NlangPackageEntry::new(
                    name,
                    fs::read(file.path()).map_err(io_error)?,
                ));
            }
        }
        NlangPackageInput::new(entries.clone())
            .validate(
                &locale,
                &config.identity().map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        let files = entries
            .into_iter()
            .map(|entry| (entry.path().to_owned(), entry.bytes().to_vec()));
        packages.push((format!("{locale}.nlang"), package_zip::encode(files)?));
    }
    packages.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(packages)
}

/// 把 `styles/` 下的作者 CSS 作为 `__narrava/styles/` 资源并入目录。
fn embed_author_styles(
    project: &Path,
    resources: ResourceCatalog,
) -> Result<ResourceCatalog, String> {
    let style_root = project.join("styles");
    if !style_root.exists() {
        return Ok(resources);
    }
    let mut inputs: Vec<ResourceInput> = resources.inputs().map_err(|error| error.to_string())?;
    collect_author_styles(&style_root, &style_root, &mut inputs)?;
    ResourceCatalog::new(inputs).map_err(|error| error.to_string())
}

/// 递归收集 `styles/` 下的 CSS 文件为逻辑路径资源。
fn collect_author_styles(
    root: &Path,
    directory: &Path,
    inputs: &mut Vec<ResourceInput>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let file_type = entry.file_type().map_err(io_error)?;
        if file_type.is_dir() {
            collect_author_styles(root, &entry.path(), inputs)?;
        } else if file_type.is_file()
            && entry.path().extension().is_some_and(|value| value == "css")
        {
            let relative = entry
                .path()
                .strip_prefix(root)
                .expect("Style 必须位于发现根目录")
                .components()
                .map(|component| {
                    component
                        .as_os_str()
                        .to_str()
                        .ok_or_else(|| String::from("Style 逻辑路径必须是 UTF-8"))
                })
                .collect::<Result<Vec<_>, _>>()?
                .join("/");
            let path = format!("__narrava/styles/{relative}");
            inputs.push(ResourceInput::new(
                path,
                fs::read(entry.path()).map_err(io_error)?,
            ));
        }
    }
    Ok(())
}

/// 从作者配置的 `[host.tauri] icon` 字段读取可选图标路径。
fn tauri_icon(config: &str) -> Result<Option<String>, String> {
    let value: toml::Value = toml::from_str(config).map_err(|error| error.to_string())?;
    Ok(value
        .get("host")
        .and_then(|value| value.get("tauri"))
        .and_then(|value| value.get("icon"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned))
}

/// 复制源目录中指定扩展名的文件到目标目录（不递归）。
fn copy_packages(source: &Path, target: &Path, extension: &str) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(source).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if entry.file_type().map_err(io_error)?.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|value| value == extension)
        {
            fs::copy(entry.path(), target.join(entry.file_name())).map_err(io_error)?;
        }
    }
    Ok(())
}

/// 当前平台下 Host 可执行文件名。
fn host_name() -> &'static str {
    if cfg!(windows) {
        "narrava.exe"
    } else {
        "narrava"
    }
}

/// Unix 下为 Host 可执行文件添加可执行权限。
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).map_err(io_error)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(io_error)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// 把 I/O 错误转换为发行构建的错误文本。
fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

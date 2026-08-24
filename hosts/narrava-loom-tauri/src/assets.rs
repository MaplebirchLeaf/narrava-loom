use std::{fs, io, path::Path};

use narrava_loom_core::resource::ResourceCatalog;
use serde::Serialize;

use crate::HostErrorDto;

const PACKAGED_STYLE_PREFIX: &str = "__narrava/styles/";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HostStyleDto {
    pub path: String,
    pub css: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HostResourceDto {
    pub path: String,
    pub media_type: String,
    pub size: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct HostAssetsDto {
    pub title: String,
    pub styles: Vec<HostStyleDto>,
    pub resources: Vec<HostResourceDto>,
}

impl HostAssetsDto {
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

    pub fn discover(game_path: &Path) -> Result<Self, HostErrorDto> {
        let resources = ResourceCatalog::discover(game_path)
            .map_err(|error| HostErrorDto::new("tauri_host.resource", error.to_string()))?;
        Self::discover_with_catalog(game_path, &resources)
    }

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

fn read_error(path: &Path, error: io::Error) -> HostErrorDto {
    HostErrorDto::new(
        "tauri_host.asset_read",
        format!("{}：{error}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::HostAssetsDto;

    #[test]
    fn host_assets_discover_sorted_css_and_resource_metadata_without_bytes() {
        let root = PathBuf::from(format!(
            "target/test-projects/narrava-loom-tauri-assets-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("styles/theme")).unwrap();
        fs::create_dir_all(root.join("resources/img")).unwrap();
        fs::write(root.join("styles/z.css"), "nv-story { color: red; }").unwrap();
        fs::write(root.join("styles/theme/a.css"), "nv-story { color: blue; }").unwrap();
        fs::write(root.join("styles/ignored.txt"), "ignored").unwrap();
        fs::write(root.join("resources/img/a.png"), [1, 2, 3]).unwrap();

        let assets = HostAssetsDto::discover(&root).unwrap();

        assert_eq!(
            assets
                .styles
                .iter()
                .map(|style| style.path.as_str())
                .collect::<Vec<_>>(),
            vec!["theme/a.css", "z.css"]
        );
        assert_eq!(assets.resources[0].path, "img/a.png");
        assert_eq!(assets.resources[0].media_type, "image/png");
        assert_eq!(assets.resources[0].size, 3);
        assert!(
            !serde_json::to_value(&assets).unwrap()["resources"][0]
                .as_object()
                .unwrap()
                .contains_key("bytes")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn packaged_host_styles_are_restored_from_the_reserved_resource_namespace() {
        let resources = narrava_loom_core::resource::ResourceCatalog::new([
            narrava_loom_core::resource::ResourceInput::new(
                "__narrava/styles/theme/main.css",
                b"nv-story { color: wheat; }".to_vec(),
            ),
            narrava_loom_core::resource::ResourceInput::new("images/scene.svg", b"<svg/>".to_vec()),
        ])
        .unwrap();

        let assets = HostAssetsDto::from_catalog(&resources).unwrap();

        assert_eq!(assets.styles.len(), 1);
        assert_eq!(assets.styles[0].path, "theme/main.css");
        assert_eq!(assets.styles[0].css, "nv-story { color: wheat; }");
        assert_eq!(assets.resources.len(), 1);
        assert_eq!(assets.resources[0].path, "images/scene.svg");
    }
}

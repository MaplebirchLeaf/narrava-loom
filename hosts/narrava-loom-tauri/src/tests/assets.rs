//! `assets` 的 Host 资源清单测试（原内联于 assets.rs，按源码规范收拢）。

use std::{fs, path::PathBuf};

use crate::assets::HostAssetsDto;

/// 开发目录发现按路径排序的 CSS 与 Resource 元数据，且不携带字节。
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

/// 发行包内保留命名空间的样式表被还原为 Host styles。
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

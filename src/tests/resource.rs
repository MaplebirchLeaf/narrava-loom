use std::{fs, path::PathBuf};

use crate::resource::{ResourceCatalog, ResourceError, ResourceInput, ResourcePath};

#[test]
fn resource_path_accepts_only_normalized_relative_logical_paths() {
    let path = ResourcePath::parse("img/forest.webp").expect("普通资源路径应有效");
    assert_eq!(path.as_str(), "img/forest.webp");

    for invalid in [
        "",
        "/img/a.png",
        "img//a.png",
        "img/./a.png",
        "img/../a.png",
        "img\\a.png",
    ] {
        assert_eq!(
            ResourcePath::parse(invalid).unwrap_err(),
            ResourceError::InvalidPath(invalid.to_owned())
        );
    }
}

#[test]
fn resource_catalog_keeps_stable_paths_bytes_media_types_and_candidate_order() {
    let catalog = ResourceCatalog::new([
        ResourceInput::new("data/scene.json", br#"{"night":true}"#.to_vec()),
        ResourceInput::new("img/forest.webp", vec![1, 2, 3]),
        ResourceInput::with_media_type("unknown/data.bin", "model/gltf-binary", vec![4, 5]),
    ])
    .expect("资源目录应建立");

    assert_eq!(
        catalog.paths().collect::<Vec<_>>(),
        vec!["data/scene.json", "img/forest.webp", "unknown/data.bin"]
    );
    assert!(catalog.has("img/forest.webp"));
    assert_eq!(
        catalog.read("img/forest.webp").unwrap(),
        Some(&[1, 2, 3][..])
    );
    assert_eq!(
        catalog.text("data/scene.json").expect("JSON 应为 UTF-8"),
        Some(r#"{"night":true}"#)
    );
    assert_eq!(
        catalog
            .info("img/forest.webp")
            .expect("资源信息应存在")
            .media_type(),
        "image/webp"
    );
    assert_eq!(
        catalog
            .info("unknown/data.bin")
            .expect("显式媒体类型应存在")
            .media_type(),
        "model/gltf-binary"
    );
    assert_eq!(
        catalog.pick(["missing.png", "img/forest.webp", "data/scene.json"]),
        Some("img/forest.webp")
    );
}

#[test]
fn resource_catalog_rejects_duplicates_and_reports_non_utf8_text() {
    assert_eq!(
        ResourceCatalog::new([
            ResourceInput::new("data/value.bin", vec![0xff]),
            ResourceInput::new("data/value.bin", vec![0]),
        ])
        .unwrap_err(),
        ResourceError::DuplicatePath(String::from("data/value.bin"))
    );

    let catalog = ResourceCatalog::new([ResourceInput::new("data/value.bin", vec![0xff])])
        .expect("二进制资源应可进入目录");
    assert_eq!(
        catalog.text("data/value.bin").unwrap_err(),
        ResourceError::InvalidUtf8(String::from("data/value.bin"))
    );
}

#[test]
fn resource_discovery_reads_optional_resources_directory_in_stable_order() {
    let root = unique_temp_dir("resource-discovery");
    fs::create_dir_all(root.join("resources/img")).expect("测试目录应建立");
    fs::create_dir_all(root.join("resources/data")).expect("测试目录应建立");
    fs::write(root.join("resources/img/z.png"), [3, 2, 1]).expect("测试资源应写入");
    fs::write(root.join("resources/data/a.txt"), "hello").expect("测试资源应写入");

    let catalog = ResourceCatalog::discover(&root).expect("资源目录应可发现");

    assert_eq!(
        catalog.paths().collect::<Vec<_>>(),
        vec!["data/a.txt", "img/z.png"]
    );
    assert_eq!(catalog.text("data/a.txt").unwrap(), Some("hello"));

    fs::remove_dir_all(root).expect("测试目录应清理");
}

#[test]
fn resource_discovery_defers_file_reads_until_the_resource_is_opened() {
    let root = unique_temp_dir("resource-lazy-read");
    let file = root.join("resources/data/later.txt");
    fs::create_dir_all(file.parent().unwrap()).expect("测试目录应建立");
    fs::write(&file, "later").expect("测试资源应写入");

    let catalog = ResourceCatalog::discover(&root).expect("发现阶段只读取资源元数据");
    assert_eq!(catalog.info("data/later.txt").unwrap().size(), 5);
    fs::remove_file(&file).expect("测试资源应删除");

    assert!(matches!(
        catalog.read("data/later.txt"),
        Err(ResourceError::Read { .. })
    ));
    fs::remove_dir_all(root).expect("测试目录应清理");
}

#[test]
fn resource_file_bytes_are_cached_after_the_first_successful_read() {
    let root = unique_temp_dir("resource-read-cache");
    let file = root.join("resources/data/cached.txt");
    fs::create_dir_all(file.parent().unwrap()).expect("测试目录应建立");
    fs::write(&file, "cached").expect("测试资源应写入");

    let catalog = ResourceCatalog::discover(&root).expect("资源目录应可发现");
    assert_eq!(
        catalog.read("data/cached.txt").unwrap(),
        Some(b"cached".as_slice())
    );
    fs::remove_file(&file).expect("测试资源应删除");
    assert_eq!(
        catalog.read("data/cached.txt").unwrap(),
        Some(b"cached".as_slice())
    );

    fs::remove_dir_all(root).expect("测试目录应清理");
}

#[test]
fn missing_resources_directory_is_an_empty_catalog() {
    let root = unique_temp_dir("resource-empty");
    fs::create_dir_all(&root).expect("测试目录应建立");

    let catalog = ResourceCatalog::discover(&root).expect("resources 可省略");

    assert!(catalog.is_empty());
    fs::remove_dir_all(root).expect("测试目录应清理");
}

#[cfg(unix)]
#[test]
fn resource_discovery_rejects_non_utf8_logical_paths() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let root = unique_temp_dir("resource-non-utf8");
    let resources = root.join("resources");
    fs::create_dir_all(&resources).expect("测试目录应建立");
    fs::write(resources.join(OsString::from_vec(vec![b'x', 0xff])), [1]).expect("测试资源应写入");

    assert!(matches!(
        ResourceCatalog::discover(&root),
        Err(ResourceError::InvalidPath(_))
    ));
    fs::remove_dir_all(root).expect("测试目录应清理");
}

fn unique_temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "narrava-loom-{name}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

// NresPackage 打包往返（原内联于 resource/package.rs，按源码规范收拢）。

use crate::resource::NresPackage;

#[test]
fn nres_round_trips_resource_bytes_and_media_type() {
    let catalog = ResourceCatalog::new([ResourceInput::with_media_type(
        "guide.txt",
        "text/plain",
        b"hello".to_vec(),
    )])
    .unwrap();
    let package = NresPackage::build(&catalog).unwrap();
    let decoded = NresPackage::from_files(package.files().collect())
        .unwrap()
        .validate()
        .unwrap();
    assert_eq!(decoded.text("guide.txt").unwrap(), Some("hello"));
    assert_eq!(
        decoded.info("guide.txt").unwrap().media_type(),
        "text/plain"
    );
}

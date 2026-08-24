use crate::{
    ProjectConfig, SourceList,
    nar::{NarPackage, NarPackageError, NarSourceArchive, NarSourceError},
    resource::{ResourceCatalog, ResourceInput},
};

#[test]
fn nar_source_archive_round_trips_owned_core_sources() {
    let sources = SourceList::discover("src/tests/fixtures/game").expect("fixture 应可读取");

    let archive = NarSourceArchive::from_sources(&sources);
    let encoded = archive.to_json().expect("源码记录应可编码");
    let decoded = NarSourceArchive::from_json(&encoded).expect("源码记录应可解码");
    let restored = decoded.into_sources().expect("源码记录应可恢复");

    assert_eq!(restored.items.len(), sources.items.len());
    for (actual, expected) in restored.items.iter().zip(&sources.items) {
        assert_eq!(actual.path.as_str(), expected.path.as_str());
        assert_eq!(actual.kind, expected.kind);
        assert_eq!(actual.content, expected.content);
    }
}

#[test]
fn nar_source_archive_rejects_duplicate_saved_paths() {
    let encoded = r#"{"sources":[
        {"path":"story/main.twee","kind":"twee","content":":: Start"},
        {"path":"story/main.twee","kind":"twee","content":":: Other"}
    ]}"#;

    let error = match NarSourceArchive::from_json(encoded).and_then(NarSourceArchive::into_sources)
    {
        Ok(_) => panic!("重复保存路径必须拒绝"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        NarSourceError::DuplicatePath(String::from("story/main.twee"))
    );
}

#[test]
fn nar_source_archive_rejects_kind_that_disagrees_with_path() {
    let encoded = r#"{"sources":[
        {"path":"scripts/main.ts","kind":"javascript","content":"export {}"}
    ]}"#;

    let error = match NarSourceArchive::from_json(encoded).and_then(NarSourceArchive::into_sources)
    {
        Ok(_) => panic!("路径和类型不一致必须拒绝"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        NarSourceError::KindMismatch(String::from("scripts/main.ts"))
    );
}

#[test]
fn nar_package_validates_manifest_identity_and_source_hash() {
    let config = ProjectConfig::load("src/tests/fixtures/game").expect("fixture config 应可读取");
    let sources = SourceList::discover("src/tests/fixtures/game").expect("fixture 应可读取");

    let package = NarPackage::build(&config, &sources).expect("NAR 应可构建");
    let validated = package.validate().expect("Core 产物应通过自身校验");

    assert_eq!(validated.game().id(), config.game.id);
    assert_eq!(validated.game().version().to_string(), config.game.version);
    assert_eq!(validated.sources().items.len(), sources.items.len());
    validated
        .with_bytecode(|bytecode, scripts| {
            assert!(bytecode.passage("Start").is_some());
            assert!(!scripts.is_empty());
        })
        .expect("已校验 NAR 应可建立 VM 输入");
}

#[test]
fn nar_package_rejects_tampered_source_record() {
    let config = ProjectConfig::load("src/tests/fixtures/game").expect("fixture config 应可读取");
    let sources = SourceList::discover("src/tests/fixtures/game").expect("fixture 应可读取");
    let mut package = NarPackage::build(&config, &sources).expect("NAR 应可构建");

    package
        .file_mut("sources.json")
        .expect("源码记录必须存在")
        .push(b' ');

    assert_eq!(
        package.validate().unwrap_err(),
        NarPackageError::HashMismatch
    );
}

#[test]
fn nar_package_rejects_wrong_package_type() {
    let config = ProjectConfig::load("src/tests/fixtures/game").expect("fixture config 应可读取");
    let sources = SourceList::discover("src/tests/fixtures/game").expect("fixture 应可读取");
    let mut package = NarPackage::build(&config, &sources).expect("NAR 应可构建");
    let manifest = package.file_mut("manifest.json").expect("清单必须存在");
    let text = String::from_utf8(manifest.clone()).expect("清单必须是 UTF-8");
    *manifest = text.replace("narrava-game", "ordinary-zip").into_bytes();

    assert_eq!(
        package.validate().unwrap_err(),
        NarPackageError::WrongPackageType
    );
}

#[test]
fn nar_package_rejects_tampered_owned_bytecode() {
    let config = ProjectConfig::load("src/tests/fixtures/game").expect("fixture config 应可读取");
    let sources = SourceList::discover("src/tests/fixtures/game").expect("fixture 应可读取");
    let mut package = NarPackage::build(&config, &sources).expect("NAR 应可构建");

    package
        .file_mut("bytecode.json")
        .expect("Bytecode 记录必须存在")
        .push(b' ');

    assert_eq!(
        package.validate().unwrap_err(),
        NarPackageError::HashMismatch
    );
}

#[test]
fn nar_package_round_trips_through_an_external_zip_file_boundary() {
    let config = ProjectConfig::load("src/tests/fixtures/game").expect("fixture config 应可读取");
    let sources = SourceList::discover("src/tests/fixtures/game").expect("fixture 应可读取");
    let files = NarPackage::build(&config, &sources)
        .expect("NAR 应可构建")
        .into_files();

    let restored = NarPackage::from_files(files).expect("解包后的文件集应可重新进入 Core");
    let validated = restored.validate().expect("往返 NAR 应保持有效");

    assert_eq!(validated.game().id(), config.game.id);
    assert!(validated.bytecode().passage("Start").is_some());
}

#[test]
fn nar_package_rejects_duplicate_or_unknown_unpacked_entries() {
    assert_eq!(
        NarPackage::from_files([
            (String::from("manifest.json"), Vec::new()),
            (String::from("manifest.json"), Vec::new()),
        ])
        .unwrap_err(),
        NarPackageError::DuplicatePath(String::from("manifest.json"))
    );
    assert_eq!(
        NarPackage::from_files([(String::from("../outside"), Vec::new())]).unwrap_err(),
        NarPackageError::InvalidPath(String::from("../outside"))
    );
    assert_eq!(
        NarPackage::from_files([(String::from("extra.json"), Vec::new())]).unwrap_err(),
        NarPackageError::UnknownFile(String::from("extra.json"))
    );
}

#[test]
fn nar_package_round_trips_owned_resources_with_media_types() {
    let config = ProjectConfig::load("src/tests/fixtures/game").expect("fixture config 应可读取");
    let sources = SourceList::discover("src/tests/fixtures/game").expect("fixture 应可读取");
    let resources = ResourceCatalog::new([
        ResourceInput::new("img/forest.webp", vec![1, 2, 3]),
        ResourceInput::with_media_type("models/room.bin", "model/gltf-binary", vec![4, 5]),
    ])
    .expect("测试资源应有效");

    let files = NarPackage::build_with_resources(&config, &sources, &resources)
        .expect("带资源 NAR 应可构建")
        .into_files();
    let package = NarPackage::from_files(files).expect("解包文件应可进入 Core");
    let validated = package.validate().expect("带资源 NAR 应通过校验");

    assert_eq!(
        validated.resources().paths().collect::<Vec<_>>(),
        vec!["img/forest.webp", "models/room.bin"]
    );
    assert_eq!(
        validated.resources().read("img/forest.webp").unwrap(),
        Some(&[1, 2, 3][..])
    );
    assert_eq!(
        validated
            .resources()
            .info("models/room.bin")
            .unwrap()
            .media_type(),
        "model/gltf-binary"
    );
}

#[test]
fn nar_package_rejects_tampered_or_unlisted_resource_files() {
    let config = ProjectConfig::load("src/tests/fixtures/game").expect("fixture config 应可读取");
    let sources = SourceList::discover("src/tests/fixtures/game").expect("fixture 应可读取");
    let resources = ResourceCatalog::new([ResourceInput::new("img/forest.webp", vec![1, 2, 3])])
        .expect("测试资源应有效");
    let mut package = NarPackage::build_with_resources(&config, &sources, &resources)
        .expect("带资源 NAR 应可构建");
    package
        .file_mut("resources/img/forest.webp")
        .expect("资源文件应存在")
        .push(4);
    assert_eq!(
        package.validate().unwrap_err(),
        NarPackageError::ResourceHashMismatch(String::from("img/forest.webp"))
    );

    let package = NarPackage::from_files([(String::from("resources/../escape"), Vec::new())]);
    assert_eq!(
        package.unwrap_err(),
        NarPackageError::InvalidPath(String::from("resources/../escape"))
    );
}

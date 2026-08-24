//! Source 与 SourceList 行为测试。

use std::{
    io,
    path::{Path, PathBuf},
};

use crate::source::{Source, SourceError, SourceKind, SourceList, SourcePath};

#[test]
fn load_error_keeps_source_path() {
    let root: PathBuf = PathBuf::from("src/tests/fixtures/game");
    let path: PathBuf = PathBuf::from("missing.twee");
    let result: Result<Source, SourceError> = Source::load(&root, &path);
    let error: SourceError = match result {
        Ok(_) => panic!("缺失文件不应读取成功"),
        Err(error) => error,
    };

    assert_eq!(error.path, root.join("contents").join(path));
}

#[test]
fn keeps_path_relative_to_content_root() {
    let root: PathBuf = PathBuf::from("src/tests/fixtures/game");
    let path: PathBuf = PathBuf::from("story/main.twee");
    let source: Source = Source::load(&root, &path).expect("示例源码应可读取");

    assert_eq!(source.path.as_str(), "story/main.twee");
    assert_eq!(source.kind, SourceKind::Twee);
}

#[test]
fn rejects_parent_path() {
    let root: PathBuf = PathBuf::from("src/tests/fixtures/game");
    let path: PathBuf = PathBuf::from("../outside.twee");
    let result: Result<Source, SourceError> = Source::load(&root, &path);
    let error: SourceError = match result {
        Ok(_) => panic!("父级路径必须被拒绝"),
        Err(error) => error,
    };

    assert_eq!(error.source.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn rejects_absolute_path() {
    let root: PathBuf = PathBuf::from("src/tests/fixtures/game");
    let path: PathBuf = PathBuf::from("/outside.twee");
    let result: Result<Source, SourceError> = Source::load(&root, &path);
    let error: SourceError = match result {
        Ok(_) => panic!("绝对路径必须被拒绝"),
        Err(error) => error,
    };

    assert_eq!(error.source.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn rejects_absolute_project_root() {
    let root: PathBuf = PathBuf::from("/game");
    let path: PathBuf = PathBuf::from("story/main.twee");
    let result: Result<Source, SourceError> = Source::load(&root, &path);
    let error: SourceError = match result {
        Ok(_) => panic!("绝对游戏目录必须被拒绝"),
        Err(error) => error,
    };

    assert_eq!(error.source.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn rejects_unsupported_source_kind() {
    let project: PathBuf = PathBuf::from("src/tests/fixtures/game");
    let path: PathBuf = PathBuf::from("story/main.txt");
    let result: Result<Source, SourceError> = Source::load(&project, &path);
    let error: SourceError = match result {
        Ok(_) => panic!("未知源码类型必须被拒绝"),
        Err(error) => error,
    };

    assert_eq!(error.source.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn recognizes_typescript_source() {
    let path: SourcePath = SourcePath::from_path(Path::new("scripts/main.ts")).expect("路径应有效");
    let kind: SourceKind = SourceKind::from_path(&path).expect("TypeScript 应受支持");

    assert_eq!(kind, SourceKind::TypeScript);
}

#[test]
fn recognizes_javascript_source() {
    let path: SourcePath = SourcePath::from_path(Path::new("scripts/main.js")).expect("路径应有效");
    let kind: SourceKind = SourceKind::from_path(&path).expect("JavaScript 应受支持");

    assert_eq!(kind, SourceKind::JavaScript);
}

#[test]
fn rejects_css_as_a_core_source() {
    let path: SourcePath = SourcePath::from_path(Path::new("styles/main.css")).expect("路径应有效");
    let result: Result<SourceKind, io::Error> = SourceKind::from_path(&path);

    let error: io::Error = result.expect_err("CSS 不应进入 Narrava Core Source");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn loads_sources_in_given_order() {
    let project: PathBuf = PathBuf::from("src/tests/fixtures/game");
    let paths: Vec<PathBuf> = vec![
        PathBuf::from("story/main.twee"),
        PathBuf::from("scripts/main.ts"),
    ];
    let sources: SourceList = SourceList::load(&project, &paths).expect("源码列表应可读取");

    assert_eq!(sources.items.len(), 2);
    assert_eq!(sources.items[0].kind, SourceKind::Twee);
    assert_eq!(sources.items[1].kind, SourceKind::TypeScript);
}

#[test]
fn discovers_sources_in_stable_path_order() {
    let project: PathBuf = PathBuf::from("src/tests/fixtures/game");
    let sources: SourceList = SourceList::discover(&project).expect("应能发现示例源码");
    let paths: Vec<&str> = sources
        .items
        .iter()
        .map(|source: &Source| source.path.as_str())
        .collect();

    assert_eq!(paths, vec!["scripts/main.ts", "story/main.twee"]);
}

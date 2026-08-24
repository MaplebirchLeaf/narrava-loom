//! 公共 Diagnostic 数据结构测试。

use crate::diagnostic::{
    Diagnostic, DiagnosticLocation, DiagnosticLocationError, DiagnosticLocator, DiagnosticSeverity,
};

#[test]
fn creates_diagnostic_without_source_location() {
    let diagnostic: Diagnostic = Diagnostic::new(
        "macro.missing_definition",
        DiagnosticSeverity::Error,
        "Macro `weather` 尚未注册",
    );

    assert_eq!(diagnostic.code, "macro.missing_definition");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.message, "Macro `weather` 尚未注册");
    assert_eq!(diagnostic.location, None);
}

#[test]
fn attaches_relative_source_location() {
    let location: DiagnosticLocation = DiagnosticLocation {
        source: "story/main.twee".to_owned(),
        start: 12,
        end: 24,
        line: 3,
        column: 1,
    };
    let diagnostic: Diagnostic = Diagnostic::new(
        "twee.unclosed_macro",
        DiagnosticSeverity::Warning,
        "Macro 缺少闭合符",
    )
    .with_location(location.clone());

    assert_eq!(diagnostic.location, Some(location));
}

#[test]
fn maps_fragment_span_to_unicode_source_location() {
    let content: &str = "标题\n位置：${$name}\n";
    let fragment_start: usize = content.find("$name").expect("测试片段应存在");
    let locator: DiagnosticLocator<'_> = DiagnosticLocator::new("story/main.twee", content);

    let location: DiagnosticLocation = locator
        .locate(fragment_start, 0, "$name".len())
        .expect("有效片段范围应能映射");

    assert_eq!(location.source, "story/main.twee");
    assert_eq!(location.start, fragment_start);
    assert_eq!(location.end, fragment_start + "$name".len());
    assert_eq!(location.line, 2);
    assert_eq!(location.column, 6);
}

#[test]
fn rejects_invalid_fragment_span() {
    let content: &str = "值：${$name}";
    let locator: DiagnosticLocator<'_> = DiagnosticLocator::new("story/main.twee", content);

    let reversed: DiagnosticLocationError =
        locator.locate(0, 4, 2).expect_err("逆序范围必须被拒绝");
    let split_unicode: DiagnosticLocationError = locator
        .locate(1, 0, 1)
        .expect_err("UTF-8 字符中间位置必须被拒绝");

    assert_eq!(reversed, DiagnosticLocationError::InvalidRange);
    assert_eq!(split_unicode, DiagnosticLocationError::InvalidUtf8Boundary);
}

#[test]
fn displays_one_stable_error_without_repeating_pipeline_stages() {
    let plain = Diagnostic::new(
        "engine.mir.missing_passage",
        DiagnosticSeverity::Error,
        "MIR 中缺少 Passage：Start",
    );
    assert_eq!(
        plain.to_string(),
        "[engine.mir.missing_passage] MIR 中缺少 Passage：Start"
    );

    let located = plain.with_location(DiagnosticLocation {
        source: "story/main.twee".to_owned(),
        start: 0,
        end: 5,
        line: 2,
        column: 3,
    });
    assert_eq!(
        located.to_string(),
        "story/main.twee:2:3: [engine.mir.missing_passage] MIR 中缺少 Passage：Start"
    );
}

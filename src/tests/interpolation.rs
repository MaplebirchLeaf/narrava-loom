//! 共享插值边界扫描测试。

use crate::interpolation::find_interpolation_end;

#[test]
fn skips_nested_delimiters_and_braces_inside_strings() {
    let source: &str = "{ value: \"Map}\", nested: [1, { active: true }] }.value} tail";
    let end: usize = find_interpolation_end(source, 0).expect("应找到最外层插值闭合花括号");

    assert_eq!(
        &source[..end],
        "{ value: \"Map}\", nested: [1, { active: true }] }.value"
    );
    assert_eq!(&source[end..], "} tail");
}

#[test]
fn returns_none_for_unclosed_interpolation() {
    assert_eq!(find_interpolation_end("{ value: 1 }", 0), None);
}

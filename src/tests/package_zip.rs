//! `package_zip` 的 ZIP 编解码测试（原内联于 src/package_zip.rs，按源码规范收拢）。

use crate::package_zip::{decode, encode};

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

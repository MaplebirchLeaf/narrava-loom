//! 共享存档 IO 的错误归属、覆盖写入与读取配额。

use crate::save_io::process_save_io;
use narrava_loom_protocol::SaveOperation::{Export, Import};
use std::{fs, path::PathBuf};

#[test]
fn save_io_preserves_host_errors_and_existing_document() {
    for host in ["tauri_host", "tui_host"] {
        let root: PathBuf =
            std::env::temp_dir().join(format!("narrava-save-io-{host}-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        for target in ["", "../outside", "a/b", "a\\b", "中文", &"a".repeat(81)] {
            let error = process_save_io(&root, Export, target, Some(vec![1]), host).unwrap_err();
            assert_eq!(error.code, format!("{host}.save_target"));
        }
        let target: String = "a".repeat(80);
        for bytes in [vec![1, 2, 3], vec![4]] {
            assert_eq!(
                process_save_io(&root, Export, &target, Some(bytes.clone()), host).unwrap(),
                None
            );
            assert_eq!(
                process_save_io(&root, Import, &target, None, host).unwrap(),
                Some(bytes)
            );
        }
        let error = process_save_io(&root, Export, &target, None, host).unwrap_err();
        assert_eq!(error.code, format!("{host}.save"));
        assert_eq!(error.message, "Save export 缺少存档内容");
        assert_eq!(
            process_save_io(&root, Import, &target, None, host).unwrap(),
            Some(vec![4])
        );
        let error = process_save_io(&root, Import, "missing", None, host).unwrap_err();
        assert_eq!(error.code, format!("{host}.save"));
        assert!(
            !root
                .join("save")
                .join(format!("{target}.nsave.tmp"))
                .exists()
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn save_io_accepts_limit_and_rejects_larger_file() {
    let root: PathBuf =
        std::env::temp_dir().join(format!("narrava-save-limit-{}", std::process::id()));
    fs::create_dir_all(root.join("save")).unwrap();
    let file = fs::File::create(root.join("save/large.nsave")).unwrap();
    file.set_len(16 << 20).unwrap();
    assert_eq!(
        process_save_io(&root, Import, "large", None, "tui_host")
            .unwrap()
            .unwrap()
            .len(),
        16 << 20
    );
    file.set_len((16 << 20) + 1).unwrap();
    let error = process_save_io(&root, Import, "large", None, "tui_host").unwrap_err();
    assert_eq!(error.code, "tui_host.save");
    assert_eq!(error.message, "存档超过 16 MiB 上限");
    drop(file);
    fs::remove_dir_all(root).unwrap();
}

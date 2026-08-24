fn main() {
    let output =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo 必须提供 OUT_DIR"));
    let icon = output.join("narrava-transparent-build-icon.png");
    // Tauri 代码生成必须有默认 PNG。使用 32×32 临时图可避开 Linux GTK 不接受 1×1 图标的问题，
    // 同时不要求仓库提交一个产品图标；游戏显式配置的图标仍会在创建窗口时覆盖它。
    std::fs::write(
        &icon,
        [
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 32, 0, 0, 0, 32,
            8, 6, 0, 0, 0, 115, 122, 122, 244, 0, 0, 0, 26, 73, 68, 65, 84, 120, 156, 237, 193, 1,
            1, 0, 0, 0, 130, 32, 255, 175, 110, 72, 64, 1, 0, 0, 0, 239, 6, 16, 32, 0, 1, 25, 67,
            52, 238, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ],
    )
    .expect("必须能写入 Tauri 临时构建图标");
    let escaped = icon
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let override_config = format!(r#"{{"bundle":{{"icon":["{escaped}"]}}}}"#);
    // SAFETY: build script 单线程设置自己的子进程环境，并立即交给 tauri-build。
    unsafe { std::env::set_var("TAURI_CONFIG", &override_config) };
    println!("cargo:rustc-env=TAURI_CONFIG={override_config}");
    tauri_build::build();
}

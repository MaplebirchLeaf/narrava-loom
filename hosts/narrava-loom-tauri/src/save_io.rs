//! `Save.export/import` 与发行目录 `save/` 的平台落盘边界。

use std::{fs, path::Path};

use narrava_loom_core::{ProjectConfig, save::SaveDocument, state::State, story::Story};

use crate::{HostErrorDto, script_runtime::EcmaBinding};

/// 取出脚本登记的 Save 请求并执行；import 成功后同步脚本变量，再回传完成钩子。
pub(crate) fn process_save(
    game_path: &Path,
    config: &ProjectConfig,
    script: &EcmaBinding,
    state: &mut State,
    story: &mut Story<'_, '_>,
) -> Result<(), HostErrorDto> {
    let Some((operation, target)) = script.take_save()? else {
        return Ok(());
    };
    let outcome = process_save_operation(game_path, config, state, story, &operation, &target);
    if outcome.is_ok() && operation == "import" {
        script.sync_variables(state)?;
    }
    script.complete_save(
        &operation,
        &target,
        outcome
            .as_ref()
            .map(|_| ())
            .map_err(|error| error.message.as_str()),
    )?;
    outcome
}

/// 执行同一存档边界：Host UI 命令与脚本请求共用此实现。
pub(crate) fn process_save_operation(
    game_path: &Path,
    config: &ProjectConfig,
    state: &mut State,
    story: &mut Story<'_, '_>,
    operation: &str,
    target: &str,
) -> Result<(), HostErrorDto> {
    let file_name = save_file_name(target)?;
    let save_directory = game_path.join("save");
    let path = save_directory.join(file_name);
    let identity = config
        .identity()
        .map_err(|error| HostErrorDto::new("tauri_host.save_identity", error.to_string()))?;
    (|| -> Result<(), String> {
        match operation {
            "export" => {
                fs::create_dir_all(&save_directory).map_err(|error| error.to_string())?;
                let document = SaveDocument::capture(&identity, state, story)
                    .map_err(|error| error.to_string())?;
                let json = document.to_json().map_err(|error| error.to_string())?;
                fs::write(&path, json).map_err(|error| error.to_string())
            }
            "import" => {
                let json = fs::read_to_string(&path).map_err(|error| error.to_string())?;
                let document = SaveDocument::from_json(&json).map_err(|error| error.to_string())?;
                document
                    .restore(&identity, state, story)
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
            _ => Err(format!("未知 Save 操作：{operation}")),
        }
    })()
    .map_err(|message| HostErrorDto::new("tauri_host.save", message))
}

/// 校验并规范化存档目标为 `<target>.nsave` 文件名（禁止路径逃逸）。
pub(crate) fn save_file_name(target: &str) -> Result<String, HostErrorDto> {
    if target.is_empty()
        || target.len() > 80
        || !target
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
    {
        return Err(HostErrorDto::new(
            "tauri_host.save_target",
            "Save target 只允许 1 至 80 个 ASCII 字母、数字、连字符或下划线",
        ));
    }
    Ok(format!("{target}.nsave"))
}

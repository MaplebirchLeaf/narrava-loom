//! TUI Host：编译游戏、驱动 Core Engine 并渲染到终端。
//!
//! 同步单线程驱动：加载开发目录或 `game.nar` 发行包 → 编译 Twee/脚本 →
//! `HostApi` 驱动 Engine 事务 → `TuiRenderer` 渲染 Surface → 终端输入
//! （编号选择导航、输入控件写回）回送 Host。脚本执行与宏分发复用
//! `narrava-loom-script`（Boa + 共享 dispatch）。

use std::{io, path::Path};

use narrava_loom_core::{
    ProjectConfig, SourceList,
    bytecode::BytecodeProgram,
    hir::HirStory,
    lir::LirProgram,
    mir::MirStory,
    nar::{NAR_MAGIC, NarPackage},
    package_zip,
    resource::ResourceCatalog,
    state::State,
    twee,
};
use narrava_loom_protocol::{
    HostErrorDto, HostUpdateDto, PendingOperation, RuntimeCommand, RuntimeSessionId, RuntimeUpdate,
};
use narrava_loom_script::{
    EcmaBinding, RuntimeSession, RuntimeSessionDriver,
    protocol_adapter::{SurfaceValue, diagnostic},
};

use crate::{TuiFrame, TuiRenderer, write_frame};

/// 装载游戏并进入渲染/输入主循环；`game_path` 是开发目录或含 `game.nar` 的发行目录。
pub fn run(game_path: &str) -> Result<(), HostErrorDto> {
    let loaded = load_game(game_path)?;
    let (sources, resources) = match &loaded {
        LoadedGame::Release {
            sources, resources, ..
        } => (sources, resources),
        LoadedGame::Development { sources, resources } => (sources, resources),
    };
    let config = match &loaded {
        LoadedGame::Release { config_toml, .. } => {
            ProjectConfig::parse(Path::new("game.nar/config.toml"), config_toml)
                .map_err(|error| HostErrorDto::new("tui_host.config", error.to_string()))?
        }
        LoadedGame::Development { .. } => ProjectConfig::load(game_path)
            .map_err(|error| HostErrorDto::new("tui_host.config", error.to_string()))?,
    };
    let ast: twee::Story<'_> =
        twee::Story::build(&sources.items).map_err(|error| diagnostic(error.diagnostic()))?;
    let hir: HirStory<'_> = HirStory::lower(&ast).map_err(|error| diagnostic(error.diagnostic))?;
    let mir: MirStory<'_, '_> = MirStory::lower(&hir)
        .map_err(|error| HostErrorDto::new("tui_host.mir", error.kind.to_string()))?;
    let bytecode: BytecodeProgram = match &loaded {
        LoadedGame::Release { bytecode, .. } => bytecode.as_ref().clone(),
        LoadedGame::Development { .. } => {
            let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).map_err(|error| {
                HostErrorDto::new("tui_host.lir", format!("{:?}", error.kind()))
            })?;
            BytecodeProgram::compile(&lir)
        }
    };
    let mut state: State = State::new();
    let script: std::rc::Rc<EcmaBinding> = EcmaBinding::load(
        sources,
        resources,
        mir.i18n(),
        &config.game.default_locale,
        &mut state,
    )
    .map_err(|error| HostErrorDto::new("tui_host.script", error.to_string()))?;
    let session: RuntimeSession<'_, '_, EcmaBinding> =
        RuntimeSession::new(&hir, &bytecode, script, state);
    let session_id =
        RuntimeSessionId::new("main").expect("内建主 Session ID 必须满足 Protocol 校验");
    let mut runtime: RuntimeSessionDriver<'_> = RuntimeSessionDriver::new(session_id, session);
    let mut renderer: TuiRenderer = TuiRenderer::default();

    // 启动起始 Passage 并渲染第一帧。
    let mut update = ready_update(execute_blocking(&mut runtime, RuntimeCommand::Start)?)?;

    let stdin = io::stdin();
    loop {
        let frame: TuiFrame = renderer.render_update(&update);
        write_frame(&mut io::stdout().lock(), &frame)
            .map_err(|error| HostErrorDto::new("tui_host.write", error.to_string()))?;
        write_help_prompt(&mut io::stdout().lock())
            .map_err(|error| HostErrorDto::new("tui_host.write", error.to_string()))?;

        let mut line = String::new();
        if stdin.read_line(&mut line).is_err() || line.trim().is_empty() {
            continue;
        }
        let command = crate::TuiCommand::parse(line.trim())
            .map_err(|error| HostErrorDto::new("tui_host.command", error.to_string()))?;
        match command
            .resolve(&frame)
            .map_err(|error| HostErrorDto::new("tui_host.command", error.to_string()))?
        {
            crate::TuiOperation::Help => continue,
            crate::TuiOperation::Back => {
                update = ready_update(execute_blocking(&mut runtime, RuntimeCommand::Back)?)?;
            }
            crate::TuiOperation::Forward => {
                update = ready_update(execute_blocking(&mut runtime, RuntimeCommand::Forward)?)?;
            }
            crate::TuiOperation::ToggleSidebar => {
                renderer.toggle_sidebar();
            }
            crate::TuiOperation::Redraw => continue,
            crate::TuiOperation::Quit => break,
            crate::TuiOperation::Activate { id } => {
                update = ready_update(execute_blocking(
                    &mut runtime,
                    RuntimeCommand::Activate { interaction: id },
                )?)?;
            }
            crate::TuiOperation::Input { id, value } => {
                execute_blocking(
                    &mut runtime,
                    RuntimeCommand::Input {
                        interaction: id,
                        value: json_from_surface(&value),
                    },
                )?;
            }
            crate::TuiOperation::Dismiss => continue,
        }
    }
    Ok(())
}

fn ready_update(update: RuntimeUpdate) -> Result<HostUpdateDto, HostErrorDto> {
    match update {
        RuntimeUpdate::Ready { update } => Ok(update),
        RuntimeUpdate::Applied => Err(HostErrorDto::new(
            "tui_host.update_expected",
            "Runtime 命令没有产生可展示更新",
        )),
        RuntimeUpdate::Pending { .. } => unreachable!("execute_blocking consumes pending updates"),
    }
}

/// TUI 的唯一平台异步职责：等待 Runtime 公开的 delay，再用同一 ID 恢复。
fn execute_blocking(
    runtime: &mut RuntimeSessionDriver<'_>,
    mut command: RuntimeCommand,
) -> Result<RuntimeUpdate, HostErrorDto> {
    loop {
        match runtime.execute(command)? {
            RuntimeUpdate::Pending {
                operation:
                    PendingOperation::Delay {
                        operation,
                        milliseconds,
                    },
            } => {
                std::thread::sleep(std::time::Duration::from_millis(milliseconds));
                command = RuntimeCommand::Resume {
                    operation,
                    result: None,
                };
            }
            RuntimeUpdate::Pending { operation } => {
                command = RuntimeCommand::Resume {
                    operation: operation.id(),
                    result: Some(narrava_loom_protocol::PendingResult::Failed {
                        error: HostErrorDto::new(
                            "tui.platform_operation_unsupported",
                            "TUI 尚未配置该平台操作",
                        ),
                    }),
                };
            }
            update => {
                for notice in runtime.take_notices() {
                    eprintln!("! {}：{}", notice.code, notice.message);
                }
                return Ok(update);
            }
        }
    }
}

/// 已装载游戏：发行包（`game.nar`）或开发目录。
enum LoadedGame {
    Release {
        sources: SourceList,
        resources: ResourceCatalog,
        config_toml: String,
        bytecode: Box<BytecodeProgram>,
    },
    Development {
        sources: SourceList,
        resources: ResourceCatalog,
    },
}

const PACKAGE_LIMIT: usize = 64 << 20;

/// 装载游戏：优先 `game.nar` 发行包（校验 NAR 魔数与哈希），否则发现开发目录。
fn load_game(game_path: &str) -> Result<LoadedGame, HostErrorDto> {
    let nar_path = Path::new(game_path).join("game.nar");
    if nar_path.exists() {
        let bytes = std::fs::read(&nar_path)
            .map_err(|error| HostErrorDto::new("tui_host.package_read", error.to_string()))?;
        let zip_bytes = bytes.strip_prefix(NAR_MAGIC).ok_or_else(|| {
            HostErrorDto::new("tui_host.package_magic", "game.nar 缺少 NAR 魔数头")
        })?;
        let files = package_zip::decode(zip_bytes, PACKAGE_LIMIT)
            .map_err(|error| HostErrorDto::new("tui_host.package_zip", error))?;
        let package = NarPackage::from_files(files)
            .map_err(|error| HostErrorDto::new("tui_host.package", error.to_string()))?;
        let config_toml = package
            .config_toml()
            .ok_or_else(|| HostErrorDto::new("tui_host.config", "game.nar 缺少 config.toml"))?
            .to_owned();
        let package = package
            .validate()
            .map_err(|error| HostErrorDto::new("tui_host.package", error.to_string()))?;
        return Ok(LoadedGame::Release {
            sources: package.sources().clone(),
            resources: package.resources().clone(),
            config_toml,
            bytecode: Box::new(package.bytecode().clone()),
        });
    }
    let sources = SourceList::discover(game_path)
        .map_err(|error| HostErrorDto::new("tui_host.source", error.to_string()))?;
    let resources = ResourceCatalog::discover(Path::new(game_path))
        .map_err(|error| HostErrorDto::new("tui_host.resource", error.to_string()))?;
    Ok(LoadedGame::Development { sources, resources })
}

/// Surface 值 → JSON（输入控件写回用）。
fn json_from_surface(value: &SurfaceValue) -> serde_json::Value {
    match value {
        SurfaceValue::Null => serde_json::Value::Null,
        SurfaceValue::Boolean(value) => serde_json::Value::Bool(*value),
        SurfaceValue::Number(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::Value::String(value.to_string())),
        SurfaceValue::Text(value) => serde_json::Value::String(value.clone()),
        _ => serde_json::Value::Null,
    }
}

fn write_help_prompt(writer: &mut impl io::Write) -> io::Result<()> {
    writeln!(
        writer,
        "输入编号选择动作；b 后退、f 前进、s 切换侧栏、h 帮助、r 重绘、q 退出"
    )
}

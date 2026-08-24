//! Narrava Loom 当前的最小命令行入口。

use std::{env, path::Path, process::ExitCode};

use narrava_loom_core::{
    ProjectConfig, Source, SourceList, bytecode::BytecodeProgram, hir::HirStory, lir::LirProgram,
    mir::MirStory, resource::ResourceCatalog, twee,
};

fn main() -> ExitCode {
    match run(env::args()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut args: env::Args) -> Result<(), String> {
    let _program: Option<String> = args.next();

    let Some(first): Option<String> = args.next() else {
        return Err(usage());
    };

    if first == "build" {
        let project = args.next().ok_or_else(usage)?;
        let output = args.next().ok_or_else(usage)?;
        let host = args.next().ok_or_else(usage)?;
        if args.next().is_some() {
            return Err(usage());
        }
        narrava_loom_core::release::build_directory(
            Path::new(&project),
            Path::new(&output),
            Path::new(&host),
        )?;
        println!("发行目录已生成：{output}");
        return Ok(());
    }
    let project = first;

    let config: ProjectConfig = ProjectConfig::load(&project).map_err(|error| error.to_string())?;

    println!(
        "{} {}（{}）",
        config.game.name, config.game.version, config.game.id
    );

    let sources: SourceList = SourceList::discover(&project).map_err(|error| error.to_string())?;
    let resources = ResourceCatalog::discover(&project).map_err(|error| error.to_string())?;
    print_sources(&sources);
    println!("已读取 {} 个 Resource", resources.len());
    compile_story(&sources)
}

fn usage() -> String {
    String::from(
        "用法:\n  narrava-loom-core <游戏目录>\n  narrava-loom-core build <游戏目录> <输出目录> <Tauri Host 二进制>",
    )
}

fn print_sources(sources: &SourceList) {
    for source in &sources.items {
        println!(
            "已读取 {}（{:?}，{} 字节）",
            source.path.as_str(),
            source.kind,
            source.content.len(),
        );
    }
}

fn compile_story(sources: &SourceList) -> Result<(), String> {
    let source_items: &[Source] = &sources.items;
    let story: twee::Story<'_> =
        twee::Story::build(source_items).map_err(|error| error.to_string())?;

    if story
        .passage(narrava_loom_core::story::special::START_PASSAGE)
        .is_none()
    {
        return Err(format!(
            "起始 Passage 不存在：{}",
            narrava_loom_core::story::special::START_PASSAGE
        ));
    }

    let hir: HirStory<'_> =
        HirStory::lower(&story).map_err(|error| error.diagnostic.to_string())?;
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).map_err(|error| {
        format!(
            "[mir.unsupported_node] HIR 节点 `{}` 尚未定义 MIR 降低（字节 {}..{}）",
            error.kind, error.span.start, error.span.end
        )
    })?;
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).map_err(|error| {
        let instruction = error
            .instruction()
            .map_or_else(String::new, |index| format!("，指令 {index}"));
        format!(
            "[lir.lower_failed] Passage `{}`{} 无法生成可执行程序：{:?}",
            error.passage(),
            instruction,
            error.kind()
        )
    })?;
    let bytecode: BytecodeProgram = BytecodeProgram::compile(&lir);

    println!(
        "可执行 Story 已建立（{} 个 Passage，Bytecode v{}）",
        bytecode.passages().len(),
        bytecode.header().version,
    );
    Ok(())
}

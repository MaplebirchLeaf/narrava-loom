//! 通过真实编译链和 RuntimeSession 构造测试游戏；断言辅助函数只提取宿主结果。

use std::{path::PathBuf, rc::Rc};

use crate::{EcmaBinding, RuntimeData, RuntimeSession};
use narrava_loom_core::{
    GameIdentity, SourceList, bytecode::BytecodeProgram, hir::HirStory, lir::LirProgram,
    mir::MirStory, resource::ResourceCatalog, state::State, twee,
};
use narrava_loom_protocol::{
    HostNodeDto, HostUpdateDto, PendingOperation, PendingResult, RuntimeCommand, RuntimeUpdate,
    SaveOperation,
};

/// 统一读取完成帧，同时保留音频效果供调用方验证提交边界。
pub(super) fn ready(
    update: RuntimeUpdate,
) -> (Vec<narrava_loom_protocol::AudioEffect>, HostUpdateDto) {
    match update {
        RuntimeUpdate::Audio {
            effects,
            update: Some(update),
        } => (effects, update),
        RuntimeUpdate::Ready { update } => (Vec::new(), update),
        _ => panic!("expected completed frame"),
    }
}

pub(super) fn navigate(
    runtime: &mut RuntimeSession<'_, '_>,
    update: &HostUpdateDto,
    target: &str,
) -> (Vec<narrava_loom_protocol::AudioEffect>, HostUpdateDto) {
    let interaction: String = update
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation {
                id, target: name, ..
            } if name.as_deref() == Some(target) => Some(id.clone()),
            _ => None,
        })
        .expect("test navigation must exist");
    ready(
        runtime
            .execute(RuntimeCommand::Activate { interaction })
            .unwrap(),
    )
}

pub(super) fn action(update: &HostUpdateDto, label: &str) -> String {
    update
        .nodes
        .iter()
        .find_map(|node| match node {
            HostNodeDto::Navigation {
                id,
                label: text,
                target: None,
                ..
            } if text == label => Some(id.clone()),
            _ => None,
        })
        .expect("action link")
}

/// 使用真实编译、脚本装载和会话执行入口；各用例的项目路径互不共享。
pub(super) fn with_runtime(
    story: &str,
    script: &str,
    test: impl FnOnce(&mut RuntimeSession<'_, '_>),
) {
    with_seeded_runtime(story, script, 0, test)
}

pub(super) fn with_seeded_runtime(
    story: &str,
    script: &str,
    seed: u64,
    test: impl FnOnce(&mut RuntimeSession<'_, '_>),
) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let root: PathBuf = PathBuf::from(format!(
        "target/test-projects/session-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("contents/story")).unwrap();
    std::fs::create_dir_all(root.join("contents/scripts")).unwrap();
    std::fs::write(root.join("contents/story/main.twee"), story).unwrap();
    std::fs::write(root.join("contents/scripts/main.js"), script).unwrap();
    let sources: SourceList = SourceList::discover(&root).unwrap();
    let ast: twee::Story = twee::Story::build(&sources.items).unwrap();
    let hir: HirStory<'_> = HirStory::lower(&ast).unwrap();
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).unwrap();
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).unwrap();
    let bytecode: BytecodeProgram = BytecodeProgram::compile(&lir);
    let mut state: State =
        State::with_engine(Rc::new(narrava_loom_core::engine::Engine::new(seed)));
    let binding: Rc<EcmaBinding> = EcmaBinding::load(
        &sources,
        &ResourceCatalog::default(),
        mir.i18n(),
        "en",
        &mut state,
    )
    .unwrap();
    let data: RuntimeData = RuntimeData::new(
        GameIdentity::new("session.test", "1.0.0").unwrap(),
        mir.i18n().clone(),
        "en".into(),
        Vec::new(),
    );
    let mut runtime: RuntimeSession<'_, '_> =
        RuntimeSession::with_data(&hir, &bytecode, binding, state, data);
    test(&mut runtime);
    std::fs::remove_dir_all(root).unwrap();
}

pub(super) fn pending(update: RuntimeUpdate) -> PendingOperation {
    let RuntimeUpdate::Pending { operation } = update else {
        panic!("expected pending")
    };
    operation
}

pub(super) fn resume(
    runtime: &mut RuntimeSession<'_, '_>,
    operation: u64,
    result: Option<PendingResult>,
) -> RuntimeUpdate {
    runtime
        .execute(RuntimeCommand::Resume { operation, result })
        .unwrap()
}

pub(super) fn navigation(nodes: &[HostNodeDto]) -> Option<String> {
    nodes.iter().find_map(|node| match node {
        HostNodeDto::Navigation { id, .. } => Some(id.clone()),
        HostNodeDto::Container { nodes, .. } | HostNodeDto::Region { nodes, .. } => {
            navigation(nodes)
        }
        _ => None,
    })
}

/// 模拟宿主接收导出文档并确认完成；返回可供导入的原始字节。
pub(super) fn export_save(runtime: &mut RuntimeSession<'_, '_>) -> Vec<u8> {
    let PendingOperation::Save {
        operation,
        document: Some(document),
        ..
    } = pending(
        runtime
            .execute(RuntimeCommand::Save {
                operation: SaveOperation::Export,
                target: "quick".into(),
            })
            .unwrap(),
    )
    else {
        panic!("export document")
    };
    resume(
        runtime,
        operation,
        Some(PendingResult::Save { document: None }),
    );
    document
}

/// 模拟宿主读入存档，并把完整的恢复结果交回调用方检查。
pub(super) fn import_save(
    runtime: &mut RuntimeSession<'_, '_>,
    document: Vec<u8>,
) -> (Vec<narrava_loom_protocol::AudioEffect>, HostUpdateDto) {
    let PendingOperation::Save { operation, .. } = pending(
        runtime
            .execute(RuntimeCommand::Save {
                operation: SaveOperation::Import,
                target: "quick".into(),
            })
            .unwrap(),
    ) else {
        panic!("import request")
    };
    ready(resume(
        runtime,
        operation,
        Some(PendingResult::Save {
            document: Some(document),
        }),
    ))
}

/// 主 Surface 的直接文本；不把弹窗、侧栏等独立区域混入正文断言。
pub(super) fn text(frame: &HostUpdateDto) -> String {
    frame
        .nodes
        .iter()
        .filter_map(|node: &HostNodeDto| match node {
            HostNodeDto::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<&str>>()
        .join("")
}

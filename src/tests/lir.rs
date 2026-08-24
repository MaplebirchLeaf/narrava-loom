use std::path::Path;

use crate::{
    hir::{HirBodyKind, HirBodyNode, HirPassage, HirStory},
    lir::{LirLowerErrorKind, LirProgram},
    mir::MirStory,
    source::Source,
    twee::Span,
};

fn text(text: &str) -> HirBodyNode<'_> {
    HirBodyNode {
        kind: HirBodyKind::Text(text),
        span: Span {
            start: 0,
            end: text.len(),
            line: 1,
            column: 1,
        },
    }
}

#[test]
fn lir_indexes_mir_passages_for_execution() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let hir: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![text("开始")],
            },
            HirPassage {
                source: &source.path,
                name: "Forest",
                tags: Vec::new(),
                body: vec![text("森林")],
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("HIR 应进入 MIR");

    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");

    assert_eq!(lir.passages().len(), 2);
    assert_eq!(
        lir.passage("Start").expect("Start 应已索引").name(),
        "Start"
    );
    assert_eq!(
        lir.passage_by_id(lir.passage("Forest").expect("Forest 应已索引").id())
            .expect("PassageId 应可恢复")
            .name(),
        "Forest"
    );
    assert!(std::ptr::eq(lir.i18n(), mir.i18n()));
}

#[test]
fn lir_rejects_duplicate_passage_names_at_its_boundary() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("测试 Source 应可读取");
    let hir: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: Vec::new(),
            },
        ],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("HIR 应进入 MIR");

    let error = LirProgram::lower(&mir).expect_err("LIR 不得保留有歧义的 Passage 索引");

    assert_eq!(error.passage(), "Start");
    assert_eq!(error.kind(), LirLowerErrorKind::DuplicatePassage);
}

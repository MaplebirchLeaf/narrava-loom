//! 固定随机序列及 VM、Macro 上下文的共享抽样边界。

use std::path::Path;

use crate::{
    bytecode::{BytecodeMacroBody, BytecodeProgram},
    expression::{
        Expression,
        evaluator::{evaluate_with, evaluate_with_mut},
        parse,
        value::Value,
    },
    hir::{HirBodyKind, HirBodyNode, HirPassage, HirStory},
    lir::LirProgram,
    macro_runtime::{MacroEvaluationContext, MacroLocalScopes, MacroLogicContext},
    mir::{MirMacroBody, MirStory},
    random::RandomState,
    source::Source,
    state::State,
    story::{Story, StoryRuntimeRequests},
    twee::Span,
    vm::{MirExecutionFrame, MirStep},
};

#[test]
fn random_splitmix64_matches_fixed_unit_vectors() {
    let expected: [u64; 5] = [
        0xe220_a839_7b1d_cdaf,
        0x6e78_9e6a_a1b9_65f4,
        0x06c4_5d18_8009_454f,
        0xf88b_b8a8_724c_81ec,
        0x1b39_896a_51a8_749b,
    ];
    let mut random: RandomState = RandomState::new(0);
    for value in expected {
        let unit: f64 = (value >> 11) as f64 / 9_007_199_254_740_992.0;
        assert_eq!(random.next_unit(), unit);
    }
    assert_eq!(random.seed(), 0);
    assert_eq!(random.state(), 0x1715_609f_7c74_6c69);
}

#[test]
fn random_state_round_trip_preserves_full_width_seed_and_cursor() {
    let mut random: RandomState = RandomState::new(u64::MAX);
    let _first: f64 = random.next_unit();
    let bytes: Vec<u8> = postcard::to_allocvec(&random).unwrap();
    let mut restored: RandomState = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(restored.seed(), u64::MAX);
    assert_eq!(restored, random);
    assert_eq!(restored.next_unit(), random.next_unit());
    assert!((0.0..1.0).contains(&restored.next_unit()));
}

fn random_assignment<'a>(expression: &'a str) -> HirBodyNode<'a> {
    HirBodyNode {
        kind: HirBodyKind::Set(Box::new(parse(expression).unwrap())),
        span: Span {
            start: 0,
            end: expression.len(),
            line: 1,
            column: 1,
        },
    }
}

#[test]
fn random_sequence_is_shared_by_vm_macro_frames_and_macro_contexts() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .unwrap();
    let hir: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![random_assignment("$first = random()")],
        }],
    };
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).unwrap();
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).unwrap();
    let bytecode: BytecodeProgram = BytecodeProgram::compile(&lir);
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(bytecode.passage("Start").unwrap());
    let mut state: State = State::new();
    state.seed_random(42);
    let mut expected: RandomState = RandomState::new(42);
    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::Running));
    assert_eq!(
        state.variables_get("first"),
        Some(&Value::Number(expected.next_unit()))
    );

    let body: Vec<HirBodyNode<'_>> = vec![random_assignment("$second = random()")];
    let mir: MirMacroBody<'_, '_> = MirMacroBody::lower(&body).unwrap();
    let bytecode: BytecodeMacroBody = BytecodeMacroBody::compile(&mir);
    let mut frame: MirExecutionFrame = MirExecutionFrame::new_macro(&bytecode);
    assert_eq!(
        frame.step_macro(&bytecode, &mut state),
        Ok(MirStep::Running)
    );
    assert_eq!(
        state.variables_get("second"),
        Some(&Value::Number(expected.next_unit()))
    );

    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let context: MacroEvaluationContext<'_> = MacroEvaluationContext::new(&state, &locals);
    let expression: Expression<'_> = parse("random()").unwrap();
    assert_eq!(
        evaluate_with(&expression, &context).unwrap(),
        Value::Number(expected.next_unit())
    );

    let story: Story<'_, '_> = Story::new(&hir);
    let mut requests: StoryRuntimeRequests<'_, '_, '_> = StoryRuntimeRequests::new(&story);
    let mut context: MacroLogicContext<'_, StoryRuntimeRequests<'_, '_, '_>> =
        MacroLogicContext::new(&mut state, &mut requests, &mut locals);
    let expression: Expression<'_> = parse("$choice = either(10, 20)").unwrap();
    let choice: f64 = if expected.next_unit() < 0.5 {
        10.0
    } else {
        20.0
    };
    assert_eq!(
        evaluate_with_mut(&expression, &mut context).unwrap(),
        Value::Number(choice)
    );
    assert_eq!(state.random_state(), expected);
}

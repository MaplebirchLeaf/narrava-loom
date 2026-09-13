//! Engine 随机源覆盖 VM、Macro 帧与上下文。

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
    source::Source,
    state::State,
    story::{Story, StoryRuntimeRequests},
    twee::Span,
    vm::{MirExecutionFrame, MirStep},
};

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
fn ordinary_random_is_available_in_vm_macro_frames_and_contexts() {
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
    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::Running));
    assert_unit(state.variables_get("first").unwrap());

    let body: Vec<HirBodyNode<'_>> = vec![random_assignment("$second = random()")];
    let mir: MirMacroBody<'_, '_> = MirMacroBody::lower(&body).unwrap();
    let bytecode: BytecodeMacroBody = BytecodeMacroBody::compile(&mir);
    let mut frame: MirExecutionFrame = MirExecutionFrame::new_macro(&bytecode);
    assert_eq!(
        frame.step_macro(&bytecode, &mut state),
        Ok(MirStep::Running)
    );
    assert_unit(state.variables_get("second").unwrap());

    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    let context: MacroEvaluationContext<'_> = MacroEvaluationContext::new(&state, &locals);
    let expression: Expression<'_> = parse("random()").unwrap();
    assert_unit(&evaluate_with(&expression, &context).unwrap());

    let story: Story<'_, '_> = Story::new(&hir);
    let mut requests: StoryRuntimeRequests<'_, '_, '_> = StoryRuntimeRequests::new(&story);
    let mut context: MacroLogicContext<'_, StoryRuntimeRequests<'_, '_, '_>> =
        MacroLogicContext::new(&mut state, &mut requests, &mut locals);
    let expression: Expression<'_> = parse("$choice = either(10, 20)").unwrap();
    let choice: Value = evaluate_with_mut(&expression, &mut context).unwrap();
    assert!(choice == Value::Number(10.0) || choice == Value::Number(20.0));
}

fn assert_unit(value: &Value) {
    let Value::Number(unit) = value else {
        panic!("expected random number")
    };
    assert!((0.0..1.0).contains(unit));
}

#[test]
fn engine_root_seed_controls_state_draws_and_isolated_views() {
    use crate::engine::Engine;
    use std::rc::Rc;
    let engine: Rc<Engine> = Rc::new(Engine::new(42));
    let mut state: State = State::with_engine(Rc::clone(&engine));
    let expression: Expression<'_> = parse("random()").unwrap();
    let first: Value = evaluate_with(&expression, &state).unwrap();
    assert_eq!(first, Value::Number(0.7415648787718233));
    let checkpoint = state.checkpoint();
    let view: State = state.fork_view();
    let expected: Value = evaluate_with(&expression, &view).unwrap();
    assert_eq!(evaluate_with(&expression, &state).unwrap(), expected);
    state.restore_checkpoint(checkpoint);
    assert_eq!(evaluate_with(&expression, &state).unwrap(), expected);
    assert_eq!(engine.seed(), 42);
}

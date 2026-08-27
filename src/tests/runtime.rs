//! Runtime HIR 正文顺序执行与暂停组合测试。

use std::path::Path;

use crate::{
    expression::value::{TextValue, Value},
    hir::{HirBodyKind, HirBodyNode, HirMacro, HirMacroArguments, HirPassage, HirStory},
    lir::LirProgram,
    macro_runtime::{MacroHandlerOutcome, MacroLocalScopes, MacroSuspension},
    mir::{MirMacroBody, MirStory},
    runtime::{
        BodyControl, BodyExecution, RuntimeExecutionIdentity, RuntimeMacroBodyContinuation,
        RuntimeMacroBodyContinuationResume, RuntimeMacroContinuation,
        RuntimeMacroContinuationError, RuntimeMacroContinuationResume, RuntimeMacroExecution,
        execute_hir_body,
    },
    semantic::{SemanticNode, SemanticOutput},
    source::Source,
    state::State,
    twee::Span,
    vm::{MirExecutionFrame, MirStep},
};

fn text_node(text: &'static str, start: usize) -> HirBodyNode<'static> {
    HirBodyNode {
        kind: HirBodyKind::Text(text),
        span: Span {
            start,
            end: start + text.len(),
            line: 1,
            column: start + 1,
        },
    }
}

fn dynamic_macro_story(source: &Source) -> HirStory<'_> {
    HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![HirBodyNode {
                kind: HirBodyKind::Macro(HirMacro {
                    name: "wait",
                    arguments: HirMacroArguments::None,
                    syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                    body: Vec::new(),
                }),
                span: Span {
                    start: 0,
                    end: 1,
                    line: 1,
                    column: 1,
                },
            }],
        }],
    }
}

fn macro_suspension(identity: RuntimeExecutionIdentity) -> MacroSuspension<u64> {
    let mut locals: MacroLocalScopes<Value> = MacroLocalScopes::new();
    locals.enter_call(vec![Value::string("argument")]);
    MacroSuspension {
        identity,
        handle: 7,
        scopes: locals.suspend().expect("活动调用帧应能暂停"),
    }
}

#[test]
fn runtime_macro_body_continuation_resumes_and_advances_its_own_frame() {
    let body: Vec<HirBodyNode<'_>> = vec![
        HirBodyNode {
            kind: HirBodyKind::Macro(HirMacro {
                name: "wait",
                arguments: HirMacroArguments::None,
                syntax_kind: crate::twee::MacroSyntaxKind::Inline,
                body: Vec::new(),
            }),
            span: Span {
                start: 0,
                end: 1,
                line: 1,
                column: 1,
            },
        },
        text_node("after", 1),
    ];
    let mir: MirMacroBody<'_, '_> = MirMacroBody::lower(&body).expect("正文应进入 MIR");
    let macro_bytecode: crate::bytecode::BytecodeMacroBody =
        crate::bytecode::BytecodeMacroBody::compile(&mir);
    let mut frame: MirExecutionFrame = MirExecutionFrame::new_macro(&macro_bytecode);
    let mut state: State = State::new();
    assert_eq!(
        frame.step_macro(&macro_bytecode, &mut state),
        Ok(MirStep::MacroPending)
    );
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(9, 4);
    let continuation: RuntimeMacroBodyContinuation<u64> =
        RuntimeMacroBodyContinuation::new(identity, frame, macro_suspension(identity), &mir)
            .expect("正文位置与暂停身份应匹配");

    let resumed: RuntimeMacroBodyContinuationResume<u64> = continuation
        .resume(&mir, |handle, _locals| {
            assert_eq!(handle, 7);
            Ok::<_, &'static str>(MacroHandlerOutcome::Complete(RuntimeMacroExecution {
                execution: BodyExecution::default(),
                includes_entered: 0,
            }))
        })
        .expect("正文 Macro 应恢复");
    let RuntimeMacroBodyContinuationResume::Complete(mut resumed) = resumed else {
        panic!("完成回调不应再次暂停");
    };

    assert_eq!(resumed.frame.location().instruction().index(), 1);
    assert_eq!(
        resumed.frame.step_macro(&macro_bytecode, &mut state),
        Ok(MirStep::Running)
    );
    assert_eq!(
        resumed.frame.step_macro(&macro_bytecode, &mut state),
        Ok(MirStep::Halted)
    );
}

#[test]
fn runtime_macro_body_continuation_keeps_position_when_pending_again() {
    let body: Vec<HirBodyNode<'_>> = vec![HirBodyNode {
        kind: HirBodyKind::Macro(HirMacro {
            name: "wait",
            arguments: HirMacroArguments::None,
            syntax_kind: crate::twee::MacroSyntaxKind::Inline,
            body: Vec::new(),
        }),
        span: Span {
            start: 0,
            end: 1,
            line: 1,
            column: 1,
        },
    }];
    let mir: MirMacroBody<'_, '_> = MirMacroBody::lower(&body).expect("正文应进入 MIR");
    let macro_bytecode: crate::bytecode::BytecodeMacroBody =
        crate::bytecode::BytecodeMacroBody::compile(&mir);
    let mut frame: MirExecutionFrame = MirExecutionFrame::new_macro(&macro_bytecode);
    let mut state: State = State::new();
    assert_eq!(
        frame.step_macro(&macro_bytecode, &mut state),
        Ok(MirStep::MacroPending)
    );
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(9, 5);
    let continuation: RuntimeMacroBodyContinuation<u64> =
        RuntimeMacroBodyContinuation::new(identity, frame, macro_suspension(identity), &mir)
            .expect("正文位置与暂停身份应匹配");

    let resumed: RuntimeMacroBodyContinuationResume<u64> = continuation
        .resume(&mir, |_handle, _locals| {
            Ok::<_, &'static str>(MacroHandlerOutcome::Pending(8))
        })
        .expect("再次等待应保留 continuation");
    let RuntimeMacroBodyContinuationResume::Pending(pending) = resumed else {
        panic!("Pending 回调不应推进正文");
    };

    assert_eq!(pending.location().position().instruction().index(), 0);
    assert_eq!(pending.suspension().handle, 8);
}

#[test]
fn executes_all_nodes_when_each_one_continues() {
    let body: Vec<HirBodyNode<'static>> = vec![text_node("first", 0), text_node("second", 5)];
    let mut visited: Vec<&str> = Vec::new();

    let control: BodyControl = execute_hir_body(&body, |node: &HirBodyNode<'_>| {
        let HirBodyKind::Text(text) = node.kind else {
            unreachable!("测试正文只包含 Text")
        };
        visited.push(text);
        Ok::<BodyControl, &'static str>(BodyControl::Continue)
    })
    .expect("全部节点应执行完成");

    assert_eq!(control, BodyControl::Continue);
    assert_eq!(visited, vec!["first", "second"]);
}

#[test]
fn stops_before_nodes_after_goto_control() {
    let body: Vec<HirBodyNode<'static>> = vec![
        text_node("before", 0),
        text_node("goto", 6),
        text_node("after", 10),
    ];
    let mut visited: Vec<&str> = Vec::new();

    let control: BodyControl = execute_hir_body(&body, |node: &HirBodyNode<'_>| {
        let HirBodyKind::Text(text) = node.kind else {
            unreachable!("测试正文只包含 Text")
        };
        visited.push(text);
        let control: BodyControl = if text == "goto" {
            BodyControl::StopPassage
        } else {
            BodyControl::Continue
        };
        Ok::<BodyControl, &'static str>(control)
    })
    .expect("停止信号应正常返回");

    assert_eq!(control, BodyControl::StopPassage);
    assert_eq!(visited, vec!["before", "goto"]);
}

#[test]
fn with_output_accumulates_nodes_in_order_until_control_stop() {
    let body: Vec<HirBodyNode<'static>> = vec![
        text_node("first", 0),
        text_node("goto", 5),
        text_node("after", 9),
    ];
    let mut visited: Vec<&str> = Vec::new();

    let execution: crate::runtime::BodyExecution =
        crate::runtime::execute_hir_body_with_output(&body, |node: &HirBodyNode<'_>| {
            let HirBodyKind::Text(text) = node.kind else {
                unreachable!("测试正文只包含 Text")
            };
            visited.push(text);
            let control: crate::runtime::BodyControl = if text == "goto" {
                crate::runtime::BodyControl::StopPassage
            } else {
                crate::runtime::BodyControl::Continue
            };
            let output: SemanticOutput =
                SemanticOutput::from_nodes(vec![SemanticNode::Text(TextValue::from(text))]);
            Ok::<crate::runtime::BodyExecution, &'static str>(crate::runtime::BodyExecution {
                control,
                output,
            })
        })
        .expect("停止信号应正常返回");

    assert_eq!(execution.control, crate::runtime::BodyControl::StopPassage);
    assert_eq!(visited, vec!["first", "goto"]);
    assert_eq!(execution.output.len(), 2);
    assert_eq!(
        execution.output.nodes()[0],
        SemanticNode::Text(TextValue::from("first"))
    );
    assert_eq!(
        execution.output.nodes()[1],
        SemanticNode::Text(TextValue::from("goto"))
    );
}

#[test]
fn with_output_returns_empty_when_all_nodes_continue() {
    let body: Vec<HirBodyNode<'static>> = vec![text_node("only", 0)];

    let execution: crate::runtime::BodyExecution =
        crate::runtime::execute_hir_body_with_output(&body, |_: &HirBodyNode<'_>| {
            Ok::<crate::runtime::BodyExecution, &'static str>(crate::runtime::BodyExecution {
                control: crate::runtime::BodyControl::Continue,
                output: SemanticOutput::default(),
            })
        })
        .expect("应正常完成");

    assert_eq!(execution.control, crate::runtime::BodyControl::Continue);
    assert!(execution.output.is_empty());
}

#[test]
fn with_output_discards_accumulation_on_error() {
    let body: Vec<HirBodyNode<'static>> = vec![
        text_node("first", 0),
        text_node("second", 5),
        text_node("third", 11),
    ];
    let mut visited: Vec<&str> = Vec::new();

    let result: Result<crate::runtime::BodyExecution, &'static str> =
        crate::runtime::execute_hir_body_with_output(&body, |node: &HirBodyNode<'_>| {
            let HirBodyKind::Text(text) = node.kind else {
                unreachable!("测试正文只包含 Text")
            };
            visited.push(text);
            if text == "second" {
                return Err("中断");
            }
            Ok(crate::runtime::BodyExecution {
                control: crate::runtime::BodyControl::Continue,
                output: SemanticOutput::default(),
            })
        });

    assert_eq!(result, Err("中断"));
    assert_eq!(visited, vec!["first", "second"]);
}

#[test]
fn runtime_macro_continuation_binds_vm_position_to_the_same_identity() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let hir: HirStory<'_> = dynamic_macro_story(&source);
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("动态 Macro 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);
    let mut state: State = State::new();
    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::MacroPending));
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(3, 5);

    let continuation: RuntimeMacroContinuation<u64> =
        RuntimeMacroContinuation::new(identity, frame, macro_suspension(identity), &bytecode)
            .expect("相同身份与 MacroPending 位置应能组成 continuation");

    assert_eq!(continuation.identity(), identity);
    assert_eq!(continuation.location().identity(), identity);
    assert_eq!(continuation.location().position().instruction().index(), 0);
    assert_eq!(continuation.suspension().handle, 7);

    let resumed: RuntimeMacroContinuationResume<u64> = continuation
        .resume(&bytecode, |handle, locals| {
            assert_eq!(handle, 7);
            assert_eq!(locals.args(), Some([Value::string("argument")].as_slice()));
            Ok::<_, &'static str>(MacroHandlerOutcome::Complete(RuntimeMacroExecution {
                execution: BodyExecution {
                    control: BodyControl::Continue,
                    output: SemanticOutput::from_nodes(vec![SemanticNode::Text(TextValue::from(
                        "异步完成",
                    ))]),
                },
                includes_entered: 2,
            }))
        })
        .expect("异步 Macro 应回到原 VM 位置");
    let RuntimeMacroContinuationResume::Complete(resumed) = resumed else {
        panic!("完成回调不应再次暂停");
    };

    assert_eq!(resumed.frame.location().instruction().index(), 1);
    assert_eq!(resumed.frame.output().len(), 1);
    assert_eq!(resumed.control, BodyControl::Continue);
    assert_eq!(resumed.includes_entered, 2);
    assert_eq!(resumed.scopes.args(), None);
}

#[test]
fn runtime_macro_continuation_rejects_a_suspension_from_another_chain() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let hir: HirStory<'_> = dynamic_macro_story(&source);
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("动态 Macro 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let frame: MirExecutionFrame = MirExecutionFrame::new(passage);
    let expected: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(3, 5);
    let actual: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(3, 6);

    let error: RuntimeMacroContinuationError<u64> =
        RuntimeMacroContinuation::new(expected, frame, macro_suspension(actual), &bytecode)
            .expect_err("不同执行链的暂停状态不能拼接");

    let RuntimeMacroContinuationError::IdentityMismatch {
        expected: error_expected,
        parts,
    } = error
    else {
        panic!("身份不一致应优先返回身份错误");
    };
    assert_eq!(error_expected, expected);
    assert_eq!(parts.suspension.identity, actual);
    assert_eq!(parts.suspension.handle, 7);
}

#[test]
fn runtime_macro_continuation_keeps_the_vm_position_when_pending_again() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let hir: HirStory<'_> = dynamic_macro_story(&source);
    let mir: MirStory<'_, '_> = MirStory::lower(&hir).expect("动态 Macro 应进入 MIR");
    let lir: LirProgram<'_, '_, '_> = LirProgram::lower(&mir).expect("MIR 应进入 LIR");
    let bytecode: crate::bytecode::BytecodeProgram =
        crate::bytecode::BytecodeProgram::compile(&lir);
    let passage = bytecode.passage("Start").expect("Start 应存在");
    let mut frame: MirExecutionFrame = MirExecutionFrame::new(passage);
    let mut state: State = State::new();
    assert_eq!(frame.step(&bytecode, &mut state), Ok(MirStep::MacroPending));
    let identity: RuntimeExecutionIdentity = RuntimeExecutionIdentity::new(3, 5);
    let continuation: RuntimeMacroContinuation<u64> =
        RuntimeMacroContinuation::new(identity, frame, macro_suspension(identity), &bytecode)
            .expect("初次暂停应有效");

    let resumed: RuntimeMacroContinuationResume<u64> = continuation
        .resume(&bytecode, |handle, locals| {
            assert_eq!(handle, 7);
            assert!(locals.args().is_some());
            Ok::<_, &'static str>(MacroHandlerOutcome::Pending(8))
        })
        .expect("Handler 应能再次暂停");
    let RuntimeMacroContinuationResume::Pending(pending) = resumed else {
        panic!("Pending 回调不应完成 VM 指令");
    };

    assert_eq!(pending.location().position().instruction().index(), 0);
    assert_eq!(pending.suspension().handle, 8);
}

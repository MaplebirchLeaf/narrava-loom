//! 动态 Macro 回调分发。
//!
//! Engine 在 MIR 遇到动态 Macro 时回调本模块：解析参数、求值、调用 builtin/脚本
//! 宏、处理 before/after 与异步 Pending，并把输出交回 VM。本模块宿主无关，
//! Host（Tauri/TUI）只需提供脚本 Binding 与状态。

use narrava_loom_core::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    engine::{
        EngineMirMacroCallbackFailure, EngineMirMacroInvocation, PassageLifecycleContext,
        PassageLifecyclePhase,
    },
    expression::{
        evaluator::{assign_value_with_mut, evaluate_with_mut, value_to_text},
        parse as parse_expression,
        value::Value,
    },
    hir::{HirBodyKind, HirBodyNode, HirMacro, HirMacroArguments, HirStory, OwnedHirMacro},
    macro_runtime::{
        MacroDefinition, MacroDefinitions, MacroInteractions, MacroLocalScopes, MacroLogicContext,
        MacroResumeOutcome, MacroSuspension, RuntimeMacroHandler, button_with_body, checkbox,
        link_with_body, parse_argument_list, prepare_argument_values, print, radiobutton, replace,
        slot, textbox,
    },
    runtime::{BodyControl, BodyExecution, RuntimeExecutionContext, RuntimeMacroExecution},
    semantic::SemanticOutput,
    state::State,
    story::StoryRuntimeRequests,
};

use narrava_loom_protocol::{Surface, SurfaceNode};

use crate::ScriptError;

/// 把 Core 已确认的 Passage 生命周期事实投递给游戏脚本。
///
/// 映射属于共享 Native Script Binding，而不是某一种 Host；Tauri 与 TUI 必须调用同一实现，
/// 才能保证同一游戏在不同前端收到相同的内建事件序列。
pub fn emit_passage_event(
    script: &crate::EcmaBinding,
    phase: PassageLifecyclePhase,
    context: PassageLifecycleContext<'_, '_, '_, '_>,
) -> Result<(), Diagnostic> {
    let name: &str = match phase {
        PassageLifecyclePhase::Init => "passage:init",
        PassageLifecyclePhase::Start => "passage:start",
        PassageLifecyclePhase::Render => "passage:render",
        PassageLifecyclePhase::Display => "passage:display",
        PassageLifecyclePhase::End => "passage:end",
    };
    let passage = context.entry().passage();
    script
        .emit_builtin_event(
            name,
            &serde_json::json!({ "passage": passage.name, "tags": passage.tags }),
        )
        .map(|_| ())
        .map_err(|error| {
            Diagnostic::new(
                "script.passage_event",
                DiagnosticSeverity::Error,
                &error.message,
            )
        })
}

#[allow(clippy::too_many_arguments)]
pub fn dispatch_macro<'hir, 'source>(
    script: &crate::EcmaBinding,
    hir: &'hir HirStory<'source>,
    interactions: &mut MacroInteractions<'hir, 'source>,
    scheduled: &mut Option<crate::ScriptPending>,
    invocation: EngineMirMacroInvocation<'_>,
    state: &mut State,
    requests: &mut StoryRuntimeRequests<'_, 'hir, 'source>,
    mut scopes: MacroLocalScopes<Value>,
) -> Result<
    MacroResumeOutcome<RuntimeMacroExecution, crate::ScriptPending>,
    EngineMirMacroCallbackFailure<String>,
> {
    let call: HirMacro<'_> = invocation.call.as_hir();
    let raw: &str = match &call.arguments {
        HirMacroArguments::Raw(raw) => raw,
        HirMacroArguments::None => "",
        HirMacroArguments::Expression(_) => {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("脚本 Macro 暂不接受编译器 Expression 参数：{}", call.name),
                scopes,
            });
        }
    };
    if call.name == "print" {
        let parsed = parse_argument_list(raw).map_err(|error| EngineMirMacroCallbackFailure {
            error: format!("print 参数无效：{error:?}"),
            scopes: scopes.clone(),
        })?;
        let arguments: Vec<Value> = {
            let mut context = MacroLogicContext::new(state, requests, &mut scopes);
            prepare_argument_values(&parsed, |expression| {
                evaluate_with_mut(expression, &mut context)
            })
            .map_err(|error| EngineMirMacroCallbackFailure {
                error: format!("print 参数无法求值：{error:?}"),
                scopes: scopes.clone(),
            })?
        };
        let execution = print(&arguments).map_err(|error| EngineMirMacroCallbackFailure {
            error: error.to_string(),
            scopes: scopes.clone(),
        })?;
        return Ok(MacroResumeOutcome::Complete {
            output: RuntimeMacroExecution {
                execution,
                includes_entered: 0,
            },
            scopes,
        });
    }
    if matches!(call.name, "replace" | "slot") {
        let parsed = parse_argument_list(raw).map_err(|error| EngineMirMacroCallbackFailure {
            error: format!("replace 参数无效：{error:?}"),
            scopes: scopes.clone(),
        })?;
        let arguments: Vec<Value> = {
            let mut context = MacroLogicContext::new(state, requests, &mut scopes);
            prepare_argument_values(&parsed, |expression| {
                evaluate_with_mut(expression, &mut context)
            })
            .map_err(|error| EngineMirMacroCallbackFailure {
                error: format!("replace 参数无法求值：{error:?}"),
                scopes: scopes.clone(),
            })?
        };
        let [Value::String(target)] = arguments.as_slice() else {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("{} 必须接收一个文字 key", call.name),
                scopes,
            });
        };
        let target: String =
            target
                .to_unicode_string()
                .ok_or_else(|| EngineMirMacroCallbackFailure {
                    error: format!("{} key 必须是有效 Unicode", call.name),
                    scopes: scopes.clone(),
                })?;
        let source_call: &'hir HirMacro<'source> = find_hir_macro(hir, invocation.call)
            .ok_or_else(|| EngineMirMacroCallbackFailure {
                error: format!("无法从原始 HIR 找回 {} 容器正文", call.name),
                scopes: scopes.clone(),
            })?;
        let definitions: MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'hir, 'source, ()>>> =
            MacroDefinitions::new();
        let body_execution = {
            let mut runtime =
                RuntimeExecutionContext::new(&definitions, state, requests, &mut scopes);
            runtime.execute_fragment(source_call.body.as_slice())
        }
        .map_err(|error| EngineMirMacroCallbackFailure {
            error: format!("replace 正文执行失败：{error:?}"),
            scopes: scopes.clone(),
        })?;
        if !matches!(
            body_execution.control,
            BodyControl::Continue | BodyControl::ExitScope
        ) {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("{} 正文不能中断 Passage 或发起导航", call.name),
                scopes,
            });
        }
        let execution: BodyExecution = if call.name == "slot" {
            slot(target.as_str(), body_execution.output)
        } else {
            replace(target.as_str(), body_execution.output)
        }
        .map_err(|error| EngineMirMacroCallbackFailure {
            error: error.to_string(),
            scopes: scopes.clone(),
        })?;
        return Ok(MacroResumeOutcome::Complete {
            output: RuntimeMacroExecution {
                execution,
                includes_entered: 0,
            },
            scopes,
        });
    }
    if matches!(call.name, "checkbox" | "radiobutton" | "textbox") {
        let parsed = parse_argument_list(raw).map_err(|error| EngineMirMacroCallbackFailure {
            error: format!("{} 参数无效：{error:?}", call.name),
            scopes: scopes.clone(),
        })?;
        let arguments: Vec<Value> = {
            let mut context: MacroLogicContext<'_, StoryRuntimeRequests<'_, 'hir, 'source>> =
                MacroLogicContext::new(state, requests, &mut scopes);
            prepare_argument_values(&parsed, |expression| {
                evaluate_with_mut(expression, &mut context)
            })
            .map_err(|error| EngineMirMacroCallbackFailure {
                error: format!("{} 参数无法求值：{error:?}", call.name),
                scopes: scopes.clone(),
            })?
        };
        let receiver: String = match arguments.first() {
            Some(Value::String(value)) => {
                value
                    .to_unicode_string()
                    .ok_or_else(|| EngineMirMacroCallbackFailure {
                        error: format!("{} receiver 必须是有效 Unicode", call.name),
                        scopes: scopes.clone(),
                    })?
            }
            _ => {
                return Err(EngineMirMacroCallbackFailure {
                    error: format!("{} 第一个参数必须是带引号的 receiver", call.name),
                    scopes,
                });
            }
        };
        if receiver.starts_with('@') {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("{} 暂不支持 @ receiver", call.name),
                scopes,
            });
        }
        let receiver_expression =
            parse_expression(receiver.as_str()).map_err(|error| EngineMirMacroCallbackFailure {
                error: format!("{} receiver 无效：{error:?}", call.name),
                scopes: scopes.clone(),
            })?;
        if !receiver_expression.is_assignable_target() {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("{} receiver 不是可写目标", call.name),
                scopes,
            });
        }
        let current_result = {
            let mut context: MacroLogicContext<'_, StoryRuntimeRequests<'_, 'hir, 'source>> =
                MacroLogicContext::new(state, requests, &mut scopes);
            evaluate_with_mut(&receiver_expression, &mut context)
        };
        let mut current: Value = current_result.map_err(|error| EngineMirMacroCallbackFailure {
            error: format!("{} receiver 无法读取：{error:?}", call.name),
            scopes: scopes.clone(),
        })?;
        if call.name == "textbox" && matches!(current, Value::Undefined) {
            let default: Value =
                arguments
                    .get(1)
                    .cloned()
                    .ok_or_else(|| EngineMirMacroCallbackFailure {
                        error: "textbox 需要 receiver 与默认值".to_owned(),
                        scopes: scopes.clone(),
                    })?;
            let assignment = {
                let mut context: MacroLogicContext<'_, StoryRuntimeRequests<'_, 'hir, 'source>> =
                    MacroLogicContext::new(state, requests, &mut scopes);
                assign_value_with_mut(&receiver_expression, default.clone(), &mut context)
            };
            assignment.map_err(|error| EngineMirMacroCallbackFailure {
                error: format!("textbox 默认值无法写入：{error:?}"),
                scopes: scopes.clone(),
            })?;
            current = default;
        }
        let execution: BodyExecution = match (call.name, arguments.as_slice()) {
            ("checkbox", [_, unchecked, checked]) => checkbox(
                receiver.as_str(),
                unchecked,
                checked,
                &current,
                invocation.identity,
                invocation.location.instruction().index(),
            ),
            ("radiobutton", [_, value]) => radiobutton(
                receiver.as_str(),
                value,
                &current,
                invocation.identity,
                invocation.location.instruction().index(),
            ),
            ("textbox", [_, _]) => textbox(
                receiver.as_str(),
                &current,
                invocation.identity,
                invocation.location.instruction().index(),
            ),
            _ => Err(Diagnostic::new(
                "macro.input.invalid_arguments",
                DiagnosticSeverity::Error,
                &format!("{} 参数数量不正确", call.name),
            )),
        }
        .map_err(|error| EngineMirMacroCallbackFailure {
            error: error.to_string(),
            scopes: scopes.clone(),
        })?;
        return Ok(MacroResumeOutcome::Complete {
            output: RuntimeMacroExecution {
                execution,
                includes_entered: 0,
            },
            scopes,
        });
    }
    if !matches!(call.name, "link" | "button") {
        let exists =
            script
                .has_macro(call.name)
                .map_err(|error| EngineMirMacroCallbackFailure {
                    error: error.to_string(),
                    scopes: scopes.clone(),
                })?;
        if !exists {
            return Err(EngineMirMacroCallbackFailure {
                error: format!("Macro 不存在：{}", call.name),
                scopes,
            });
        }
        let outcome = script.call_macro(call.name, raw, state).map_err(|error| {
            EngineMirMacroCallbackFailure {
                error: error.to_string(),
                scopes: scopes.clone(),
            }
        })?;
        let value: Value = match outcome {
            crate::ScriptMacroOutcome::Complete(value) => value,
            crate::ScriptMacroOutcome::Pending(handle) => {
                scopes.enter_call(Vec::new());
                *scheduled = Some(handle.clone());
                let suspended =
                    scopes
                        .suspend()
                        .map_err(|error| EngineMirMacroCallbackFailure {
                            error: format!("Macro 局部域无法暂停：{error:?}"),
                            scopes: MacroLocalScopes::new(),
                        })?;
                return Ok(MacroResumeOutcome::Pending(MacroSuspension {
                    identity: invocation.identity,
                    handle,
                    scopes: suspended,
                }));
            }
        };
        let execution: RuntimeMacroExecution =
            macro_value_execution(&value).map_err(|error| EngineMirMacroCallbackFailure {
                error: error.to_string(),
                scopes: scopes.clone(),
            })?;
        return Ok(MacroResumeOutcome::Complete {
            output: execution,
            scopes,
        });
    }
    let parsed = parse_argument_list(raw).map_err(|error| EngineMirMacroCallbackFailure {
        error: format!("link 参数无效：{error:?}"),
        scopes: scopes.clone(),
    })?;
    let arguments: Vec<Value> =
        prepare_argument_values(&parsed, |_expression| Err::<Value, ()>(())).map_err(|error| {
            EngineMirMacroCallbackFailure {
                error: format!("link 参数不能求值：{error:?}"),
                scopes: scopes.clone(),
            }
        })?;
    let source_call: &'hir HirMacro<'source> =
        find_hir_macro(hir, invocation.call).ok_or_else(|| EngineMirMacroCallbackFailure {
            error: format!("无法从原始 HIR 找回 {} 容器正文", call.name),
            scopes: scopes.clone(),
        })?;
    let execution: BodyExecution = if call.name == "button" {
        button_with_body(
            &arguments,
            invocation.identity,
            source_call.body.as_slice(),
            invocation.captures,
            interactions,
        )
    } else {
        link_with_body(
            &arguments,
            invocation.identity,
            source_call.body.as_slice(),
            invocation.captures,
            interactions,
        )
    }
    .map_err(|error| EngineMirMacroCallbackFailure {
        error: format!("{} 执行失败：{error:?}", call.name),
        scopes: scopes.clone(),
    })?;
    Ok(MacroResumeOutcome::Complete {
        output: RuntimeMacroExecution {
            execution,
            includes_entered: 0,
        },
        scopes,
    })
}

pub(crate) fn find_hir_macro<'hir, 'source>(
    story: &'hir HirStory<'source>,
    owned: &OwnedHirMacro,
) -> Option<&'hir HirMacro<'source>> {
    story
        .passages
        .iter()
        .find_map(|passage| find_hir_macro_in_body(&passage.body, owned))
}

pub(crate) fn find_hir_macro_in_body<'hir, 'source>(
    body: &'hir [HirBodyNode<'source>],
    owned: &OwnedHirMacro,
) -> Option<&'hir HirMacro<'source>> {
    body.iter().find_map(|node| match &node.kind {
        HirBodyKind::Macro(call) if OwnedHirMacro::from(call) == *owned => Some(call),
        HirBodyKind::Macro(call) => find_hir_macro_in_body(&call.body, owned),
        HirBodyKind::If(conditional) => conditional
            .branches
            .iter()
            .find_map(|branch| find_hir_macro_in_body(&branch.body, owned))
            .or_else(|| {
                conditional
                    .fallback
                    .as_deref()
                    .and_then(|body| find_hir_macro_in_body(body, owned))
            }),
        HirBodyKind::Switch(switch) => switch
            .cases
            .iter()
            .find_map(|case| find_hir_macro_in_body(&case.body, owned))
            .or_else(|| {
                switch
                    .default
                    .as_deref()
                    .and_then(|body| find_hir_macro_in_body(body, owned))
            }),
        HirBodyKind::For(loop_node) => find_hir_macro_in_body(&loop_node.body, owned),
        HirBodyKind::While(loop_node) => find_hir_macro_in_body(&loop_node.body, owned),
        HirBodyKind::Silently(body) => find_hir_macro_in_body(body, owned),
        HirBodyKind::Widget(widget) => find_hir_macro_in_body(&widget.body, owned),
        HirBodyKind::Capture(capture) => find_hir_macro_in_body(&capture.body, owned),
        _ => None,
    })
}

pub fn macro_value_execution(value: &Value) -> Result<RuntimeMacroExecution, ScriptError> {
    // 脚本 bridge 产生协议 Surface；Core 宏执行输出需要语义表示，做同构反向转换。
    let surface: Surface = match narrava_loom_protocol::protocol_bridge::output(value)? {
        Some(output) => output,
        None => {
            let mut output = Surface::default();
            if let Some(text) = value_to_text(value) {
                output.push(SurfaceNode::Text(text));
            }
            output
        }
    };
    Ok(RuntimeMacroExecution {
        execution: BodyExecution {
            control: narrava_loom_core::runtime::BodyControl::Continue,
            output: SemanticOutput::from(&surface),
        },
        includes_entered: 0,
    })
}

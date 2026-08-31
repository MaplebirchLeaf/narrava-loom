//! Reaction 结构化效果到现有 Twee Runtime 语义的唯一适配层。

use narrava_loom_core::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    hir::HirStory,
    macro_runtime::{
        MacroDefinitions, MacroLocalScopes, MacroStoryAccess, parse_fragment,
        register_story_widgets, replace,
    },
    reaction::ReactionEffect,
    runtime::{BodyControl, RuntimeExecutionContext},
    semantic::SemanticOutput,
    state::State,
    story::StoryRuntimeRequests,
};

use crate::ScriptAdapter;

pub(crate) fn apply_lifecycle_reactions<'hir, 'source>(
    script: &impl ScriptAdapter,
    hir: &'hir HirStory<'source>,
    passage: &narrava_loom_core::hir::HirPassage<'source>,
    state: &mut State,
    requests: &mut StoryRuntimeRequests<'_, 'hir, 'source>,
) -> Result<narrava_loom_core::runtime::BodyExecution, Diagnostic> {
    let reactions = script
        .resolve_lifecycle_reactions(passage, state)
        .map_err(|error| reaction_error(&error.code, error.message))?;
    let mut combined = narrava_loom_core::runtime::BodyExecution::default();
    for reaction in reactions {
        let execution = apply_effect(hir, &reaction, state, requests)?;
        combined.output.append(execution.output);
        combined.control = match (combined.control, execution.control) {
            (BodyControl::StopPassage, _) | (_, BodyControl::StopPassage) => {
                BodyControl::StopPassage
            }
            (BodyControl::ExitScope, _) | (_, BodyControl::ExitScope) => BodyControl::ExitScope,
            _ => BodyControl::Continue,
        };
    }
    Ok(combined)
}

/// 固定应用顺序：内容 → replace → goto；没有 goto 时才应用 exit。
pub(crate) fn apply_effect<'hir, 'source>(
    hir: &'hir HirStory<'source>,
    effect: &ReactionEffect,
    state: &mut State,
    requests: &mut StoryRuntimeRequests<'_, 'hir, 'source>,
) -> Result<narrava_loom_core::runtime::BodyExecution, Diagnostic> {
    let execution = apply_content(hir, effect, state, requests)?;
    let mut output: SemanticOutput = execution.output;
    let mut control: BodyControl = execution.control;

    if effect.goto.is_some() && requests.pending_goto().is_some() {
        return Err(reaction_error(
            "reaction.multiple_goto",
            "Reaction 内容与 goto 字段不能同时发起导航",
        ));
    }
    if let Some(target) = &effect.replace {
        output = replace(target, output)?.output;
    }
    if let Some(target) = &effect.goto {
        requests
            .goto(target)
            .map_err(|error| reaction_error("reaction.goto", error.to_string()))?;
        control = BodyControl::StopPassage;
    } else if effect.exit && control != BodyControl::StopPassage {
        control = BodyControl::ExitScope;
    }
    Ok(narrava_loom_core::runtime::BodyExecution { output, control })
}

fn apply_content<'hir, 'source>(
    hir: &'hir HirStory<'source>,
    effect: &ReactionEffect,
    state: &mut State,
    requests: &mut StoryRuntimeRequests<'_, 'hir, 'source>,
) -> Result<narrava_loom_core::runtime::BodyExecution, Diagnostic> {
    let mut definitions = MacroDefinitions::new();
    register_story_widgets::<()>(&mut definitions, hir);
    let mut scopes: MacroLocalScopes<narrava_loom_core::expression::value::Value> =
        MacroLocalScopes::new();
    let mut runtime = RuntimeExecutionContext::new(&definitions, state, requests, &mut scopes);
    let execution = match (&effect.widget, &effect.include) {
        (Some(widget), None) => {
            let fragment = parse_fragment(widget).map_err(|error| error.diagnostic)?;
            runtime.execute_fragment(fragment.nodes())
        }
        (None, Some(include)) => {
            let passage = hir.passage(include).ok_or_else(|| {
                reaction_error(
                    "reaction.include",
                    format!("Reaction include 不存在：{include}"),
                )
            })?;
            runtime.execute_passage_with_includes(passage, 256)
        }
        (None, None) => return Ok(narrava_loom_core::runtime::BodyExecution::default()),
        (Some(_), Some(_)) => unreachable!("Reaction 注册时已拒绝多个内容来源"),
    }
    .map_err(|error| reaction_error("reaction.content", format!("{error:?}")))?;
    if !matches!(
        execution.control,
        BodyControl::Continue | BodyControl::ExitScope | BodyControl::StopPassage
    ) {
        return Err(reaction_error(
            "reaction.control",
            format!("Reaction 内容返回了非法控制信号：{:?}", execution.control),
        ));
    }
    Ok(execution)
}

fn reaction_error(code: &str, message: impl AsRef<str>) -> Diagnostic {
    Diagnostic::new(code, DiagnosticSeverity::Error, message.as_ref())
}

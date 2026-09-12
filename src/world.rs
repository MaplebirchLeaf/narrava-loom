//! 世界领域类型与 Passage 地点标签的 Core 适配。

pub use narrava_loom_world::{
    Environment, MAX_COORDINATE, Place, Point, World, WorldError, WorldPosition, WorldState,
};

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    hir::{HirPassage, HirStory},
    state::State,
    story::special::START_PASSAGE,
};

/// 已注册地点的 ID 是普通 Passage tag；其他作者标签保留原有含义。
pub fn passage_place<'world>(
    world: &'world World,
    passage: &HirPassage<'_>,
) -> Result<(Option<&'world Place>, Option<Environment>), Diagnostic> {
    let mut place: Option<&Place> = None;
    for tag in &passage.tags {
        if let Some(candidate) = world.get(tag) {
            if place.is_some_and(|previous: &Place| previous.id != candidate.id) {
                return Err(passage_error(passage, "一个 Passage 只能绑定一个地点 ID"));
            }
            place = Some(candidate);
        }
    }
    let environment: Option<Environment> =
        match (passage.has_tag("inside"), passage.has_tag("outside")) {
            (true, true) => return Err(passage_error(passage, "inside 与 outside 不能同时使用")),
            (true, false) => Some(Environment::Inside),
            (false, true) => Some(Environment::Outside),
            (false, false) => None,
        };
    Ok((place, environment))
}

/// 注册完成后统一检查标签，避免作者进入某条分支时才发现歧义。
pub fn validate_passages(world: &World, story: &HirStory<'_>) -> Result<(), Diagnostic> {
    for passage in &story.passages {
        let _binding = passage_place(world, passage)?;
    }
    Ok(())
}

/// 普通导航进入地点；include、widget 与 Host 辅助区域不调用此入口。
pub fn enter_passage(state: &mut State, passage: &HirPassage<'_>) -> Result<(), Diagnostic> {
    if passage.name == START_PASSAGE {
        *state.world_state_mut() = WorldState::default();
        return Ok(());
    }
    let (place, environment) = passage_place(state.world(), passage)?;
    if place.is_none() && environment.is_none() {
        return Ok(());
    }
    let mut next: WorldState = state.world_state().clone();
    if let Some(place) = place {
        state
            .world()
            .enter(&mut next, &place.id, environment)
            .map_err(|error: WorldError| {
                Diagnostic::new(error.code(), DiagnosticSeverity::Error, &error.to_string())
            })?;
    } else if let Some(environment) = environment
        && let Some(position) = next.position.as_mut()
    {
        position.environment = Some(environment);
    }
    *state.world_state_mut() = next;
    Ok(())
}

fn passage_error(passage: &HirPassage<'_>, message: &str) -> Diagnostic {
    Diagnostic::new(
        "world.passage_tags",
        DiagnosticSeverity::Error,
        &format!("Passage {}：{message}", passage.name),
    )
}

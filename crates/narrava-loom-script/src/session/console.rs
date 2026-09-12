//! 开发脚本结果、Story 查询和导航；继续使用普通事务与 Pending/Resume。
use super::*;
use crate::console::{ConsoleNavigation, ConsoleOutcome};

impl<'hir, 'source> RuntimeSession<'hir, 'source> {
    pub(super) fn execute_console(&mut self, source: &str) -> Result<RuntimeUpdate, HostErrorDto> {
        let passages: Vec<serde_json::Value> = self
            .hir
            .passages
            .iter()
            .map(|passage| serde_json::json!({"name": passage.name, "tags": passage.tags}))
            .collect();
        let visits: serde_json::Map<String, serde_json::Value> = self
            .hir
            .passages
            .iter()
            .map(|passage| {
                (
                    passage.name.to_owned(),
                    serde_json::json!(self.story.visits(passage.name)),
                )
            })
            .collect();
        let story: serde_json::Value = serde_json::json!({
            "passages": passages,
            "visits": visits,
            "current": self.story.current().map(|passage| passage.name),
        });
        self.script
            .configure_console_story(story)
            .map_err(ScriptError::into_host_error)?;
        let outcome: ConsoleOutcome = self
            .script
            .evaluate_console(source, &mut self.state)
            .map_err(ScriptError::into_host_error)?;
        self.finish_console(outcome)
    }

    pub(super) fn finish_console(
        &mut self,
        outcome: ConsoleOutcome,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let value = match outcome {
            ConsoleOutcome::Pending(pending) => {
                let operation = protocol_pending(&pending);
                self.pending = Some(Pending::Console(pending));
                return Ok(RuntimeUpdate::Pending { operation });
            }
            ConsoleOutcome::Complete(value) => value,
        };
        self.console_sequence = self.console_sequence.saturating_add(1);
        self.console_result = Some(narrava_loom_protocol::HostDebugEvaluationDto {
            sequence: self.console_sequence,
            value,
        });
        match self
            .script
            .take_console_navigation()
            .map_err(ScriptError::into_host_error)?
        {
            None => Ok(RuntimeUpdate::Applied),
            Some(ConsoleNavigation::Goto { target }) => self.navigate(&target),
            Some(ConsoleNavigation::Back) => self.history(true),
            Some(ConsoleNavigation::Forward) => self.history(false),
            Some(ConsoleNavigation::Restart) => {
                self.state
                    .restore_checkpoint(self.initial_state.checkpoint());
                self.script
                    .restore_reaction_state(&self.initial_reactions)
                    .map_err(ScriptError::into_host_error)?;
                self.story = Story::new(self.hir);
                self.presented = None;
                self.interactions = MacroInteractions::new();
                self.transaction.as_mut().expect("控制台拥有事务").before =
                    Rc::new(self.state.snapshot());
                self.start()
            }
        }
    }
}

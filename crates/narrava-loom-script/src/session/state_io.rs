//! Save 与语言选择的平台适配边界。

use std::rc::Rc;

use narrava_loom_core::{
    GameIdentity,
    i18n::{I18nCatalog, I18nRuntimeLanguage, NlangValidatedPackage},
    reaction::ReactionRuntimeState,
    save::SaveDocument,
    state::State,
    story::Story,
};
use narrava_loom_protocol::{
    HostErrorDto, PendingOperation, PendingResult, RuntimeUpdate, SaveOperation,
};

use super::{HostAction, HostOperation, InputCheckpoint, Pending, RuntimeSession};

/// Runtime 自己持有的 Save/I18n 数据；不包含路径、文件句柄或 UI。
pub struct RuntimeData {
    game: GameIdentity,
    catalog: I18nCatalog,
    default_locale: String,
    language_packages: Vec<NlangValidatedPackage>,
}

impl RuntimeData {
    pub fn new(
        game: GameIdentity,
        catalog: I18nCatalog,
        default_locale: String,
        language_packages: Vec<NlangValidatedPackage>,
    ) -> Self {
        Self {
            game,
            catalog,
            default_locale,
            language_packages,
        }
    }

    fn prepare_save<'hir, 'source>(
        &self,
        operation: SaveOperation,
        state: &State,
        story: &Story<'hir, 'source>,
        reactions: &[ReactionRuntimeState],
    ) -> Result<Option<Vec<u8>>, HostErrorDto> {
        if operation == SaveOperation::Import {
            return Ok(None);
        }
        SaveDocument::capture(&self.game, state, story)
            .map(|document| document.with_reactions(reactions.to_vec()))
            .and_then(|document| document.to_bytes())
            .map(Some)
            .map_err(|error| HostErrorDto::new("runtime_session.save", error.to_string()))
    }

    fn complete_save<'hir, 'source>(
        &self,
        operation: SaveOperation,
        document: Option<Vec<u8>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
    ) -> Result<Option<Vec<ReactionRuntimeState>>, HostErrorDto> {
        if operation == SaveOperation::Export {
            return Ok(None);
        }
        let document = document.ok_or_else(|| {
            HostErrorDto::new("runtime_session.save_document", "Save import 缺少存档内容")
        })?;
        let document = SaveDocument::from_bytes(&document)
            .map_err(|error| HostErrorDto::new("runtime_session.save", error.to_string()))?;
        document
            .restore(&self.game, state, story)
            .map_err(|error| HostErrorDto::new("runtime_session.save", error.to_string()))?;
        Ok(Some(document.reactions().to_vec()))
    }

    fn select_language(&self, locale: &str) -> Result<Option<I18nRuntimeLanguage>, HostErrorDto> {
        I18nRuntimeLanguage::select(
            &self.catalog,
            &self.default_locale,
            locale,
            self.language_packages.clone(),
        )
        .map_err(|error| HostErrorDto::new("runtime_session.language_select", error.to_string()))
    }
}

impl<'hir, 'source> RuntimeSession<'hir, 'source> {
    pub(super) fn begin_save(
        &mut self,
        operation: SaveOperation,
        target: String,
        after: RuntimeUpdate,
        script_save: bool,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let document: Option<Vec<u8>> = self
            .data
            .as_ref()
            .ok_or_else(|| unsupported("save"))?
            .prepare_save(
                operation,
                &self.state,
                &self.story,
                &self.script.reaction_state(),
            )?;
        let operation_id: u64 = self.sequence;
        self.sequence = self.sequence.saturating_add(1);
        let pending = PendingOperation::Save {
            operation: operation_id,
            direction: operation,
            target: target.clone(),
            document,
        };
        self.pending = Some(Pending::Host(Box::new(HostOperation {
            operation: operation_id,
            action: HostAction::Save { operation, target },
            after,
            script_save,
            input_checkpoint: None,
        })));
        Ok(RuntimeUpdate::Pending { operation: pending })
    }

    pub(super) fn begin_language(
        &mut self,
        locale: String,
        after: RuntimeUpdate,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let operation_id: u64 = self.sequence;
        self.sequence = self.sequence.saturating_add(1);
        self.pending = Some(Pending::Host(Box::new(HostOperation {
            operation: operation_id,
            action: HostAction::SelectLanguage {
                locale: locale.clone(),
            },
            after,
            script_save: false,
            input_checkpoint: None,
        })));
        Ok(RuntimeUpdate::Pending {
            operation: PendingOperation::SelectLanguage {
                operation: operation_id,
                locale,
            },
        })
    }

    fn select_language(&mut self, locale: &str) -> Result<(), HostErrorDto> {
        let language: Option<I18nRuntimeLanguage> = self
            .data
            .as_ref()
            .ok_or_else(|| unsupported("language"))?
            .select_language(locale)?;
        self.script
            .select_locale(locale)
            .map_err(crate::ScriptError::into_host_error)?;
        self.language = language.map(Rc::new);
        Ok(())
    }

    pub(super) fn process_script_save(
        &mut self,
        after: RuntimeUpdate,
        input_checkpoint: &mut Option<InputCheckpoint<'hir, 'source>>,
    ) -> Result<Option<RuntimeUpdate>, HostErrorDto> {
        let Some((operation, target)) = self
            .script
            .take_save()
            .map_err(crate::ScriptError::into_host_error)?
        else {
            return Ok(None);
        };
        // 先完成编码，只有成功挂起后才转移检查点，保证立即失败也能回滚。
        let update: RuntimeUpdate = self.begin_save(operation, target, after, true)?;
        let Some(Pending::Host(waiting)) = self.pending.as_mut() else {
            unreachable!("begin_save 成功后必须持有平台保存操作")
        };
        waiting.input_checkpoint = input_checkpoint.take();
        Ok(Some(update))
    }

    pub(super) fn process_script_language(
        &mut self,
        after: RuntimeUpdate,
    ) -> Result<Option<RuntimeUpdate>, HostErrorDto> {
        let Some(locale) = self
            .script
            .take_language()
            .map_err(crate::ScriptError::into_host_error)?
        else {
            return Ok(None);
        };
        self.begin_language(locale, after).map(Some)
    }

    pub(super) fn resume_host(
        &mut self,
        waiting: HostOperation<'hir, 'source>,
        result: Option<PendingResult>,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let outcome: Result<(), HostErrorDto> = match &waiting.action {
            HostAction::Save { operation, .. } => match result {
                Some(PendingResult::Save { document }) => self.apply_save(*operation, document),
                Some(PendingResult::Failed { error }) => Err(error),
                _ => Err(platform_result_mismatch("save")),
            },
            HostAction::SelectLanguage { locale } => match result {
                Some(PendingResult::SelectLanguage) => self.select_language(locale),
                Some(PendingResult::Failed { error }) => Err(error),
                _ => Err(platform_result_mismatch("selectLanguage")),
            },
        };
        if outcome.is_err()
            && matches!(
                waiting.action,
                HostAction::Save {
                    operation: SaveOperation::Import,
                    ..
                }
            )
        {
            self.rollback_transaction();
            // Save.after observes the restored state; non-fatal notices may still settle events.
            self.begin_transaction();
        }
        if waiting.script_save {
            let completion: Result<(), HostErrorDto> =
                self.finish_script_save(&waiting.action, outcome.clone());
            if let Some(checkpoint) = waiting.input_checkpoint {
                if let Err(error) = completion.and(outcome) {
                    self.transaction = Some(checkpoint);
                    return Err(error);
                }
            } else {
                completion?;
                if let Err(error) = outcome {
                    self.notices.push(error);
                }
            }
            return Ok(waiting.after);
        }
        outcome?;
        if matches!(
            waiting.action,
            HostAction::Save {
                operation: SaveOperation::Import,
                ..
            } | HostAction::SelectLanguage { .. }
        ) && self.presented.is_some()
        {
            return self.replay(narrava_loom_core::host::HostReplayTarget::RefreshCurrent);
        }
        Ok(waiting.after)
    }

    fn apply_save(
        &mut self,
        operation: SaveOperation,
        document: Option<Vec<u8>>,
    ) -> Result<(), HostErrorDto> {
        let reactions = self
            .data
            .as_ref()
            .ok_or_else(|| unsupported("save"))?
            .complete_save(operation, document, &mut self.state, &mut self.story)?;
        if let Some(reactions) = reactions
            && let Err(error) = self.script.restore_reaction_state(&reactions)
        {
            return Err(error.into_host_error());
        }
        Ok(())
    }

    pub(super) fn finish_script_save(
        &mut self,
        action: &HostAction,
        outcome: Result<(), HostErrorDto>,
    ) -> Result<(), HostErrorDto> {
        let HostAction::Save { operation, target } = action else {
            return Ok(());
        };
        self.script
            .complete_save(
                operation.as_str(),
                target,
                outcome
                    .as_ref()
                    .map(|_| ())
                    .map_err(|error| error.message.as_str()),
                &mut self.state,
            )
            .map_err(crate::ScriptError::into_host_error)
    }
}

fn platform_result_mismatch(expected: &str) -> HostErrorDto {
    HostErrorDto::new(
        "runtime_session.platform_result_mismatch",
        format!("平台挂起操作需要 {expected} 完成结果"),
    )
}

fn unsupported(operation: &str) -> HostErrorDto {
    match operation {
        "save" => HostErrorDto::new(
            "runtime_session.save_unsupported",
            "当前 Host 没有提供存档 IO",
        ),
        _ => HostErrorDto::new(
            "runtime_session.language_unsupported",
            "当前 Host 没有提供语言包装载",
        ),
    }
}

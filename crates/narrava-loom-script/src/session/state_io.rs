//! Save 与语言选择的平台适配边界。

use std::rc::Rc;

use narrava_loom_core::{
    GameIdentity,
    i18n::{I18nCatalog, I18nRuntimeLanguage, NlangValidatedPackage},
    reaction::ReactionRuntimeState,
    save::SaveDocument,
    script::ScriptCallDispatcher,
    state::{State, StateCheckpoint},
    story::{Story, StorySnapshot},
};
use narrava_loom_protocol::{
    HostErrorDto, PendingOperation, PendingResult, RuntimeUpdate, SaveOperation,
};

use super::{PlatformAction, PlatformWaiting, RuntimeSession, Waiting};
use crate::ScriptAdapter;

/// Runtime 需要 Host 完成的平台 IO。
///
/// 实现只负责读写文件与装载语言包；调用顺序、Script hook、State 同步和当前语言
/// 提交由 [`RuntimeSession`] 统一管理。
pub(crate) trait RuntimePlatform<'hir, 'source> {
    fn prepare_save(
        &mut self,
        operation: SaveOperation,
        target: &str,
        state: &State,
        story: &Story<'hir, 'source>,
        reactions: &[ReactionRuntimeState],
    ) -> Result<Option<Vec<u8>>, HostErrorDto>;

    fn complete_save(
        &mut self,
        operation: SaveOperation,
        target: &str,
        document: Option<Vec<u8>>,
        state: &mut State,
        story: &mut Story<'hir, 'source>,
    ) -> Result<Option<Vec<ReactionRuntimeState>>, HostErrorDto>;

    fn select_language(
        &mut self,
        locale: &str,
    ) -> Result<Option<I18nRuntimeLanguage>, HostErrorDto>;
}

/// 未配置平台 IO 时使用的显式拒绝实现。
pub struct UnsupportedRuntimePlatform;

/// Runtime 自己持有的 Save/I18n 数据服务；不包含路径、文件句柄或 UI。
pub struct RuntimeServices {
    game: GameIdentity,
    catalog: I18nCatalog,
    default_locale: String,
    language_packages: Vec<NlangValidatedPackage>,
}

impl RuntimeServices {
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
}

impl<'hir, 'source> RuntimePlatform<'hir, 'source> for RuntimeServices {
    fn prepare_save(
        &mut self,
        operation: SaveOperation,
        _target: &str,
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

    fn complete_save(
        &mut self,
        operation: SaveOperation,
        _target: &str,
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

    fn select_language(
        &mut self,
        locale: &str,
    ) -> Result<Option<I18nRuntimeLanguage>, HostErrorDto> {
        I18nRuntimeLanguage::select(
            &self.catalog,
            &self.default_locale,
            locale,
            self.language_packages.clone(),
        )
        .map_err(|error| HostErrorDto::new("runtime_session.language_select", error.to_string()))
    }
}

impl<'hir, 'source> RuntimePlatform<'hir, 'source> for UnsupportedRuntimePlatform {
    fn prepare_save(
        &mut self,
        _operation: SaveOperation,
        _target: &str,
        _state: &State,
        _story: &Story<'hir, 'source>,
        _reactions: &[ReactionRuntimeState],
    ) -> Result<Option<Vec<u8>>, HostErrorDto> {
        Err(HostErrorDto::new(
            "runtime_session.save_unsupported",
            "当前 Host 没有提供存档 IO",
        ))
    }

    fn complete_save(
        &mut self,
        _operation: SaveOperation,
        _target: &str,
        _document: Option<Vec<u8>>,
        _state: &mut State,
        _story: &mut Story<'hir, 'source>,
    ) -> Result<Option<Vec<ReactionRuntimeState>>, HostErrorDto> {
        Err(HostErrorDto::new(
            "runtime_session.save_unsupported",
            "当前 Host 没有提供存档 IO",
        ))
    }

    fn select_language(
        &mut self,
        _locale: &str,
    ) -> Result<Option<I18nRuntimeLanguage>, HostErrorDto> {
        Err(HostErrorDto::new(
            "runtime_session.language_unsupported",
            "当前 Host 没有提供语言包装载",
        ))
    }
}

impl<'hir, 'source, Adapter: ScriptAdapter + ScriptCallDispatcher + 'static>
    RuntimeSession<'hir, 'source, Adapter>
{
    pub(super) fn begin_save(
        &mut self,
        operation: SaveOperation,
        target: String,
        after: RuntimeUpdate,
        script_save: bool,
        input_checkpoint: Option<StateCheckpoint>,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        self.ensure_idle()?;
        let document: Option<Vec<u8>> = self.platform.prepare_save(
            operation,
            &target,
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
        self.waiting = Some(Waiting::Platform(Box::new(PlatformWaiting {
            operation: operation_id,
            action: PlatformAction::Save { operation, target },
            after,
            script_save,
            input_checkpoint,
        })));
        Ok(RuntimeUpdate::Pending { operation: pending })
    }

    pub(super) fn begin_language(
        &mut self,
        locale: String,
        after: RuntimeUpdate,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        self.ensure_idle()?;
        let operation_id: u64 = self.sequence;
        self.sequence = self.sequence.saturating_add(1);
        self.waiting = Some(Waiting::Platform(Box::new(PlatformWaiting {
            operation: operation_id,
            action: PlatformAction::SelectLanguage {
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
        let language: Option<I18nRuntimeLanguage> = self.platform.select_language(locale)?;
        self.script
            .select_locale(locale)
            .map_err(|error| HostErrorDto::new(&error.code, error.message))?;
        self.language = language.map(Rc::new);
        Ok(())
    }

    pub(super) fn process_script_save(
        &mut self,
        after: RuntimeUpdate,
        input_checkpoint: &mut Option<StateCheckpoint>,
    ) -> Result<Option<RuntimeUpdate>, HostErrorDto> {
        let Some((operation, target)) = self
            .script
            .take_save()
            .map_err(|error| HostErrorDto::new(&error.code, error.message))?
        else {
            return Ok(None);
        };
        let operation_kind: SaveOperation = match operation.as_str() {
            "export" => SaveOperation::Export,
            "import" => SaveOperation::Import,
            _ => {
                return Err(HostErrorDto::new(
                    "runtime_session.save_operation",
                    format!("未知 Script Save 操作：{operation}"),
                ));
            }
        };
        self.begin_save(operation_kind, target, after, true, input_checkpoint.take())
            .map(Some)
    }

    pub(super) fn resume_platform(
        &mut self,
        waiting: PlatformWaiting,
        result: Option<PendingResult>,
    ) -> Result<RuntimeUpdate, HostErrorDto> {
        let import_checkpoint: Option<(
            StateCheckpoint,
            StorySnapshot<'hir, 'source>,
            Vec<ReactionRuntimeState>,
        )> = matches!(
            &waiting.action,
            PlatformAction::Save {
                operation: SaveOperation::Import,
                ..
            }
        )
        .then(|| {
            (
                self.state.checkpoint(),
                self.story.snapshot(),
                self.script.reaction_state(),
            )
        });
        let mut import_checkpoint = import_checkpoint;
        let outcome: Result<(), HostErrorDto> = match &waiting.action {
            PlatformAction::Save { operation, target } => match result {
                Some(PendingResult::Save { document }) => {
                    self.apply_save(*operation, target, document)
                }
                Some(PendingResult::Failed { error }) => Err(error),
                _ => Err(platform_result_mismatch("save")),
            },
            PlatformAction::SelectLanguage { locale } => match result {
                Some(PendingResult::SelectLanguage) => self.select_language(locale),
                Some(PendingResult::Failed { error }) => Err(error),
                _ => Err(platform_result_mismatch("selectLanguage")),
            },
        };
        if outcome.is_err()
            && let Some((state_checkpoint, story_snapshot, reaction_checkpoint)) =
                import_checkpoint.take()
        {
            self.rollback_import(Some(state_checkpoint), Some(story_snapshot))?;
            self.script
                .restore_reaction_state(&reaction_checkpoint)
                .map_err(|error| HostErrorDto::new(&error.code, error.message))?;
        }
        if waiting.script_save {
            if let Err(error) = self.finish_script_save(&waiting.action, outcome.clone()) {
                if outcome.is_ok()
                    && let Some((state_checkpoint, story_snapshot, reaction_checkpoint)) =
                        import_checkpoint
                {
                    self.rollback_import(Some(state_checkpoint), Some(story_snapshot))?;
                    self.script
                        .restore_reaction_state(&reaction_checkpoint)
                        .map_err(|error| HostErrorDto::new(&error.code, error.message))?;
                }
                return Err(error);
            }
            if let Err(error) = outcome {
                if let Some(checkpoint) = waiting.input_checkpoint {
                    self.state.restore_checkpoint(checkpoint);
                    self.sync_script_variables()?;
                    return Err(error);
                }
                self.notices.push(error);
            }
            return Ok(waiting.after);
        }
        outcome?;
        Ok(waiting.after)
    }

    fn apply_save(
        &mut self,
        operation: SaveOperation,
        target: &str,
        document: Option<Vec<u8>>,
    ) -> Result<(), HostErrorDto> {
        let reactions = self.platform.complete_save(
            operation,
            target,
            document,
            &mut self.state,
            &mut self.story,
        )?;
        if let Some(reactions) = reactions
            && let Err(error) = self.script.restore_reaction_state(&reactions)
        {
            return Err(HostErrorDto::new(&error.code, error.message));
        }
        if operation == SaveOperation::Import
            && let Err(error) = self.sync_script_variables()
        {
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn finish_script_save(
        &self,
        action: &PlatformAction,
        outcome: Result<(), HostErrorDto>,
    ) -> Result<(), HostErrorDto> {
        let PlatformAction::Save { operation, target } = action else {
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
            )
            .map_err(|error| HostErrorDto::new(&error.code, error.message))
    }

    /// 恢复 Import 之前的 Core 与 Script 状态，避免任一步骤失败留下半提交存档。
    fn rollback_import(
        &mut self,
        state_checkpoint: Option<StateCheckpoint>,
        story_snapshot: Option<StorySnapshot<'hir, 'source>>,
    ) -> Result<(), HostErrorDto> {
        let state_checkpoint: StateCheckpoint = state_checkpoint.expect("Import 必须捕获 State");
        let story_snapshot: StorySnapshot<'hir, 'source> =
            story_snapshot.expect("Import 必须捕获 Story");
        self.state.restore_checkpoint(state_checkpoint);
        self.story
            .restore(story_snapshot)
            .expect("同一 RuntimeSession 捕获的 Story 快照必须可恢复");
        self.sync_script_variables().map_err(|error| {
            HostErrorDto::new(
                "runtime_session.save_rollback",
                format!(
                    "存档导入失败，Core 已回滚，但 Script 状态恢复失败：{}",
                    error.message
                ),
            )
        })
    }

    fn sync_script_variables(&self) -> Result<(), HostErrorDto> {
        self.script
            .sync_variables(&self.state)
            .map_err(|error| HostErrorDto::new(&error.code, error.message))
    }
}

fn platform_result_mismatch(expected: &str) -> HostErrorDto {
    HostErrorDto::new(
        "runtime_session.platform_result_mismatch",
        format!("平台挂起操作需要 {expected} 完成结果"),
    )
}

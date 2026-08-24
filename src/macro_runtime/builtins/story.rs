//! 请求 Story 执行 Passage 操作的原生逻辑 Macro。

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    expression::{
        Expression, Span,
        evaluator::{EvalError, evaluate_with_mut, value_to_text},
        value::{TextValue, Value},
    },
    runtime::BodyControl,
};

use super::super::{MacroLogicContext, MacroStoryAccess};

/// Story 逻辑 Macro 在求值和请求阶段可能产生的问题。
#[derive(Debug, PartialEq)]
pub enum StoryMacroError<StoryError> {
    Evaluation(EvalError),
    InvalidPassageName(Span),
    MissingPassage { name: String, span: Span },
    Story(StoryError),
}

impl<StoryError> StoryMacroError<StoryError> {
    /// Expression 相关错误保留局部 Span；Story 内部错误没有虚构位置。
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Evaluation(error) => Some(error.span()),
            Self::InvalidPassageName(span) | Self::MissingPassage { span, .. } => Some(*span),
            Self::Story(_) => None,
        }
    }

    /// 转换为 Logger 和调试器可使用的稳定 Diagnostic。
    pub fn diagnostic(self, convert_story: impl FnOnce(StoryError) -> Diagnostic) -> Diagnostic {
        match self {
            Self::Evaluation(error) => error.diagnostic(),
            Self::InvalidPassageName(_) => Diagnostic::new(
                "macro.invalid_passage_name",
                DiagnosticSeverity::Error,
                "Passage 名称必须是可转换为文本的标量",
            ),
            Self::MissingPassage { name, .. } => Diagnostic::new(
                "macro.missing_passage",
                DiagnosticSeverity::Error,
                &format!("Passage `{name}` 不存在"),
            ),
            Self::Story(error) => convert_story(error),
        }
    }
}

/// 执行 `<<include passage>>`，在当前位置请求 Story 包含目标 Passage。
pub fn include<Story>(
    expression: &Expression<'_>,
    context: &mut MacroLogicContext<'_, Story>,
) -> Result<BodyControl, StoryMacroError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    let name: String = passage_name(expression, context)?;
    context
        .story_mut()
        .include(&name)
        .map_err(StoryMacroError::Story)?;
    Ok(BodyControl::Continue)
}

/// 执行 `<<goto passage>>`，请求导航并终止当前 Passage 的后续执行。
pub fn goto<Story>(
    expression: &Expression<'_>,
    context: &mut MacroLogicContext<'_, Story>,
) -> Result<BodyControl, StoryMacroError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    let name: String = passage_name(expression, context)?;
    context
        .story_mut()
        .goto(&name)
        .map_err(StoryMacroError::Story)?;
    Ok(BodyControl::StopPassage)
}

/// 两种 Story 操作共享完全相同的名称求值和大小写规则。
fn passage_name<Story>(
    expression: &Expression<'_>,
    context: &mut MacroLogicContext<'_, Story>,
) -> Result<String, StoryMacroError<Story::Error>>
where
    Story: MacroStoryAccess + ?Sized,
{
    let value: Value =
        evaluate_with_mut(expression, context).map_err(StoryMacroError::Evaluation)?;
    let text: TextValue =
        value_to_text(&value).ok_or(StoryMacroError::InvalidPassageName(expression.span))?;
    let name: String = text
        .to_unicode_string()
        .ok_or(StoryMacroError::InvalidPassageName(expression.span))?;
    if context.story().has(&name) {
        Ok(name)
    } else {
        Err(StoryMacroError::MissingPassage {
            name,
            span: expression.span,
        })
    }
}

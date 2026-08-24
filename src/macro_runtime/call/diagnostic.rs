//! Macro 调用准备错误的 Diagnostic 与 Twee Source 定位。

use crate::{
    diagnostic::{Diagnostic, DiagnosticLocationError, DiagnosticLocator},
    expression::{Span, evaluator::EvalError},
};

use super::super::MacroArgumentIssue;
use super::MacroCallPreparationError;

/// Definition 查询与参数准备共用的 Diagnostic 结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroCallPreparationIssue {
    pub diagnostic: Diagnostic,
    /// 仅参数错误拥有相对 Macro 参数原文的 Span。
    pub span: Option<Span>,
}

impl MacroCallPreparationIssue {
    /// 参数错误映射回 Twee Source；无参数 Span 时保留无位置 Diagnostic。
    pub fn locate(
        self,
        locator: &DiagnosticLocator<'_>,
        fragment_start: usize,
    ) -> Result<Diagnostic, DiagnosticLocationError> {
        let Some(span) = self.span else {
            return Ok(self.diagnostic);
        };
        let location = locator.locate(fragment_start, span.start, span.end)?;
        Ok(self.diagnostic.with_location(location))
    }
}

impl MacroCallPreparationError<EvalError> {
    /// 统一 Definition、参数解析与参数求值错误，同时保留参数片段 Span。
    pub fn issue(self, name: &str, source_len: usize) -> MacroCallPreparationIssue {
        match self {
            Self::Definition(error) => MacroCallPreparationIssue {
                diagnostic: error.diagnostic(name),
                span: None,
            },
            Self::ArgumentList(error) => preparation_argument_issue(error.issue(source_len)),
            Self::ArgumentValue(error) => preparation_argument_issue(error.issue(source_len)),
        }
    }
}

fn preparation_argument_issue(issue: MacroArgumentIssue) -> MacroCallPreparationIssue {
    MacroCallPreparationIssue {
        diagnostic: issue.diagnostic,
        span: Some(issue.span),
    }
}

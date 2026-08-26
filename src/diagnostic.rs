//! 跨 Lexer、Parser 与 Runtime 共用的结构化诊断数据。

use std::fmt;

/// Diagnostic 对游戏继续运行的影响程度。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Note,
}

/// Diagnostic 对应的相对源码位置。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticLocation {
    pub source: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

/// Expression 等源码片段映射失败的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticLocationError {
    InvalidRange,
    InvalidUtf8Boundary,
}

/// 将片段内的 UTF-8 字节范围映射回完整 Source。
#[derive(Clone, Copy, Debug)]
pub struct DiagnosticLocator<'source> {
    source: &'source str,
    content: &'source str,
}

impl<'source> DiagnosticLocator<'source> {
    /// 用完整 Source 文本与当前片段内容建立定位器。
    pub fn new(source: &'source str, content: &'source str) -> Self {
        Self { source, content }
    }

    /// `fragment_start` 和局部范围均使用 UTF-8 字节偏移。
    pub fn locate(
        &self,
        fragment_start: usize,
        local_start: usize,
        local_end: usize,
    ) -> Result<DiagnosticLocation, DiagnosticLocationError> {
        if local_start > local_end || fragment_start > self.content.len() {
            return Err(DiagnosticLocationError::InvalidRange);
        }

        let start: usize = fragment_start
            .checked_add(local_start)
            .ok_or(DiagnosticLocationError::InvalidRange)?;
        let end: usize = fragment_start
            .checked_add(local_end)
            .ok_or(DiagnosticLocationError::InvalidRange)?;

        if end > self.content.len() {
            return Err(DiagnosticLocationError::InvalidRange);
        }
        if !self.content.is_char_boundary(fragment_start)
            || !self.content.is_char_boundary(start)
            || !self.content.is_char_boundary(end)
        {
            return Err(DiagnosticLocationError::InvalidUtf8Boundary);
        }

        let prefix: &str = &self.content[..start];
        let line: usize = prefix.bytes().filter(|byte: &u8| *byte == b'\n').count() + 1;
        let line_prefix: &str = prefix
            .rsplit_once('\n')
            .map_or(prefix, |(_, tail): (&str, &str)| tail);
        let column: usize = line_prefix.chars().count() + 1;

        Ok(DiagnosticLocation {
            source: self.source.to_owned(),
            start,
            end,
            line,
            column,
        })
    }
}

/// 可返回给编译器、调试器或 Logger 的结构化问题。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub location: Option<DiagnosticLocation>,
}

impl Diagnostic {
    /// 建立尚未附加源码位置的 Diagnostic。
    pub fn new(code: &str, severity: DiagnosticSeverity, message: &str) -> Self {
        Self {
            code: code.to_owned(),
            severity,
            message: message.to_owned(),
            location: None,
        }
    }

    /// 附加调用方已经转换好的公共源码位置。
    pub fn with_location(mut self, location: DiagnosticLocation) -> Self {
        self.location = Some(location);
        self
    }
}

/// 为 CLI 和 Host 提供同一种紧凑表示，避免每层再加一次“某阶段失败”。
impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(location) = &self.location {
            write!(
                formatter,
                "{}:{}:{}: [{}] {}",
                location.source, location.line, location.column, self.code, self.message
            )
        } else {
            write!(formatter, "[{}] {}", self.code, self.message)
        }
    }
}

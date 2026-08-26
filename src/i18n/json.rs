//! I18n 所用 JSON 数据的稳定错误边界。

use std::fmt;

use serde_json::error::Category;

/// JSON 失败的稳定分类，避免公开 API 直接依赖 `serde_json::Error`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum I18nJsonErrorKind {
    Syntax,
    Data,
    EndOfInput,
    Encode,
}

/// 保留可供 Diagnostic 展示的位置与底层说明。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I18nJsonError {
    kind: I18nJsonErrorKind,
    line: usize,
    column: usize,
    message: String,
}

impl I18nJsonError {
    /// 错误的稳定分类。
    pub fn kind(&self) -> I18nJsonErrorKind {
        self.kind
    }

    /// 出错位置的行号（1 基）。
    pub fn line(&self) -> usize {
        self.line
    }

    /// 出错位置的列号（1 基）。
    pub fn column(&self) -> usize {
        self.column
    }

    /// 底层错误说明。
    pub fn message(&self) -> &str {
        &self.message
    }

    /// 把解码错误包装为稳定分类；仅 crate 内部使用。
    pub(super) fn decode(error: serde_json::Error) -> Self {
        let kind: I18nJsonErrorKind = match error.classify() {
            Category::Syntax => I18nJsonErrorKind::Syntax,
            Category::Data => I18nJsonErrorKind::Data,
            Category::Eof => I18nJsonErrorKind::EndOfInput,
            // `from_str` 不执行 I/O；保留防御性映射，避免不可达分支 panic。
            Category::Io => I18nJsonErrorKind::Data,
        };
        Self {
            kind,
            line: error.line(),
            column: error.column(),
            message: error.to_string(),
        }
    }

    /// 把编码错误包装为 `Encode` 分类；仅 crate 内部使用。
    pub(super) fn encode(error: serde_json::Error) -> Self {
        Self {
            kind: I18nJsonErrorKind::Encode,
            line: error.line(),
            column: error.column(),
            message: error.to_string(),
        }
    }
}

impl fmt::Display for I18nJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for I18nJsonError {}

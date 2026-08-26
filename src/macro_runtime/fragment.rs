//! 动态 Twee 片段的公开解析入口（`Macro.parse()` 的 Rust 侧实现）。

use crate::diagnostic::Diagnostic;
use crate::hir::{self, HirBodyNode};
use crate::source::SourcePath;
use crate::twee;

/// 动态片段解析或降级失败；统一携带稳定 Diagnostic。
#[derive(Debug, PartialEq, Eq)]
pub struct FragmentParseError {
    pub diagnostic: Diagnostic,
}

impl From<twee::ParseError<'_>> for FragmentParseError {
    fn from(error: twee::ParseError<'_>) -> Self {
        Self {
            diagnostic: error.diagnostic(),
        }
    }
}

impl From<hir::HirError> for FragmentParseError {
    fn from(error: hir::HirError) -> Self {
        Self {
            diagnostic: error.diagnostic,
        }
    }
}

/// 已解析的动态 Twee 片段，可直接交给 Runtime 执行。
///
/// 片段不含 Passage 声明与语义上下文；通用 Macro 参数保持 Raw，
/// 由运行时 Macro 控制器按名称分派，不在此处静态绑定。
#[derive(Debug, PartialEq, Eq)]
pub struct ParsedFragment<'source> {
    nodes: Vec<HirBodyNode<'source>>,
}

impl ParsedFragment<'_> {
    /// 按源码顺序暴露可执行正文节点。
    pub fn nodes(&self) -> &[HirBodyNode<'_>] {
        &self.nodes
    }
}

/// 解析动态 Twee 片段为可执行正文（公开 `Macro.parse()` 入口）。
///
/// 输入是一段正文片段（不含 Passage 声明），例如
/// `<<link [[查看|X]]>><</link>>` 或 `<<print $name>>`；普通 `${...}` 仍是字面文本，
/// 只有显式 Macro 参数才进入 Expression 求值边界。片段使用虚拟来源定位，
/// 解析失败时返回携带稳定 Diagnostic 的 [`FragmentParseError`]。
pub fn parse_fragment(text: &str) -> Result<ParsedFragment<'_>, FragmentParseError> {
    let source: SourcePath = SourcePath::fragment();
    let nodes: Vec<twee::BodyNode<'_>> = twee::parse_fragment(text, &source)?;
    let nodes: Vec<HirBodyNode<'_>> = hir::lower_fragment(&nodes)?;
    Ok(ParsedFragment { nodes })
}

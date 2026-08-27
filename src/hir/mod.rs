//! 保留叙事结构并完成 Expression 解析的高层 IR。
//!
//! 类型定义与 Twee AST → HIR 的 lowering 分离：本模块保存 HIR 类型，
//! `lowering` 子模块负责从 Twee AST 转换。

mod lowering;
mod owned;

pub use lowering::lower_fragment;
pub use owned::*;

use crate::diagnostic::Diagnostic;
use crate::expression::Expression;
use crate::source::SourcePath;
use crate::twee;

/// HIR 中已经完成 Expression 解析的正文节点。
#[derive(Debug, PartialEq, Eq)]
pub struct HirBodyNode<'source> {
    pub kind: HirBodyKind<'source>,
    pub span: twee::Span,
}

/// 正文节点的具体语义；编译器认识的逻辑 Macro（if/for/switch/set 等）在此定型，
/// 其余通用 Macro 保留为未绑定的 [`HirMacro`] 调用。
#[derive(Debug, PartialEq, Eq)]
pub enum HirBodyKind<'source> {
    Text(&'source str),
    HardBreak,
    Print(HirPrint<'source>),
    Silently(Vec<HirBodyNode<'source>>),
    If(HirIf<'source>),
    Switch(Box<HirSwitch<'source>>),
    For(Box<HirFor<'source>>),
    While(Box<HirWhile<'source>>),
    Break,
    Continue,
    Exit,
    Set(Box<Expression<'source>>),
    Unset(Box<Expression<'source>>),
    Run(Box<Expression<'source>>),
    Include(Box<Expression<'source>>),
    Goto(Box<Expression<'source>>),
    Widget(HirWidget<'source>),
    /// 保留返回语法；可返回值调用域确定前不进入 Runtime 控制流。
    Return(Option<Box<Expression<'source>>>),
    Capture(HirCapture<'source>),
    Macro(HirMacro<'source>),
}

/// `print` 显式区分求值参数与反引号字面文本。
#[derive(Debug, PartialEq, Eq)]
pub enum HirPrint<'source> {
    /// 求值后输出其文本形式。
    Expression(Expression<'source>),
    /// 反引号包裹的原样字面文本。
    Literal(&'source str),
}

/// Widget 定义：声明名称与正文；调用处始终是 Inline Macro，不能携带调用正文。
#[derive(Debug, PartialEq, Eq)]
pub struct HirWidget<'source> {
    pub name: &'source str,
    pub body: Vec<HirBodyNode<'source>>,
}

/// Capture 只记录需要延长生命周期的当前 Macro 局部变量。
#[derive(Debug, PartialEq, Eq)]
pub struct HirCapture<'source> {
    pub locals: Vec<&'source str>,
    pub body: Vec<HirBodyNode<'source>>,
}

/// switch 结构：只求值一次的主值与按源码顺序严格比较的 case 列表。
#[derive(Debug, PartialEq, Eq)]
pub struct HirSwitch<'source> {
    pub value: Expression<'source>,
    pub cases: Vec<HirSwitchCase<'source>>,
    pub default: Option<Vec<HirBodyNode<'source>>>,
}

/// 一个 case 分支：与主值严格比较的 Expression 及其正文。
#[derive(Debug, PartialEq, Eq)]
pub struct HirSwitchCase<'source> {
    pub value: Expression<'source>,
    pub body: Vec<HirBodyNode<'source>>,
}

/// while 循环：每次迭代前求值的条件与循环正文。
#[derive(Debug, PartialEq, Eq)]
pub struct HirWhile<'source> {
    pub condition: Expression<'source>,
    pub body: Vec<HirBodyNode<'source>>,
}

/// for 循环：写入目标、迭代模式与循环正文。
#[derive(Debug, PartialEq, Eq)]
pub struct HirFor<'source> {
    pub target: HirForTarget<'source>,
    pub kind: HirForKind<'source>,
    pub body: Vec<HirBodyNode<'source>>,
}

/// for 循环的写入目标变量及其源码位置。
#[derive(Debug, PartialEq, Eq)]
pub struct HirForTarget<'source> {
    pub value: Expression<'source>,
    pub span: twee::Span,
}

/// for 的三种迭代模式：`in`（集合键）、`of`（集合值）与数值 `range`。
#[derive(Debug, PartialEq, Eq)]
pub enum HirForKind<'source> {
    In {
        collection: Expression<'source>,
        span: twee::Span,
    },
    Of {
        collection: Expression<'source>,
        span: twee::Span,
    },
    Range {
        start: Expression<'source>,
        start_span: twee::Span,
        end: Expression<'source>,
        end_span: twee::Span,
        step: Option<Expression<'source>>,
        step_span: Option<twee::Span>,
    },
}

/// if 结构：依次尝试的条件分支与可选的 fallback 正文。
#[derive(Debug, PartialEq, Eq)]
pub struct HirIf<'source> {
    pub branches: Vec<HirIfBranch<'source>>,
    pub fallback: Option<Vec<HirBodyNode<'source>>>,
}

/// if/elseif 的一个条件分支：条件与命中时的正文。
#[derive(Debug, PartialEq, Eq)]
pub struct HirIfBranch<'source> {
    pub condition: Expression<'source>,
    pub body: Vec<HirBodyNode<'source>>,
}

/// 通用 Macro HIR 不绑定当前 Runtime Macro Definitions 中的实现。
#[derive(Debug, PartialEq, Eq)]
pub struct HirMacro<'source> {
    pub name: &'source str,
    pub arguments: HirMacroArguments<'source>,
    pub syntax_kind: twee::MacroSyntaxKind,
    pub body: Vec<HirBodyNode<'source>>,
}

/// 只有编译器认识的逻辑 Macro 才会提前解析参数。
#[derive(Debug, PartialEq, Eq)]
pub enum HirMacroArguments<'source> {
    /// 不接受参数（如 `else`）。
    None,
    /// 参数原文，由运行时 Macro Definition 自行解析。
    Raw(&'source str),
    /// 编译器已解析为 Expression 的逻辑参数（if/while/run）。
    Expression(Expression<'source>),
}

/// HIR Passage 继续保留区分大小写的名称和源码身份。
#[derive(Debug, PartialEq, Eq)]
pub struct HirPassage<'source> {
    pub source: &'source SourcePath,
    pub name: &'source str,
    pub tags: Vec<&'source str>,
    pub body: Vec<HirBodyNode<'source>>,
}

impl HirPassage<'_> {
    /// Tag 是区分大小写的作者数据；这里只提供精确查询，不解释游戏语义。
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(&tag)
    }

    /// Widget Passage 只保存定义；顶层文本不进入 Surface 输出。
    pub fn emits_text(&self) -> bool {
        !self.has_tag("widget")
    }
}

/// 一份 Twee Story 编译后的完整 HIR 集合。
#[derive(Debug, PartialEq, Eq)]
pub struct HirStory<'source> {
    pub passages: Vec<HirPassage<'source>>,
}

/// HIR 转换错误统一携带已经定位的 Diagnostic。
#[derive(Debug, PartialEq, Eq)]
pub struct HirError {
    pub diagnostic: Diagnostic,
}

impl<'source> HirStory<'source> {
    /// 按区分大小写的 PassageName 查询编译结果。
    pub fn passage(&self, name: &str) -> Option<&HirPassage<'source>> {
        self.passages
            .iter()
            .find(|passage: &&HirPassage<'source>| passage.name == name)
    }
}

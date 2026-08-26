//! 动态 Twee Fragment lowering 与 `print` 共享参数语义。

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    expression::parse as parse_expression,
    twee,
};

use super::super::{HirBodyKind, HirBodyNode, HirError, HirMacro, HirMacroArguments, HirPrint};
use super::has_print_options;

/// print 参数解析失败的类型：缺少参数，或表达式错误（含片段起始偏移）。
pub(super) enum PrintLoweringError {
    Required,
    Expression {
        offset: usize,
        error: crate::expression::ParseError,
    },
}

/// 区分反引号字面文本与 `${}` 表达式，解析 print 参数。
pub(super) fn lower_print_argument<'source>(
    arguments: &'source str,
) -> Result<HirPrint<'source>, PrintLoweringError> {
    let arguments: &'source str = arguments.trim();
    if arguments.len() >= 2 && arguments.starts_with('`') && arguments.ends_with('`') {
        return Ok(HirPrint::Literal(&arguments[1..arguments.len() - 1]));
    }
    let (expression_source, offset): (&str, usize) =
        if arguments.starts_with("${") && arguments.ends_with('}') {
            (&arguments[2..arguments.len() - 1], 2)
        } else {
            (arguments, 0)
        };
    if expression_source.is_empty() {
        return Err(PrintLoweringError::Required);
    }
    parse_expression(expression_source)
        .map(HirPrint::Expression)
        .map_err(|error| PrintLoweringError::Expression { offset, error })
}

/// print 缺少参数时的公共 Diagnostic。
pub(super) fn print_argument_required_diagnostic() -> Diagnostic {
    Diagnostic::new(
        "hir.print_argument_required",
        DiagnosticSeverity::Error,
        "Macro `print` 需要 Expression、`${expression}` 或反引号字面文本",
    )
}

/// 把不含 Passage 声明的动态 Twee 节点降为 HIR 正文。
///
/// Fragment 没有 Passage 语义上下文：通用 Macro 参数保持 Raw，
/// 只有 `print` 与 `silently` 使用已确定的固有语义。
pub fn lower_fragment<'source>(
    nodes: &[twee::BodyNode<'source>],
) -> Result<Vec<HirBodyNode<'source>>, HirError> {
    nodes.iter().map(lower_fragment_node).collect()
}

/// 把单个动态 Twee 节点降为 HIR；通用 Macro 参数保持 Raw。
fn lower_fragment_node<'source>(
    node: &twee::BodyNode<'source>,
) -> Result<HirBodyNode<'source>, HirError> {
    let kind: HirBodyKind<'source> = match &node.kind {
        twee::BodyNodeKind::Text(text) => HirBodyKind::Text(text),
        twee::BodyNodeKind::Macro(macro_node) if macro_node.name == "print" => {
            if has_print_options(macro_node.arguments) {
                // 与正文路径一致：带样式选项的 print 走动态宏，单参数仍是编译器固有 Print。
                HirBodyKind::Macro(HirMacro {
                    name: macro_node.name,
                    arguments: HirMacroArguments::Raw(macro_node.arguments),
                    syntax_kind: macro_node.syntax_kind,
                    body: Vec::new(),
                })
            } else {
                let print: HirPrint<'source> =
                    lower_print_argument(macro_node.arguments).map_err(|error| HirError {
                        diagnostic: match error {
                            PrintLoweringError::Required => print_argument_required_diagnostic(),
                            PrintLoweringError::Expression { error, .. } => error.diagnostic(),
                        },
                    })?;
                HirBodyKind::Print(print)
            }
        }
        twee::BodyNodeKind::Macro(macro_node) if macro_node.name == "silently" => {
            if !macro_node.arguments.is_empty() {
                return Err(HirError {
                    diagnostic: Diagnostic::new(
                        "hir.unexpected_macro_arguments",
                        DiagnosticSeverity::Error,
                        "Macro `silently` 不接受参数",
                    ),
                });
            }
            HirBodyKind::Silently(lower_fragment(&macro_node.body)?)
        }
        twee::BodyNodeKind::Macro(macro_node) => {
            let body: Vec<HirBodyNode<'source>> = lower_fragment(&macro_node.body)?;
            HirBodyKind::Macro(HirMacro {
                name: macro_node.name,
                arguments: HirMacroArguments::Raw(macro_node.arguments),
                syntax_kind: macro_node.syntax_kind,
                body,
            })
        }
    };

    Ok(HirBodyNode {
        kind,
        span: node.span,
    })
}

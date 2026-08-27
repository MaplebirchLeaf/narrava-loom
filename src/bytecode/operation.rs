//! 拥有型 Bytecode 指令格式及其 MIR 编码。
//!
//! 本模块只负责单条指令的数据契约：把 MIR 中借用的文本、表达式和 Macro
//! 参数复制为可序列化数据，并暴露校验所需的槽位、跳转和操作数形状。
//! Passage 目录、常量目录与文件头仍由父模块管理，避免指令格式反向拥有程序容器。

use serde::{Deserialize, Serialize};

use super::Opcode;
use crate::{
    expression::{Expression, OwnedExpression},
    hir::{HirMacroArguments, OwnedHirMacroArguments},
    mir::{
        MirI18nTextPart, MirInstruction, MirInstructionPointer, MirIteratorSlot, MirOutputMode,
        MirValueSlot,
    },
    twee::{MacroSyntaxKind, Span},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// 指令关联的翻译消息身份。
///
/// `placeholder` 只存在于动态表达式片段；静态文本仍共享消息 ID，但不需要
/// Runtime 提供替换值。二者一起序列化，保证 VM 不必回看 MIR 或 HIR。
pub struct BytecodeI18nPart {
    id: String,
    placeholder: Option<String>,
}

impl BytecodeI18nPart {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn placeholder(&self) -> Option<&str> {
        self.placeholder.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Macro 参数在指令元数据中的形状。
///
/// 表达式本体存放在 `BytecodeInstruction::expressions`，这里仅保留参数类别，
/// 让解码校验能够确认表达式数量与拥有型 Macro 调用彼此一致。
pub enum BytecodeMacroArguments {
    /// 不接受参数。
    None,
    /// 参数原文，由运行时 Macro Definition 自行解析。
    Raw(String),
    /// 表达式本体保存在 `BytecodeInstruction::expressions` 中。
    Expression,
}

/// 不含任何 MIR/HIR 引用的指令元数据。
///
/// 该枚举保存控制流、输出模式、槽位和源码 Span 等直接执行信息。表达式和完整
/// Macro 调用单独保存在指令对象中，以便 VM 按统一顺序读取操作数，并在反序列化
/// 后交叉验证重复信息，拒绝被篡改或内部不一致的 Bytecode。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BytecodeOperation {
    Text {
        text: String,
        output: MirOutputMode,
        span: Span,
        i18n: Option<BytecodeI18nPart>,
    },
    HardBreak {
        output: MirOutputMode,
    },
    PrintExpression {
        output: MirOutputMode,
        span: Span,
        i18n: Option<BytecodeI18nPart>,
    },
    PrintLiteral {
        text: String,
        output: MirOutputMode,
        span: Span,
        i18n: Option<BytecodeI18nPart>,
    },
    EvaluateDiscard,
    Unset,
    RequestInclude {
        output: MirOutputMode,
    },
    RequestGoto,
    InvokeMacro {
        name: String,
        arguments: BytecodeMacroArguments,
        syntax_kind: MacroSyntaxKind,
        captures: Vec<String>,
        output: MirOutputMode,
    },
    ExitPassage,
    Evaluate {
        destination: MirValueSlot,
    },
    PrepareCollectionIteration {
        kind: crate::mir::MirCollectionIterationKind,
        destination: MirIteratorSlot,
    },
    PrepareRangeIteration {
        has_step: bool,
        destination: MirIteratorSlot,
    },
    NextIteration {
        iterator: MirIteratorSlot,
        exhausted: MirInstructionPointer,
    },
    JumpIfFalse {
        target: MirInstructionPointer,
    },
    JumpIfNotStrictEqual {
        left: MirValueSlot,
        target: MirInstructionPointer,
    },
    Jump {
        target: MirInstructionPointer,
    },
    Halt,
}

impl BytecodeOperation {
    /// 将一条 MIR 指令复制为没有借用生命周期的执行元数据。
    pub(super) fn from_mir(instruction: &MirInstruction<'_, '_>) -> Self {
        match instruction {
            MirInstruction::Text {
                text,
                output,
                span,
                i18n,
            } => Self::Text {
                text: (*text).to_owned(),
                output: *output,
                span: *span,
                i18n: i18n.as_ref().map(BytecodeI18nPart::from),
            },
            MirInstruction::HardBreak { output } => Self::HardBreak { output: *output },
            MirInstruction::PrintExpression {
                output, span, i18n, ..
            } => Self::PrintExpression {
                output: *output,
                span: *span,
                i18n: i18n.as_ref().map(BytecodeI18nPart::from),
            },
            MirInstruction::PrintLiteral {
                text,
                output,
                span,
                i18n,
            } => Self::PrintLiteral {
                text: (*text).to_owned(),
                output: *output,
                span: *span,
                i18n: i18n.as_ref().map(BytecodeI18nPart::from),
            },
            MirInstruction::EvaluateDiscard(_) => Self::EvaluateDiscard,
            MirInstruction::Unset(_) => Self::Unset,
            MirInstruction::RequestInclude { output, .. } => {
                Self::RequestInclude { output: *output }
            }
            MirInstruction::RequestGoto(_) => Self::RequestGoto,
            MirInstruction::InvokeMacro {
                call,
                captures,
                output,
            } => Self::InvokeMacro {
                name: call.name.to_owned(),
                arguments: BytecodeMacroArguments::from(&call.arguments),
                syntax_kind: call.syntax_kind,
                captures: captures.iter().map(|name| (*name).to_owned()).collect(),
                output: *output,
            },
            MirInstruction::ExitPassage => Self::ExitPassage,
            MirInstruction::Evaluate { destination, .. } => Self::Evaluate {
                destination: *destination,
            },
            MirInstruction::PrepareCollectionIteration {
                kind, destination, ..
            } => Self::PrepareCollectionIteration {
                kind: *kind,
                destination: *destination,
            },
            MirInstruction::PrepareRangeIteration {
                step, destination, ..
            } => Self::PrepareRangeIteration {
                has_step: step.is_some(),
                destination: *destination,
            },
            MirInstruction::NextIteration {
                iterator,
                exhausted,
                ..
            } => Self::NextIteration {
                iterator: *iterator,
                exhausted: *exhausted,
            },
            MirInstruction::JumpIfFalse { target, .. } => Self::JumpIfFalse { target: *target },
            MirInstruction::JumpIfNotStrictEqual { left, target, .. } => {
                Self::JumpIfNotStrictEqual {
                    left: *left,
                    target: *target,
                }
            }
            MirInstruction::Jump { target } => Self::Jump { target: *target },
            MirInstruction::Halt => Self::Halt,
        }
    }

    /// 返回输出指令（Text/PrintExpression/PrintLiteral）附着的翻译消息身份。
    pub fn i18n(&self) -> Option<&BytecodeI18nPart> {
        match self {
            Self::Text { i18n, .. }
            | Self::PrintExpression { i18n, .. }
            | Self::PrintLiteral { i18n, .. } => i18n.as_ref(),
            _ => None,
        }
    }

    /// 从操作数据重新推导 Opcode，用于验证序列化的冗余标签。
    pub(super) fn opcode(&self) -> Opcode {
        match self {
            Self::Text { .. } => Opcode::Text,
            Self::HardBreak { .. } => Opcode::HardBreak,
            Self::PrintExpression { .. } => Opcode::PrintExpression,
            Self::PrintLiteral { .. } => Opcode::PrintLiteral,
            Self::EvaluateDiscard => Opcode::EvaluateDiscard,
            Self::Unset => Opcode::Unset,
            Self::RequestInclude { .. } => Opcode::RequestInclude,
            Self::RequestGoto => Opcode::RequestGoto,
            Self::InvokeMacro { .. } => Opcode::InvokeMacro,
            Self::ExitPassage => Opcode::ExitPassage,
            Self::Evaluate { .. } => Opcode::Evaluate,
            Self::PrepareCollectionIteration { .. } => Opcode::PrepareCollectionIteration,
            Self::PrepareRangeIteration { .. } => Opcode::PrepareRangeIteration,
            Self::NextIteration { .. } => Opcode::NextIteration,
            Self::JumpIfFalse { .. } => Opcode::JumpIfFalse,
            Self::JumpIfNotStrictEqual { .. } => Opcode::JumpIfNotStrictEqual,
            Self::Jump { .. } => Opcode::Jump,
            Self::Halt => Opcode::Halt,
        }
    }

    /// 返回 VM 应按求值顺序取得的表达式数量。
    pub(super) fn expression_count(&self) -> usize {
        match self {
            Self::PrintExpression { .. }
            | Self::EvaluateDiscard
            | Self::Unset
            | Self::RequestInclude { .. }
            | Self::RequestGoto
            | Self::Evaluate { .. }
            | Self::PrepareCollectionIteration { .. }
            | Self::NextIteration { .. }
            | Self::JumpIfFalse { .. }
            | Self::JumpIfNotStrictEqual { .. } => 1,
            Self::PrepareRangeIteration { has_step, .. } => 2 + usize::from(*has_step),
            Self::InvokeMacro { arguments, .. } => {
                usize::from(matches!(arguments, BytecodeMacroArguments::Expression))
            }
            Self::Text { .. }
            | Self::HardBreak { .. }
            | Self::PrintLiteral { .. }
            | Self::ExitPassage
            | Self::Jump { .. }
            | Self::Halt => 0,
        }
    }

    /// 返回控制流目标；父模块据此验证目标仍位于当前指令体内。
    pub(super) fn jump_target(&self) -> Option<MirInstructionPointer> {
        match self {
            Self::NextIteration { exhausted, .. } => Some(*exhausted),
            Self::JumpIfFalse { target }
            | Self::JumpIfNotStrictEqual { target, .. }
            | Self::Jump { target } => Some(*target),
            _ => None,
        }
    }

    /// 返回当前操作访问的临时值槽。
    pub(super) fn value_slot(&self) -> Option<MirValueSlot> {
        match self {
            Self::Evaluate { destination } => Some(*destination),
            Self::JumpIfNotStrictEqual { left, .. } => Some(*left),
            _ => None,
        }
    }

    /// 返回当前操作访问的迭代器槽。
    pub(super) fn iterator_slot(&self) -> Option<MirIteratorSlot> {
        match self {
            Self::PrepareCollectionIteration { destination, .. }
            | Self::PrepareRangeIteration { destination, .. } => Some(*destination),
            Self::NextIteration { iterator, .. } => Some(*iterator),
            _ => None,
        }
    }
}

impl BytecodeMacroArguments {
    /// 校验轻量参数形状与完整拥有型 Macro 调用没有发生分歧。
    pub(super) fn matches_owned(&self, value: &OwnedHirMacroArguments) -> bool {
        match (self, value) {
            (Self::None, OwnedHirMacroArguments::None)
            | (Self::Expression, OwnedHirMacroArguments::Expression(_)) => true,
            (Self::Raw(left), OwnedHirMacroArguments::Raw(right)) => left == right,
            _ => false,
        }
    }
}

impl From<&MirI18nTextPart> for BytecodeI18nPart {
    fn from(value: &MirI18nTextPart) -> Self {
        Self {
            id: value.id().as_str().to_owned(),
            placeholder: value.placeholder().map(str::to_owned),
        }
    }
}

impl From<&HirMacroArguments<'_>> for BytecodeMacroArguments {
    fn from(value: &HirMacroArguments<'_>) -> Self {
        match value {
            HirMacroArguments::None => Self::None,
            HirMacroArguments::Raw(raw) => Self::Raw((*raw).to_owned()),
            HirMacroArguments::Expression(_) => Self::Expression,
        }
    }
}

/// 按 VM 求值顺序复制一条指令使用的全部表达式。
///
/// 范围循环固定为 start、end、可选 step；其他多字段指令也必须在这里显式确定
/// 顺序。解码时会用 `expression_count` 检查数量，执行时则依赖这个顺序取操作数。
pub(super) fn own_expressions(instruction: &MirInstruction<'_, '_>) -> Vec<OwnedExpression> {
    let expressions: Vec<&Expression<'_>> = match instruction {
        MirInstruction::PrintExpression { expression, .. }
        | MirInstruction::EvaluateDiscard(expression)
        | MirInstruction::Unset(expression)
        | MirInstruction::RequestGoto(expression) => vec![expression],
        MirInstruction::RequestInclude { target, .. }
        | MirInstruction::Evaluate {
            expression: target, ..
        }
        | MirInstruction::PrepareCollectionIteration {
            collection: target, ..
        }
        | MirInstruction::NextIteration { target, .. }
        | MirInstruction::JumpIfFalse {
            condition: target, ..
        }
        | MirInstruction::JumpIfNotStrictEqual { right: target, .. } => vec![target],
        MirInstruction::PrepareRangeIteration {
            start, end, step, ..
        } => {
            let mut values: Vec<&Expression<'_>> = vec![*start, *end];
            values.extend(step.iter().copied());
            values
        }
        MirInstruction::InvokeMacro { call, .. } => match &call.arguments {
            HirMacroArguments::Expression(expression) => vec![expression],
            HirMacroArguments::None | HirMacroArguments::Raw(_) => Vec::new(),
        },
        MirInstruction::Text { .. }
        | MirInstruction::HardBreak { .. }
        | MirInstruction::PrintLiteral { .. }
        | MirInstruction::ExitPassage
        | MirInstruction::Jump { .. }
        | MirInstruction::Halt => Vec::new(),
    };
    expressions.into_iter().map(OwnedExpression::from).collect()
}

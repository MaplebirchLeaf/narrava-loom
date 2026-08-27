//! HIR 正文的最小顺序执行与控制流边界。

mod executor;
mod logic;
mod native_macro;
mod widget;

mod continuation;
pub use continuation::*;

pub use executor::*;
pub use logic::*;
pub use native_macro::*;
pub use widget::*;

pub(crate) use logic::{collection_iteration_values, finite_range_number};

use crate::{hir::HirBodyNode, protocol::Surface};

/// 一个节点对当前正文剩余执行产生的控制信号。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyControl {
    Continue,
    BreakLoop,
    ContinueLoop,
    /// 结束最近的 Widget 调用；若当前不在 Widget 中，则结束 Passage。
    ExitScope,
    StopPassage,
}

/// 一次正文执行产生的控制信号与按源码顺序累积的有序输出。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyExecution {
    pub control: BodyControl,
    pub output: Surface,
}

/// 单次动态 Macro 的完成结果及其在内部展开的 Passage 数量。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeMacroExecution {
    pub execution: BodyExecution,
    pub includes_entered: usize,
}

impl Default for BodyExecution {
    fn default() -> Self {
        Self {
            control: BodyControl::Continue,
            output: Surface::default(),
        }
    }
}

/// 按源码顺序执行 HIR 节点，并在收到任何跳转信号后立即返回。
pub fn execute_hir_body<'source, Error>(
    body: &[HirBodyNode<'source>],
    mut execute_node: impl FnMut(&HirBodyNode<'source>) -> Result<BodyControl, Error>,
) -> Result<BodyControl, Error> {
    for node in body {
        let control: BodyControl = execute_node(node)?;
        if !matches!(control, BodyControl::Continue) {
            return Ok(control);
        }
    }
    Ok(BodyControl::Continue)
}

/// 与 [`execute_hir_body`] 相同，但每个节点回调返回执行结果，并按源码顺序累积输出；
/// 首个非 Continue 信号返回时携带已累积的输出，出错时丢弃本次累积。
pub fn execute_hir_body_with_output<'source, Error>(
    body: &[HirBodyNode<'source>],
    mut execute_node: impl FnMut(&HirBodyNode<'source>) -> Result<BodyExecution, Error>,
) -> Result<BodyExecution, Error> {
    let mut output: Surface = Surface::default();
    for node in body {
        let execution: BodyExecution = execute_node(node)?;
        output.append(execution.output);
        if !matches!(execution.control, BodyControl::Continue) {
            return Ok(BodyExecution {
                control: execution.control,
                output,
            });
        }
    }
    Ok(BodyExecution {
        control: BodyControl::Continue,
        output,
    })
}

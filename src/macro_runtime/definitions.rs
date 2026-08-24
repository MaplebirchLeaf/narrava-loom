//! Macro 定义契约，以及内置、Widget 与 scripts 定义的管理容器。

use std::collections::HashMap;

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    hir::{HirBodyKind, HirBodyNode, HirStory, HirWidget},
};

use super::MacroArgumentKind;

/// Macro 定义管理错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroDefinitionError {
    MissingDefinition,
}

impl MacroDefinitionError {
    /// 转换为可跨 Runtime 边界传递的稳定 Diagnostic。
    pub fn diagnostic(self, name: &str) -> Diagnostic {
        match self {
            Self::MissingDefinition => Diagnostic::new(
                "macro.missing_definition",
                DiagnosticSeverity::Error,
                &format!("Macro `{name}` 尚未注册"),
            ),
        }
    }
}

/// Macro 是否接收正文并要求对应闭合标签。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroBodyKind {
    /// 无正文，例如 `<<set $value = 1>>`。
    Inline,
    /// 有正文，例如 `<<if condition>>...<</if>>`。
    Container,
}

/// Macro Handler 返回立即结果还是异步结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroExecutionKind {
    Sync,
    Async,
}

/// Runtime 保存的完整 Macro 定义。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroDefinition<Handler> {
    pub body_kind: MacroBodyKind,
    pub argument_kind: MacroArgumentKind,
    pub execution_kind: MacroExecutionKind,
    pub handler: Handler,
}

impl<Handler> MacroDefinition<Handler> {
    /// 组合结构、参数契约、执行方式和具体 Handler。
    pub fn new(
        body_kind: MacroBodyKind,
        argument_kind: MacroArgumentKind,
        execution_kind: MacroExecutionKind,
        handler: Handler,
    ) -> Self {
        Self {
            body_kind,
            argument_kind,
            execution_kind,
            handler,
        }
    }
}

/// 同一 Macro Definitions 中可保存的原生 Handler 或 Widget 正文。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeMacroHandler<'hir, 'source, Native> {
    Native(Native),
    Widget(&'hir [HirBodyNode<'source>]),
}

/// 一次 Widget Passage 收集的可观察结果。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidgetRegistrationReport {
    pub registered: usize,
    pub replaced: usize,
}

/// 注册 Widget 声明；同名定义遵循 `Macro.add()` 的替换规则。
///
/// Widget 的声明正文是 Handler 本身，调用处仍是无正文的 Argument List Macro。
pub fn register_widget<'hir, 'source, Native>(
    definitions: &mut MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'hir, 'source, Native>>>,
    widget: &'hir HirWidget<'source>,
) -> Option<MacroDefinition<RuntimeMacroHandler<'hir, 'source, Native>>> {
    definitions.add(
        widget.name,
        MacroDefinition::new(
            MacroBodyKind::Inline,
            MacroArgumentKind::ArgumentList,
            MacroExecutionKind::Sync,
            RuntimeMacroHandler::Widget(widget.body.as_slice()),
        ),
    )
}

/// 从 `[widget]` Passage 收集顶层 Widget 定义。
///
/// 定义 Passage 的其他节点不会在注册阶段执行；它们也不会因此产生 State 副作用。
pub fn register_story_widgets<'hir, 'source, Native>(
    definitions: &mut MacroDefinitions<MacroDefinition<RuntimeMacroHandler<'hir, 'source, Native>>>,
    story: &'hir HirStory<'source>,
) -> WidgetRegistrationReport {
    let mut report: WidgetRegistrationReport = WidgetRegistrationReport::default();

    for passage in &story.passages {
        if !passage.has_tag("widget") {
            continue;
        }
        for node in &passage.body {
            let HirBodyKind::Widget(widget) = &node.kind else {
                continue;
            };
            let previous: Option<MacroDefinition<RuntimeMacroHandler<'hir, 'source, Native>>> =
                register_widget(definitions, widget);
            report.registered += 1;
            if previous.is_some() {
                report.replaced += 1;
            }
        }
    }

    report
}

/// 保存内置、Widget 或 scripts 提供的 Macro 定义。
///
/// 定义的具体 Handler 类型由执行边界决定；容器只管理名称与替换关系。
#[derive(Debug)]
pub struct MacroDefinitions<Definition> {
    definitions: HashMap<String, Definition>,
}

impl<Definition> MacroDefinitions<Definition> {
    /// 建立空定义容器。
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
        }
    }

    /// 新增定义；同名定义存在时替换并返回旧值。
    pub fn add(&mut self, name: &str, definition: Definition) -> Option<Definition> {
        self.definitions.insert(name.to_owned(), definition)
    }

    /// 只替换已经存在的定义，并返回旧值。
    pub fn update(
        &mut self,
        name: &str,
        definition: Definition,
    ) -> Result<Definition, MacroDefinitionError> {
        let current: &mut Definition = self
            .definitions
            .get_mut(name)
            .ok_or(MacroDefinitionError::MissingDefinition)?;
        Ok(std::mem::replace(current, definition))
    }

    /// 删除定义，并在存在时返回被删除的值。
    pub fn del(&mut self, name: &str) -> Option<Definition> {
        self.definitions.remove(name)
    }

    /// 获取定义的只读引用。
    pub fn get(&self, name: &str) -> Option<&Definition> {
        self.definitions.get(name)
    }

    /// 判断名称是否已经注册；名称比较区分大小写。
    pub fn has(&self, name: &str) -> bool {
        self.definitions.contains_key(name)
    }
}

impl<Definition> Default for MacroDefinitions<Definition> {
    fn default() -> Self {
        Self::new()
    }
}

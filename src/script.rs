//! 有序脚本源码与 Core 调回 Script 的函数边界。

use crate::{
    Source, SourceKind, SourceList,
    expression::{
        evaluator::ScriptCallError,
        value::{ScriptCallable, Value},
    },
    state::State,
};

/// State 在 VM 求值期间把 ScriptCallable 交还给 Binding 的瞬时路由。
///
/// 路由不进入 State checkpoint、Save 或 Value 图；真实函数对象仍归 Binding。
pub trait ScriptCallDispatcher {
    fn call(
        &self,
        callable: &ScriptCallable,
        arguments: Vec<Value>,
        state: &mut State,
    ) -> Result<Value, ScriptCallError>;
}

/// Binding 选择编译或执行路径所需的 ECMAScript 源码语言。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptLanguage {
    TypeScript,
    JavaScript,
}

/// 一份借用现有 Source 的脚本模块，不复制源码文本。
pub struct ScriptModule<'source> {
    source: &'source Source,
    language: ScriptLanguage,
}

impl ScriptModule<'_> {
    /// 模块的保存路径。
    pub fn path(&self) -> &str {
        self.source.path.as_str()
    }

    /// 模块源码语言。
    pub fn language(&self) -> ScriptLanguage {
        self.language
    }

    /// 模块源码文本。
    pub fn source(&self) -> &str {
        &self.source.content
    }
}

/// 按 Source 顺序交给同一个 Script Binding 的模块集合。
#[derive(Default)]
pub struct ScriptBundle<'source> {
    modules: Vec<ScriptModule<'source>>,
}

impl<'source> ScriptBundle<'source> {
    /// Twee 继续进入叙事编译器，只有 `.ts/.js` 进入脚本边界。
    pub fn from_sources(sources: &'source SourceList) -> Self {
        let modules: Vec<ScriptModule<'source>> = sources
            .items
            .iter()
            .filter_map(|source: &Source| {
                let language: ScriptLanguage = match source.kind {
                    SourceKind::TypeScript => ScriptLanguage::TypeScript,
                    SourceKind::JavaScript => ScriptLanguage::JavaScript,
                    SourceKind::Twee => return None,
                };
                Some(ScriptModule { source, language })
            })
            .collect();
        Self { modules }
    }

    /// 按 Source 顺序的模块集合。
    pub fn modules(&self) -> &[ScriptModule<'source>] {
        &self.modules
    }

    /// 是否不含任何脚本模块。
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

//! Source List 到有序 ECMAScript 模块集合的只读视图。
//!
//! Bundle 借用源码而不复制文本，只负责把 TypeScript 与 JavaScript 分流给同一个
//! Binding；Twee 必须继续走叙事编译管线，不能在这里被当作脚本执行。

use super::*;

/// 由 Tauri、Web 或嵌入式 ECMAScript 环境实现的启动加载边界。
pub trait ScriptBinding {
    type Error;

    fn load(
        &mut self,
        bundle: &ScriptBundle<'_>,
        context: &mut ScriptLoadContext<'_>,
    ) -> Result<(), Self::Error>;
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

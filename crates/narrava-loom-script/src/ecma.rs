//! Boa 上下文、脚本装载与 Oxc 转译；不负责 Host 命令编排。

use crate::{
    EcmaRuntime, ScriptError, bootstrap_source, json_to_value, reaction_adapter, resource_adapter,
    script_error, state_adapter, value_to_json,
};
use boa_engine::{Context, Source};
use narrava_loom_core::{
    SourceList,
    expression::{
        evaluator::ScriptCallError,
        value::{ScriptCallable, Value},
    },
    i18n::I18nCatalog,
    reaction::ReactionRegistry,
    resource::ResourceCatalog,
    script::ScriptBundle,
    state::State,
};
use oxc::{
    allocator::Allocator,
    codegen::Codegen,
    parser::Parser,
    semantic::SemanticBuilder,
    span::SourceType,
    transformer::{TransformOptions, Transformer, TypeScriptOptions},
};
use std::{cell::RefCell, path::Path, rc::Rc};

/// 官方 Runtime 单个 ECMAScript 循环允许执行的最大迭代次数。
const DEFAULT_SCRIPT_LOOP_LIMIT: u64 = 1_000_000;

/// 建立带明确执行上限的 Boa Context，避免作者脚本永久占住 Runtime Worker。
pub(super) fn runtime_context(loop_limit: u64) -> Context {
    let mut context = Context::default();
    context
        .runtime_limits_mut()
        .set_loop_iteration_limit(loop_limit);
    context
}

impl EcmaRuntime {
    /// 安装 State/Resource 桥接、注入启动脚本并执行全部转译后的模块。
    pub fn load(
        sources: &SourceList,
        resources: &ResourceCatalog,
        i18n: &I18nCatalog,
        default_locale: &str,
        state: &mut State,
    ) -> Result<Self, ScriptError> {
        let mut context = runtime_context(DEFAULT_SCRIPT_LOOP_LIMIT);
        state_adapter::install(&mut context)
            .map_err(|error| script_error("script.state_bridge", error))?;
        crate::world_adapter::install(&mut context)
            .map_err(|error| script_error("script.world_bridge", error))?;
        resource_adapter::install(&mut context, resources.clone())
            .map_err(|error| script_error("script.resource_bridge", error))?;
        let reactions: Rc<RefCell<ReactionRegistry<ScriptCallable>>> =
            reaction_adapter::install(&mut context)
                .map_err(|error| script_error("script.reaction_bridge", error))?;
        let configuration = serde_json::json!({
            "defaultLocale": default_locale,
            "locale": default_locale,
            "i18nExport": serde_json::to_string_pretty(&i18n.template(default_locale))
                .map_err(|error| ScriptError::new("script.i18n_export", error.to_string()))?,
        });
        let configure = format!("__narrava.configure({configuration})");
        let modules = ScriptBundle::from_sources(sources)
            .modules()
            .iter()
            .map(|module| transpile(module.path(), module.source()))
            .collect::<Result<Vec<_>, _>>()?;
        state_adapter::with_state(&mut context, state, |context| {
            let bootstrap: &str = bootstrap_source();
            context
                .eval(Source::from_bytes(&bootstrap))
                .map_err(|error| script_error("script.bootstrap", error))?;
            context
                .eval(Source::from_bytes(configure.as_bytes()))
                .map_err(|error| script_error("script.configure", error))?;
            for javascript in &modules {
                context
                    .eval(Source::from_bytes(javascript))
                    .map_err(|error| script_error("script.execute", error))?;
                context
                    .run_jobs()
                    .map_err(|error| script_error("script.jobs", error))?;
            }
            Ok::<(), ScriptError>(())
        })?;
        // 允许地点跨脚本前向引用，全部装载后才校验父关系并封闭注册。
        state
            .world()
            .validate()
            .map_err(|error| ScriptError::new(error.code(), error.to_string()))?;
        crate::world_adapter::seal(&context);
        Ok(Self { context, reactions })
    }
}

impl EcmaRuntime {
    /// 在当前 State 上调用脚本函数并读回 Core 值。
    pub fn call(
        &mut self,
        callable: &ScriptCallable,
        arguments: Vec<Value>,
        state: &mut State,
    ) -> Result<Value, ScriptCallError> {
        let arguments = arguments
            .iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ScriptCallError::Failed)?;
        let expression = format!(
            "JSON.stringify(__narrava.call({}, {}))",
            callable.id(),
            serde_json::to_string(&arguments).expect("JSON Value 必须可序列化")
        );
        state_adapter::with_state(&mut self.context, state, |context| {
            let result = context
                .eval(Source::from_bytes(expression.as_bytes()))
                .map_err(|_| ScriptCallError::Failed)?;
            context.run_jobs().map_err(|_| ScriptCallError::Failed)?;
            if result.is_undefined() {
                return Ok(Value::Undefined);
            }
            let json = result
                .to_string(context)
                .map_err(|_| ScriptCallError::Failed)?
                .to_std_string_escaped();
            let value: serde_json::Value =
                serde_json::from_str(&json).map_err(|_| ScriptCallError::Failed)?;
            json_to_value(&value).map_err(|_| ScriptCallError::Failed)
        })
    }
}

/// 按扩展名转译脚本：`.js` 原样返回，`.ts` 走 oxc 解析/语义/转换/代码生成。
pub fn transpile(path: &str, source: &str) -> Result<String, ScriptError> {
    if path.ends_with(".js") {
        return Ok(source.to_owned());
    }
    let source_type = SourceType::from_path(path)
        .map_err(|error| ScriptError::new("script.language", error.to_string()))?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(ScriptError::new(
            "script.parse",
            parsed.diagnostics[0].to_string(),
        ));
    }
    let mut program = parsed.program;
    let semantic = SemanticBuilder::new().build(&program);
    if !semantic.diagnostics.is_empty() {
        return Err(ScriptError::new(
            "script.semantic",
            semantic.diagnostics[0].to_string(),
        ));
    }
    let options = TransformOptions {
        typescript: TypeScriptOptions::default(),
        ..TransformOptions::default()
    };
    let transformed = Transformer::new(&allocator, Path::new(path), &options)
        .build_with_scoping(semantic.semantic.into_scoping(), &mut program);
    if !transformed.diagnostics.is_empty() {
        return Err(ScriptError::new(
            "script.transform",
            transformed.diagnostics[0].to_string(),
        ));
    }
    Ok(Codegen::new().build(&program).code)
}

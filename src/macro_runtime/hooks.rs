//! Macro before／after 生命周期订阅及编译期固有语法保护。

use std::collections::HashMap;

use crate::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    expression::value::Value,
    presentation::PresentationOutput,
};

/// Runtime 调用 Macro 生命周期时依赖的最小边界。
///
/// 具体的 Macro 控制器负责查找订阅并执行 Rust 或 scripts Callback；Runtime
/// 只提供当前调用的参数与隔离输出，不持有平台函数或脚本对象。
pub trait MacroLifecycleCallbacks {
    /// Handler 执行前可修改本次调用的 `@args`。
    fn before(&mut self, name: &str, arguments: &mut [Value]) -> Result<(), Diagnostic>;

    /// Handler 完成后可转换本次调用产生的独立语义输出。
    fn after(
        &mut self,
        name: &str,
        arguments: &[Value],
        output: PresentationOutput,
    ) -> Result<PresentationOutput, Diagnostic>;
}

/// 将有序订阅转换为 Runtime 可调用的 Macro 生命周期边界。
///
/// 两个调用适配器决定如何执行 Hook。Hook 可以是 Rust Handler、scripts
/// Callback 身份或 Binding 自己的句柄，Core 不需要知道其平台类型。
pub struct MacroLifecycleController<'subscriptions, Hook, Before, After> {
    subscriptions: &'subscriptions MacroLifecycleSubscriptions<Hook>,
    invoke_before: Before,
    invoke_after: After,
}

impl<'subscriptions, Hook, Before, After>
    MacroLifecycleController<'subscriptions, Hook, Before, After>
{
    /// 组合订阅集合与 before／after 两个调用适配器。
    pub fn new(
        subscriptions: &'subscriptions MacroLifecycleSubscriptions<Hook>,
        invoke_before: Before,
        invoke_after: After,
    ) -> Self {
        Self {
            subscriptions,
            invoke_before,
            invoke_after,
        }
    }
}

impl<Hook, Before, After> MacroLifecycleCallbacks
    for MacroLifecycleController<'_, Hook, Before, After>
where
    Before: FnMut(&Hook, &str, &mut [Value]) -> Result<(), Diagnostic>,
    After:
        FnMut(&Hook, &str, &[Value], PresentationOutput) -> Result<PresentationOutput, Diagnostic>,
{
    fn before(&mut self, name: &str, arguments: &mut [Value]) -> Result<(), Diagnostic> {
        for hook in self.subscriptions.before_hooks(name) {
            (self.invoke_before)(hook, name, arguments)?;
        }
        Ok(())
    }

    fn after(
        &mut self,
        name: &str,
        arguments: &[Value],
        mut output: PresentationOutput,
    ) -> Result<PresentationOutput, Diagnostic> {
        for hook in self.subscriptions.after_hooks(name) {
            output = (self.invoke_after)(hook, name, arguments, output)?;
        }
        Ok(output)
    }
}

/// 一次 Macro 生命周期订阅的稳定进程内身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MacroLifecycleSubscriptionId(u64);

impl MacroLifecycleSubscriptionId {
    /// Binding 可把进程内订阅身份编码为普通整数。
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Macro 生命周期订阅不能建立的稳定原因。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroLifecycleSubscriptionError {
    CompilerOwnedMacro(String),
    IdExhausted,
}

impl MacroLifecycleSubscriptionError {
    /// 转换为 Macro API、Logger 与调试器共用的稳定 Diagnostic。
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::CompilerOwnedMacro(name) => Diagnostic::new(
                "macro.lifecycle.compiler_owned",
                DiagnosticSeverity::Error,
                &format!("Macro `{name}` 是编译器固有语法，不能注册生命周期 Hook"),
            ),
            Self::IdExhausted => Diagnostic::new(
                "macro.lifecycle.id_exhausted",
                DiagnosticSeverity::Error,
                "Macro 生命周期订阅编号已耗尽",
            ),
        }
    }
}

#[derive(Debug)]
struct MacroLifecycleSubscription<Hook> {
    id: MacroLifecycleSubscriptionId,
    hook: Hook,
}

/// 按 MacroName 和阶段保存有序 Hook；不拥有 Macro Definition 或 Runtime 状态。
#[derive(Debug)]
pub struct MacroLifecycleSubscriptions<Hook> {
    next_id: u64,
    before: HashMap<String, Vec<MacroLifecycleSubscription<Hook>>>,
    after: HashMap<String, Vec<MacroLifecycleSubscription<Hook>>>,
}

impl<Hook> MacroLifecycleSubscriptions<Hook> {
    /// 建立没有任何订阅的 Macro 生命周期集合。
    pub fn new() -> Self {
        Self {
            next_id: 1,
            before: HashMap::new(),
            after: HashMap::new(),
        }
    }

    /// 按注册顺序追加 before Hook。
    pub fn before(
        &mut self,
        name: &str,
        hook: Hook,
    ) -> Result<MacroLifecycleSubscriptionId, MacroLifecycleSubscriptionError> {
        Self::register(&mut self.next_id, &mut self.before, name, hook)
    }

    /// 按注册顺序追加 after Hook。
    pub fn after(
        &mut self,
        name: &str,
        hook: Hook,
    ) -> Result<MacroLifecycleSubscriptionId, MacroLifecycleSubscriptionError> {
        Self::register(&mut self.next_id, &mut self.after, name, hook)
    }

    /// 取消一条订阅并交还 Hook；未知身份不改变集合。
    pub fn off(&mut self, id: MacroLifecycleSubscriptionId) -> Option<Hook> {
        Self::remove_from(&mut self.before, id).or_else(|| Self::remove_from(&mut self.after, id))
    }

    /// 按登记顺序读取指定 Macro 的 before Hook。
    pub fn before_hooks<'hooks>(&'hooks self, name: &str) -> impl Iterator<Item = &'hooks Hook> {
        self.before
            .get(name)
            .into_iter()
            .flat_map(|hooks: &Vec<MacroLifecycleSubscription<Hook>>| hooks.iter())
            .map(|subscription: &MacroLifecycleSubscription<Hook>| &subscription.hook)
    }

    /// 按登记顺序读取指定 Macro 的 after Hook。
    pub fn after_hooks<'hooks>(&'hooks self, name: &str) -> impl Iterator<Item = &'hooks Hook> {
        self.after
            .get(name)
            .into_iter()
            .flat_map(|hooks: &Vec<MacroLifecycleSubscription<Hook>>| hooks.iter())
            .map(|subscription: &MacroLifecycleSubscription<Hook>| &subscription.hook)
    }

    /// 校验编译器固有名称并分配递增订阅编号后登记。
    fn register(
        next_id: &mut u64,
        subscriptions: &mut HashMap<String, Vec<MacroLifecycleSubscription<Hook>>>,
        name: &str,
        hook: Hook,
    ) -> Result<MacroLifecycleSubscriptionId, MacroLifecycleSubscriptionError> {
        if compiler_owns_macro(name) {
            return Err(MacroLifecycleSubscriptionError::CompilerOwnedMacro(
                name.to_owned(),
            ));
        }
        let id: MacroLifecycleSubscriptionId = MacroLifecycleSubscriptionId(*next_id);
        *next_id = next_id
            .checked_add(1)
            .ok_or(MacroLifecycleSubscriptionError::IdExhausted)?;
        subscriptions
            .entry(name.to_owned())
            .or_default()
            .push(MacroLifecycleSubscription { id, hook });
        Ok(id)
    }

    /// 在指定阶段集合中按编号查找并移除一条订阅。
    fn remove_from(
        subscriptions: &mut HashMap<String, Vec<MacroLifecycleSubscription<Hook>>>,
        id: MacroLifecycleSubscriptionId,
    ) -> Option<Hook> {
        for hooks in subscriptions.values_mut() {
            if let Some(index) = hooks
                .iter()
                .position(|subscription: &MacroLifecycleSubscription<Hook>| subscription.id == id)
            {
                return Some(hooks.remove(index).hook);
            }
        }
        None
    }
}

impl<Hook> Default for MacroLifecycleSubscriptions<Hook> {
    fn default() -> Self {
        Self::new()
    }
}

/// 编译器固有语法 Macro 不允许注册生命周期 Hook。
fn compiler_owns_macro(name: &str) -> bool {
    matches!(
        name,
        "if" | "elseif"
            | "else"
            | "switch"
            | "case"
            | "default"
            | "for"
            | "while"
            | "break"
            | "continue"
            | "set"
            | "unset"
            | "run"
            | "include"
            | "goto"
            | "print"
            | "silently"
            | "return"
            | "capture"
            | "exit"
            | "widget"
    )
}

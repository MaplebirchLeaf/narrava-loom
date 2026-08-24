//! Macro 调用帧、@ 局部变量与 Expression 上下文适配。

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::expression::{VariableScope, evaluator::EvaluationContext, value::Value};

/// `@` 局部变量操作错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroLocalError {
    NoActiveScope,
    ReservedName,
}

impl MacroLocalError {
    /// 转换为不附加虚构源码位置的稳定 Diagnostic。
    pub fn diagnostic(self) -> Diagnostic {
        match self {
            Self::NoActiveScope => Diagnostic::new(
                "macro.no_active_local_scope",
                DiagnosticSeverity::Error,
                "当前没有活动的 Macro Local Scope",
            ),
            Self::ReservedName => Diagnostic::new(
                "macro.reserved_local_name",
                DiagnosticSeverity::Error,
                "`@args` 由当前 Macro 调用帧提供，不允许修改",
            ),
        }
    }
}

/// 一次 Macro 调用拥有独立局部绑定，同时保留完整位置实参。
#[derive(Clone, Debug, PartialEq)]
struct MacroCallFrame<Value> {
    locals: HashMap<String, Value>,
    arguments: Vec<Value>,
}

impl<Value> MacroCallFrame<Value> {
    fn new(arguments: Vec<Value>) -> Self {
        Self {
            locals: HashMap::new(),
            arguments,
        }
    }
}

/// 按 Macro 调用层级保存互相隔离的 `@` 局部变量。
#[derive(Clone, Debug, PartialEq)]
pub struct MacroLocalScopes<Value> {
    scopes: Vec<MacroCallFrame<Value>>,
}

/// 延迟正文显式保留的局部绑定。
///
/// 它不保存原调用栈或 `@args`，因此未列入 `capture` 的局部状态不会跨越交互边界。
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedMacroLocals<Value> {
    locals: HashMap<String, Value>,
}

impl<Value> MacroLocalScopes<Value> {
    /// 建立尚未进入任何 Macro 调用的空作用域栈。
    pub fn new() -> Self {
        Self { scopes: Vec::new() }
    }

    /// 为一次 Macro 调用建立新的当前作用域。
    pub fn enter(&mut self) {
        self.scopes.push(MacroCallFrame::new(Vec::new()));
    }

    /// 建立调用帧；实参稍后通过当前上下文的 `@args` 读取。
    pub fn enter_call(&mut self, arguments: Vec<Value>) {
        self.scopes.push(MacroCallFrame::new(arguments));
    }

    /// 结束当前调用并删除它的全部局部变量。
    pub fn leave(&mut self) -> bool {
        self.scopes.pop().is_some()
    }

    /// 把整条执行链的局部帧移出当前执行位置。
    pub fn suspend(&mut self) -> Result<SuspendedMacroScopes<Value>, MacroLocalError> {
        if self.scopes.is_empty() {
            return Err(MacroLocalError::NoActiveScope);
        }
        Ok(SuspendedMacroScopes {
            scopes: std::mem::take(self),
        })
    }

    /// 从当前层向外查找，但不改变任何绑定。
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope: &MacroCallFrame<Value>| scope.locals.get(name))
    }

    /// 读取当前调用的完整 `@args` 位置实参。
    pub fn args(&self) -> Option<&[Value]> {
        self.scopes
            .last()
            .map(|scope: &MacroCallFrame<Value>| scope.arguments.as_slice())
    }

    /// 生命周期 before Hook 可修改当前调用帧的 `@args`。
    pub fn args_mut(&mut self) -> Option<&mut [Value]> {
        self.scopes
            .last_mut()
            .map(|scope: &mut MacroCallFrame<Value>| scope.arguments.as_mut_slice())
    }

    /// 只在当前调用层新增或替换绑定。
    pub fn set(&mut self, name: &str, value: Value) -> Result<Option<Value>, MacroLocalError> {
        if name == "args" {
            return Err(MacroLocalError::ReservedName);
        }
        let scope: &mut MacroCallFrame<Value> = self
            .scopes
            .last_mut()
            .ok_or(MacroLocalError::NoActiveScope)?;
        Ok(scope.locals.insert(name.to_owned(), value))
    }

    /// 只删除当前调用层中的同名绑定。
    pub fn del(&mut self, name: &str) -> Result<Option<Value>, MacroLocalError> {
        if name == "args" {
            return Err(MacroLocalError::ReservedName);
        }
        let scope: &mut MacroCallFrame<Value> = self
            .scopes
            .last_mut()
            .ok_or(MacroLocalError::NoActiveScope)?;
        Ok(scope.locals.remove(name))
    }
}

impl<Value: Clone> MacroLocalScopes<Value> {
    /// 从当前可见局部变量中复制指定绑定；缺失名称继续保持未定义状态。
    pub fn capture(&self, names: &[&str]) -> CapturedMacroLocals<Value> {
        let locals: HashMap<String, Value> = names
            .iter()
            .filter_map(|name: &&str| {
                self.get(name)
                    .cloned()
                    .map(|value: Value| ((*name).to_owned(), value))
            })
            .collect();
        CapturedMacroLocals { locals }
    }
}

impl<Value> CapturedMacroLocals<Value> {
    pub fn empty() -> Self {
        Self {
            locals: HashMap::new(),
        }
    }

    /// 为一次延迟正文建立隔离调用帧；原调用的 `@args` 不会被隐式恢复。
    pub fn into_scopes(self) -> MacroLocalScopes<Value> {
        MacroLocalScopes {
            scopes: vec![MacroCallFrame {
                locals: self.locals,
                arguments: Vec::new(),
            }],
        }
    }
}

impl<Value> Default for CapturedMacroLocals<Value> {
    fn default() -> Self {
        Self::empty()
    }
}

/// 异步 Pending 期间独占拥有的 Macro 局部作用域链。
#[derive(Debug, PartialEq)]
pub struct SuspendedMacroScopes<Value> {
    scopes: MacroLocalScopes<Value>,
}

impl<Value> SuspendedMacroScopes<Value> {
    /// 取回完整作用域链，交给恢复后的独立执行位置。
    pub fn into_scopes(self) -> MacroLocalScopes<Value> {
        self.scopes
    }
}

/// 把 State 等基础上下文与 Macro 调用帧组合为只读 Expression 上下文。
pub struct MacroEvaluationContext<'a> {
    base: &'a dyn EvaluationContext,
    locals: &'a MacroLocalScopes<Value>,
    args: Option<Value>,
}

impl<'a> MacroEvaluationContext<'a> {
    pub fn new(base: &'a dyn EvaluationContext, locals: &'a MacroLocalScopes<Value>) -> Self {
        let args: Option<Value> = locals
            .args()
            .map(|values: &[Value]| Value::array(values.to_vec()));
        Self { base, locals, args }
    }
}

impl EvaluationContext for MacroEvaluationContext<'_> {
    fn global(&self, name: &str) -> Option<&Value> {
        self.base.global(name)
    }

    fn setup(&self) -> Option<&Value> {
        self.base.setup()
    }

    fn variable(&self, scope: VariableScope, name: &str) -> Option<&Value> {
        match (scope, name) {
            (VariableScope::Local, "args") => self.args.as_ref(),
            (VariableScope::Local, _) => self.locals.get(name),
            _ => self.base.variable(scope, name),
        }
    }
}

impl<Value> Default for MacroLocalScopes<Value> {
    fn default() -> Self {
        Self::new()
    }
}

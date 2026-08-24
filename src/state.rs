//! State 的游戏变量命名空间。

use std::{collections::BTreeMap, rc::Rc};

use crate::expression::{
    VariableScope,
    evaluator::{ContextWriteError, EvaluationContext, WritableEvaluationContext},
    value::Value,
};
use crate::script::ScriptCallDispatcher;

/// Twee 与 scripts 共用的受控游戏变量存储。
pub struct State {
    global: BTreeMap<String, Value>,
    setup: Value,
    variables: BTreeMap<String, Value>,
    temporary: BTreeMap<String, Value>,
    script_dispatcher: Option<Rc<dyn ScriptCallDispatcher>>,
}

/// 一次与活动 State 引用图隔离的 `$variables` 快照。
pub struct StateSnapshot {
    variables: BTreeMap<String, Value>,
}

/// 一次覆盖全部 State 命名空间的短期运行事务检查点。
pub struct StateCheckpoint {
    global: BTreeMap<String, Value>,
    setup: Value,
    variables: BTreeMap<String, Value>,
    temporary: BTreeMap<String, Value>,
}

/// 一次新游戏重置实际移除的游戏状态数量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateReset {
    pub variables_removed: usize,
    pub temporary_removed: usize,
}

/// 一次批量导入对 State.global 的修改结果。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlobalImportReport {
    pub inserted: usize,
    pub replaced: usize,
}

impl StateSnapshot {
    pub(crate) fn from_variables(variables: BTreeMap<String, Value>) -> Self {
        Self { variables }
    }

    pub(crate) fn variables(&self) -> &BTreeMap<String, Value> {
        &self.variables
    }

    pub fn variables_get(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    pub fn variables_has(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    pub fn variables_len(&self) -> usize {
        self.variables.len()
    }
}

impl State {
    /// 建立空 State；`setup` 从一开始就是可读取的空对象。
    pub fn new() -> Self {
        Self {
            global: BTreeMap::new(),
            setup: Value::object(Vec::new()),
            variables: BTreeMap::new(),
            temporary: BTreeMap::new(),
            script_dispatcher: None,
        }
    }

    /// 为 Host 的只读辅助区域建立隔离运行视图。
    ///
    /// Value 图与活动 State 脱离，但复用同一个脚本调用路由；视图中的写入会随视图丢弃。
    pub fn fork_view(&self) -> Self {
        let mut view: Self = Self::new();
        view.restore_checkpoint(self.checkpoint());
        view.script_dispatcher = self.script_dispatcher.clone();
        view
    }

    /// 附着当前 Binding 的瞬时函数路由；不会进入任何持久化状态。
    pub fn attach_script_dispatcher(&mut self, dispatcher: Rc<dyn ScriptCallDispatcher>) {
        self.script_dispatcher = Some(dispatcher);
    }

    pub fn detach_script_dispatcher(&mut self) {
        self.script_dispatcher = None;
    }

    /// 查询 scripts 与 Twee 共用的普通全局名称。
    pub fn global_get(&self, name: &str) -> Option<&Value> {
        self.global.get(name)
    }

    pub fn global_has(&self, name: &str) -> bool {
        self.global.contains_key(name)
    }

    pub fn global_entries(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.global
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }

    /// 写入全局名称，并返回被替换的旧值。
    pub fn global_set(&mut self, name: &str, value: Value) -> Option<Value> {
        self.global.insert(name.to_owned(), value)
    }

    /// 批量导入 scripts 暴露的普通名称；同名项按输入顺序覆盖。
    pub fn global_extend(
        &mut self,
        values: impl IntoIterator<Item = (String, Value)>,
    ) -> GlobalImportReport {
        let mut report: GlobalImportReport = GlobalImportReport::default();
        for (name, value) in values {
            if self.global_set(name.as_str(), value).is_some() {
                report.replaced += 1;
            } else {
                report.inserted += 1;
            }
        }
        report
    }

    pub fn global_del(&mut self, name: &str) -> Option<Value> {
        self.global.remove(name)
    }

    /// 读取 `$name` 所属的持久游戏变量表。
    pub fn variables_get(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    pub fn variables_has(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    pub fn variables_entries(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.variables
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }

    pub fn variables_set(&mut self, name: &str, value: Value) -> Option<Value> {
        self.variables.insert(name.to_owned(), value)
    }

    pub fn variables_del(&mut self, name: &str) -> Option<Value> {
        self.variables.remove(name)
    }

    /// 读取 `_name` 所属的临时游戏变量表。
    pub fn temporary_get(&self, name: &str) -> Option<&Value> {
        self.temporary.get(name)
    }

    pub fn temporary_has(&self, name: &str) -> bool {
        self.temporary.contains_key(name)
    }

    pub fn temporary_entries(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.temporary
            .iter()
            .map(|(name, value)| (name.as_str(), value))
    }

    pub fn temporary_set(&mut self, name: &str, value: Value) -> Option<Value> {
        self.temporary.insert(name.to_owned(), value)
    }

    pub fn temporary_del(&mut self, name: &str) -> Option<Value> {
        self.temporary.remove(name)
    }

    /// 清空 `_` 变量并返回移除数量；调用时机由上层生命周期显式决定。
    pub fn temporary_clear(&mut self) -> usize {
        let removed: usize = self.temporary.len();
        self.temporary.clear();
        removed
    }

    /// `setup` 根始终存在，因此直接返回借用而不是 Option。
    pub fn setup_get(&self) -> &Value {
        &self.setup
    }

    /// 替换 setup 根，并返回旧值。
    pub fn setup_set(&mut self, value: Value) -> Value {
        std::mem::replace(&mut self.setup, value)
    }

    /// 只捕获可保存的 `$variables`，并与活动 Value 图完全脱离。
    pub fn snapshot(&self) -> StateSnapshot {
        let names: Vec<String> = self.variables.keys().cloned().collect();
        let values: Vec<Value> = self.variables.values().cloned().collect();
        let detached: Vec<Value> = Value::detached_clone_many(&values);
        let variables: BTreeMap<String, Value> = names.into_iter().zip(detached).collect();
        StateSnapshot { variables }
    }

    /// 恢复 `$variables` 并丢弃不属于快照的 `_temporary`。
    ///
    /// `global` 与 `setup` 由当前启动环境管理，因此保持不变。
    pub fn restore(&mut self, snapshot: StateSnapshot) {
        self.variables = snapshot.variables;
        let _removed: usize = self.temporary_clear();
    }

    /// 捕获全部 State，并在一次复制中保留跨命名空间的共享引用。
    pub fn checkpoint(&self) -> StateCheckpoint {
        let global_names: Vec<String> = self.global.keys().cloned().collect();
        let variable_names: Vec<String> = self.variables.keys().cloned().collect();
        let temporary_names: Vec<String> = self.temporary.keys().cloned().collect();
        let global_count: usize = global_names.len();
        let setup_index: usize = global_count;
        let variable_start: usize = setup_index + 1;
        let temporary_start: usize = variable_start + variable_names.len();
        let mut values: Vec<Value> = Vec::with_capacity(
            global_names.len() + variable_names.len() + temporary_names.len() + 1,
        );
        values.extend(self.global.values().cloned());
        values.push(self.setup.clone());
        values.extend(self.variables.values().cloned());
        values.extend(self.temporary.values().cloned());

        let detached: Vec<Value> = Value::detached_clone_many(&values);
        let global: BTreeMap<String, Value> = global_names
            .into_iter()
            .zip(detached[..global_count].iter().cloned())
            .collect();
        let setup: Value = detached[setup_index].clone();
        let variables: BTreeMap<String, Value> = variable_names
            .into_iter()
            .zip(detached[variable_start..temporary_start].iter().cloned())
            .collect();
        let temporary: BTreeMap<String, Value> = temporary_names
            .into_iter()
            .zip(detached[temporary_start..].iter().cloned())
            .collect();

        StateCheckpoint {
            global,
            setup,
            variables,
            temporary,
        }
    }

    /// 一次性恢复完整 State；该入口只用于短期运行事务。
    pub fn restore_checkpoint(&mut self, checkpoint: StateCheckpoint) {
        self.global = checkpoint.global;
        self.setup = checkpoint.setup;
        self.variables = checkpoint.variables;
        self.temporary = checkpoint.temporary;
    }

    /// 开始新游戏时清空 `$` 与 `_`，保留启动环境提供的 global/setup。
    pub fn reset_game(&mut self) -> StateReset {
        let variables_removed: usize = self.variables.len();
        self.variables.clear();
        let temporary_removed: usize = self.temporary_clear();
        StateReset {
            variables_removed,
            temporary_removed,
        }
    }
}

impl EvaluationContext for State {
    fn global(&self, name: &str) -> Option<&Value> {
        self.global_get(name)
    }

    fn setup(&self) -> Option<&Value> {
        Some(self.setup_get())
    }

    fn variable(&self, scope: VariableScope, name: &str) -> Option<&Value> {
        match scope {
            VariableScope::Variables => self.variables_get(name),
            VariableScope::Temporary => self.temporary_get(name),
            VariableScope::Local => None,
        }
    }
}

impl WritableEvaluationContext for State {
    fn set_global(&mut self, name: &str, value: Value) -> Result<(), ContextWriteError> {
        let _previous: Option<Value> = self.global_set(name, value);
        Ok(())
    }

    fn set_variable(
        &mut self,
        scope: VariableScope,
        name: &str,
        value: Value,
    ) -> Result<(), ContextWriteError> {
        let previous: Option<Value> = match scope {
            VariableScope::Variables => self.variables_set(name, value),
            VariableScope::Temporary => self.temporary_set(name, value),
            VariableScope::Local => return Err(ContextWriteError::Rejected),
        };
        let _previous: Option<Value> = previous;
        Ok(())
    }

    fn set_setup(&mut self, value: Value) -> Result<(), ContextWriteError> {
        let _previous: Value = self.setup_set(value);
        Ok(())
    }

    fn del_global(&mut self, name: &str) -> Result<Option<Value>, ContextWriteError> {
        Ok(self.global_del(name))
    }

    fn del_variable(
        &mut self,
        scope: VariableScope,
        name: &str,
    ) -> Result<Option<Value>, ContextWriteError> {
        match scope {
            VariableScope::Variables => Ok(self.variables_del(name)),
            VariableScope::Temporary => Ok(self.temporary_del(name)),
            VariableScope::Local => Err(ContextWriteError::Rejected),
        }
    }

    fn authorize_reference_write(&mut self) -> Result<(), ContextWriteError> {
        Ok(())
    }

    fn call_script(
        &mut self,
        callable: &crate::expression::value::ScriptCallable,
        arguments: Vec<Value>,
    ) -> Result<Value, crate::expression::evaluator::ScriptCallError> {
        let dispatcher = self
            .script_dispatcher
            .as_ref()
            .cloned()
            .ok_or(crate::expression::evaluator::ScriptCallError::Unavailable)?;
        dispatcher.call(callable, arguments, self)
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

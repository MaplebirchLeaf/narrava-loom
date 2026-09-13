//! 游戏变量、地点位置与事务快照。

use std::{
    collections::{BTreeMap, HashMap},
    rc::Rc,
};

use crate::engine::{Engine, EngineSnapshot};
use crate::expression::{
    VariableScope,
    evaluator::{ContextWriteError, EvaluationContext, WritableEvaluationContext},
    value::Value,
};
use crate::location::{Location, LocationState};
use crate::script::ScriptCallDispatcher;

/// Twee 与 scripts 共用的受控游戏变量存储。
pub struct State {
    global: HashMap<String, Value>,
    setup: Value,
    variables: HashMap<String, Value>,
    temporary: HashMap<String, Value>,
    location: Rc<Location>,
    location_state: LocationState,
    engine: Rc<Engine>,
    script_dispatcher: Option<Rc<dyn ScriptCallDispatcher>>,
}

/// 与活动变量引用图隔离的持久状态快照；地点定义由启动环境持有。
pub struct StateSnapshot {
    variables: BTreeMap<String, Value>,
    location_state: LocationState,
    engine: EngineSnapshot,
}

/// 覆盖变量、地点及 Engine 正文/重绘进度的短期事务检查点。
pub struct StateCheckpoint {
    engine: EngineSnapshot,
    replay: Option<EngineSnapshot>,
    global: HashMap<String, Value>,
    setup: Value,
    variables: HashMap<String, Value>,
    temporary: HashMap<String, Value>,
    location: Rc<Location>,
    location_state: LocationState,
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
    /// 历史入页时的 Engine 执行进度。
    pub fn engine_snapshot(&self) -> EngineSnapshot {
        self.engine
    }

    /// 从已解码并校验的持久数据构造快照。
    pub(crate) fn from_parts(
        variables: BTreeMap<String, Value>,
        location_state: LocationState,
        engine: EngineSnapshot,
    ) -> Self {
        Self {
            variables,
            location_state,
            engine,
        }
    }

    /// 历史位置与环境，不包含地点定义。
    pub fn location_state(&self) -> &LocationState {
        &self.location_state
    }

    /// 存档编码器借用历史快照中的持久变量。
    pub(crate) fn persistent_variables(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.variables
            .iter()
            .map(|(name, value): (&String, &Value)| (name.as_str(), value))
    }

    /// 读取快照中的 `$name` 持久变量值。
    pub fn variables_get(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    /// 查询快照中的 `$name` 持久变量是否存在。
    pub fn variables_has(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    /// 快照中的持久变量条目数。
    pub fn variables_len(&self) -> usize {
        self.variables.len()
    }

    /// 为历史重放建立与快照和值别名图隔离的 `$variables` 副本。
    fn detached_variables(&self) -> BTreeMap<String, Value> {
        let names: Vec<String> = self.variables.keys().cloned().collect();
        let values: Vec<Value> = self.variables.values().cloned().collect();
        names
            .into_iter()
            .zip(Value::detached_clone_many(&values))
            .collect()
    }

    /// 读取不带 `$` 的持久变量属性路径；不存在的段统一返回 `Undefined`。
    pub fn variables_path(&self, path: &str) -> Value {
        let mut segments = path.split('.');
        let Some(root) = segments.next() else {
            return Value::Undefined;
        };
        let mut value: Value = self
            .variables
            .get(root)
            .cloned()
            .unwrap_or(Value::Undefined);
        for segment in segments {
            value = match value {
                Value::Object(object) => object.get(segment).unwrap_or(Value::Undefined),
                _ => Value::Undefined,
            };
        }
        value
    }
}

impl State {
    /// 建立空 State；`setup` 从一开始就是可读取的空对象。
    pub fn new() -> Self {
        Self::with_engine(Rc::new(Engine::default()))
    }

    /// 绑定调用方提供的 Engine；种子与序列由 Engine 持有。
    pub fn with_engine(engine: Rc<Engine>) -> Self {
        Self {
            global: HashMap::new(),
            setup: Value::object(Vec::new()),
            variables: HashMap::new(),
            temporary: HashMap::new(),
            location: Rc::new(Location::default()),
            location_state: LocationState::default(),
            engine,
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

    /// 解除 Binding 的瞬时函数路由；之后脚本调用以 `Unavailable` 失败。
    pub fn detach_script_dispatcher(&mut self) {
        self.script_dispatcher = None;
    }

    /// 表达式和脚本共用的 Engine 执行上下文。
    pub fn engine(&self) -> &Engine {
        self.engine.as_ref()
    }

    /// 启动环境注册的世界地点定义。
    pub fn location(&self) -> &Location {
        self.location.as_ref()
    }

    /// 写入时与事务快照、辅助视图分离，避免修改污染其他所有者。
    pub fn location_mut(&mut self) -> &mut Location {
        Rc::make_mut(&mut self.location)
    }

    /// 可保存、回溯的世界运行状态。
    pub fn location_state(&self) -> &LocationState {
        &self.location_state
    }

    /// 供受控 Location 操作修改运行状态；外部存档必须先完成校验。
    pub fn location_state_mut(&mut self) -> &mut LocationState {
        &mut self.location_state
    }

    /// 查询 scripts 与 Twee 共用的普通全局名称。
    pub fn global_get(&self, name: &str) -> Option<&Value> {
        self.global.get(name)
    }

    /// 查询普通全局名称是否存在。
    pub fn global_has(&self, name: &str) -> bool {
        self.global.contains_key(name)
    }

    /// 按名称遍历普通全局表。
    pub fn global_entries(&self) -> impl Iterator<Item = (&str, &Value)> {
        sorted_entries(&self.global).into_iter()
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

    /// 删除普通全局名称，并返回被移除的旧值。
    pub fn global_del(&mut self, name: &str) -> Option<Value> {
        self.global.remove(name)
    }

    /// 读取 `$name` 所属的持久游戏变量表。
    pub fn variables_get(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    /// 查询 `$name` 是否存在于持久游戏变量表。
    pub fn variables_has(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    /// 按名称遍历持久游戏变量表。
    pub fn variables_entries(&self) -> impl Iterator<Item = (&str, &Value)> {
        sorted_entries(&self.variables).into_iter()
    }

    /// 存档编码器直接借用持久变量，避免先建立一份完整 StateSnapshot。
    pub(crate) fn persistent_variables(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.variables
            .iter()
            .map(|(name, value): (&String, &Value)| (name.as_str(), value))
    }

    /// 写入 `$name`，并返回被替换的旧值。
    pub fn variables_set(&mut self, name: &str, value: Value) -> Option<Value> {
        self.variables.insert(name.to_owned(), value)
    }

    /// 删除 `$name`，并返回被移除的旧值。
    pub fn variables_del(&mut self, name: &str) -> Option<Value> {
        self.variables.remove(name)
    }

    /// 读取 `_name` 所属的临时游戏变量表。
    pub fn temporary_get(&self, name: &str) -> Option<&Value> {
        self.temporary.get(name)
    }

    /// 查询 `_name` 是否存在于临时游戏变量表。
    pub fn temporary_has(&self, name: &str) -> bool {
        self.temporary.contains_key(name)
    }

    /// 按名称遍历临时游戏变量表。
    pub fn temporary_entries(&self) -> impl Iterator<Item = (&str, &Value)> {
        sorted_entries(&self.temporary).into_iter()
    }

    /// 写入 `_name`，并返回被替换的旧值。
    pub fn temporary_set(&mut self, name: &str, value: Value) -> Option<Value> {
        self.temporary.insert(name.to_owned(), value)
    }

    /// 删除 `_name`，并返回被移除的旧值。
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

    /// 捕获持久变量与世界状态，并与活动 Value 图完全脱离。
    pub fn snapshot(&self) -> StateSnapshot {
        let names: Vec<String> = self.variables.keys().cloned().collect();
        let values: Vec<Value> = self.variables.values().cloned().collect();
        let detached: Vec<Value> = Value::detached_clone_many(&values);
        let variables: BTreeMap<String, Value> = names.into_iter().zip(detached).collect();
        StateSnapshot::from_parts(
            variables,
            self.location_state.clone(),
            self.engine.snapshot(),
        )
    }

    /// 恢复持久变量、位置，并清空 `_temporary`。
    ///
    /// `global`、`setup` 与地点定义由当前启动环境管理，保持不变。
    pub fn restore(&mut self, snapshot: StateSnapshot) {
        self.variables = snapshot.variables.into_iter().collect();
        self.location_state = snapshot.location_state;
        self.engine.restore(snapshot.engine);
        let _removed: usize = self.temporary_clear();
    }

    /// 从历史快照恢复持久状态；快照保持不可变且不与活动值图共享。
    pub fn restore_snapshot(&mut self, snapshot: &StateSnapshot) {
        self.variables = snapshot.detached_variables().into_iter().collect();
        self.location_state = snapshot.location_state.clone();
        self.engine.restore(snapshot.engine);
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
        let global: HashMap<String, Value> = global_names
            .into_iter()
            .zip(detached[..global_count].iter().cloned())
            .collect();
        let setup: Value = detached[setup_index].clone();
        let variables: HashMap<String, Value> = variable_names
            .into_iter()
            .zip(detached[variable_start..temporary_start].iter().cloned())
            .collect();
        let temporary: HashMap<String, Value> = temporary_names
            .into_iter()
            .zip(detached[temporary_start..].iter().cloned())
            .collect();

        StateCheckpoint {
            engine: self.engine.snapshot(),
            replay: self.engine.replay_snapshot(),
            global,
            setup,
            variables,
            temporary,
            location: Rc::clone(&self.location),
            location_state: self.location_state.clone(),
        }
    }

    /// 一次性恢复完整 State；该入口只用于短期运行事务。
    pub fn restore_checkpoint(&mut self, checkpoint: StateCheckpoint) {
        self.global = checkpoint.global;
        self.setup = checkpoint.setup;
        self.variables = checkpoint.variables;
        self.temporary = checkpoint.temporary;
        self.location = checkpoint.location;
        self.location_state = checkpoint.location_state;
        self.engine.restore(checkpoint.engine);
        if let Some(replay) = checkpoint.replay {
            self.engine.begin_replay(replay);
        }
    }

    /// 清空游戏变量与位置；保留 global/setup 和地点定义。
    pub fn reset_game(&mut self) -> StateReset {
        let variables_removed: usize = self.variables.len();
        self.variables.clear();
        let temporary_removed: usize = self.temporary_clear();
        self.location_state = LocationState::default();
        StateReset {
            variables_removed,
            temporary_removed,
        }
    }
}

/// HashMap 保持热路径平均 O(1)，公开遍历则在边界排序以维持稳定存档与工具输出。
fn sorted_entries(values: &HashMap<String, Value>) -> Vec<(&str, &Value)> {
    let mut entries: Vec<(&str, &Value)> = values
        .iter()
        .map(|(name, value)| (name.as_str(), value))
        .collect();
    entries.sort_unstable_by_key(|(name, _)| *name);
    entries
}

impl EvaluationContext for State {
    fn next_random(&self) -> Option<f64> {
        Some(self.engine.next_random())
    }

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

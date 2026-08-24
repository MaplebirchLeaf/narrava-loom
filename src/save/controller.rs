//! Save 的 Host 请求队列与 before／after 生命周期边界。

use std::{collections::HashMap, collections::VecDeque, error::Error, fmt};

use crate::diagnostic::{Diagnostic, DiagnosticSeverity};

/// Core 可请求、但必须由 Host 执行的存档操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveOperation {
    Export,
    Import,
}

/// 一次 Save Host 请求在当前进程中的稳定身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SaveRequestId(u64);

impl SaveRequestId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Core 交给 Host 的平台无关请求。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveRequest {
    id: SaveRequestId,
    operation: SaveOperation,
    target: String,
}

impl SaveRequest {
    pub fn id(&self) -> SaveRequestId {
        self.id
    }

    pub fn operation(&self) -> SaveOperation {
        self.operation
    }

    /// Host 自行解释目标；Core 不把它视为绝对路径。
    pub fn target(&self) -> &str {
        self.target.as_str()
    }
}

/// Host 对 Save 请求的稳定完成结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Succeeded,
    Failed(Diagnostic),
}

/// after Hook 可观察的完整结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveCompletion {
    request: SaveRequest,
    outcome: SaveOutcome,
}

impl SaveCompletion {
    pub fn request(&self) -> &SaveRequest {
        &self.request
    }

    pub fn outcome(&self) -> &SaveOutcome {
        &self.outcome
    }
}

/// Binding 执行 Save Hook 所需的最小接口。
pub trait SaveLifecycleCallbacks {
    /// 请求入队前运行；可以改写 Host 目标。
    fn before(&mut self, operation: SaveOperation, target: &mut String) -> Result<(), Diagnostic>;

    /// Host 明确完成请求后运行；结果只读，不能伪造持久化成功。
    fn after(&mut self, completion: &SaveCompletion) -> Result<(), Diagnostic>;
}

/// 将有序订阅适配为 Runtime 可调用的生命周期接口。
pub struct SaveLifecycleController<'subscriptions, Hook, Before, After> {
    subscriptions: &'subscriptions SaveLifecycleSubscriptions<Hook>,
    invoke_before: Before,
    invoke_after: After,
}

impl<'subscriptions, Hook, Before, After>
    SaveLifecycleController<'subscriptions, Hook, Before, After>
{
    pub fn new(
        subscriptions: &'subscriptions SaveLifecycleSubscriptions<Hook>,
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

impl<Hook, Before, After> SaveLifecycleCallbacks
    for SaveLifecycleController<'_, Hook, Before, After>
where
    Before: FnMut(&Hook, SaveOperation, &mut String) -> Result<(), Diagnostic>,
    After: FnMut(&Hook, &SaveCompletion) -> Result<(), Diagnostic>,
{
    fn before(&mut self, operation: SaveOperation, target: &mut String) -> Result<(), Diagnostic> {
        for hook in self.subscriptions.before_hooks(operation) {
            (self.invoke_before)(hook, operation, target)?;
        }
        Ok(())
    }

    fn after(&mut self, completion: &SaveCompletion) -> Result<(), Diagnostic> {
        for hook in self.subscriptions.after_hooks(completion.request.operation) {
            (self.invoke_after)(hook, completion)?;
        }
        Ok(())
    }
}

/// 一次 Save Hook 订阅的进程内身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SaveLifecycleSubscriptionId(u64);

impl SaveLifecycleSubscriptionId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveLifecycleSubscriptionError {
    IdExhausted,
}

struct SaveLifecycleSubscription<Hook> {
    id: SaveLifecycleSubscriptionId,
    hook: Hook,
}

/// 按操作与阶段保存 Hook；不持有 Host 函数实现。
pub struct SaveLifecycleSubscriptions<Hook> {
    next_id: u64,
    before: HashMap<SaveOperation, Vec<SaveLifecycleSubscription<Hook>>>,
    after: HashMap<SaveOperation, Vec<SaveLifecycleSubscription<Hook>>>,
}

impl<Hook> SaveLifecycleSubscriptions<Hook> {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            before: HashMap::new(),
            after: HashMap::new(),
        }
    }

    pub fn before(
        &mut self,
        operation: SaveOperation,
        hook: Hook,
    ) -> Result<SaveLifecycleSubscriptionId, SaveLifecycleSubscriptionError> {
        Self::register(&mut self.next_id, &mut self.before, operation, hook)
    }

    pub fn after(
        &mut self,
        operation: SaveOperation,
        hook: Hook,
    ) -> Result<SaveLifecycleSubscriptionId, SaveLifecycleSubscriptionError> {
        Self::register(&mut self.next_id, &mut self.after, operation, hook)
    }

    pub fn off(&mut self, id: SaveLifecycleSubscriptionId) -> Option<Hook> {
        Self::remove_from(&mut self.before, id).or_else(|| Self::remove_from(&mut self.after, id))
    }

    pub fn before_hooks(&self, operation: SaveOperation) -> impl Iterator<Item = &Hook> {
        self.before
            .get(&operation)
            .into_iter()
            .flat_map(|hooks: &Vec<SaveLifecycleSubscription<Hook>>| hooks.iter())
            .map(|subscription: &SaveLifecycleSubscription<Hook>| &subscription.hook)
    }

    pub fn after_hooks(&self, operation: SaveOperation) -> impl Iterator<Item = &Hook> {
        self.after
            .get(&operation)
            .into_iter()
            .flat_map(|hooks: &Vec<SaveLifecycleSubscription<Hook>>| hooks.iter())
            .map(|subscription: &SaveLifecycleSubscription<Hook>| &subscription.hook)
    }

    fn register(
        next_id: &mut u64,
        subscriptions: &mut HashMap<SaveOperation, Vec<SaveLifecycleSubscription<Hook>>>,
        operation: SaveOperation,
        hook: Hook,
    ) -> Result<SaveLifecycleSubscriptionId, SaveLifecycleSubscriptionError> {
        let id: SaveLifecycleSubscriptionId = SaveLifecycleSubscriptionId(*next_id);
        *next_id = next_id
            .checked_add(1)
            .ok_or(SaveLifecycleSubscriptionError::IdExhausted)?;
        subscriptions
            .entry(operation)
            .or_default()
            .push(SaveLifecycleSubscription { id, hook });
        Ok(id)
    }

    fn remove_from(
        subscriptions: &mut HashMap<SaveOperation, Vec<SaveLifecycleSubscription<Hook>>>,
        id: SaveLifecycleSubscriptionId,
    ) -> Option<Hook> {
        for hooks in subscriptions.values_mut() {
            if let Some(index) = hooks
                .iter()
                .position(|subscription: &SaveLifecycleSubscription<Hook>| subscription.id == id)
            {
                return Some(hooks.remove(index).hook);
            }
        }
        None
    }
}

impl<Hook> Default for SaveLifecycleSubscriptions<Hook> {
    fn default() -> Self {
        Self::new()
    }
}

/// 请求建立或 Hook 执行失败。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveControllerError {
    EmptyTarget,
    RequestIdExhausted,
    Lifecycle(Diagnostic),
}

impl SaveControllerError {
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::EmptyTarget => Diagnostic::new(
                "save.request.empty_target",
                DiagnosticSeverity::Error,
                "Save Host 目标不能为空",
            ),
            Self::RequestIdExhausted => Diagnostic::new(
                "save.request.id_exhausted",
                DiagnosticSeverity::Error,
                "Save 请求编号已耗尽",
            ),
            Self::Lifecycle(diagnostic) => diagnostic.clone(),
        }
    }
}

impl fmt::Display for SaveControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.diagnostic().message.as_str())
    }
}

impl Error for SaveControllerError {}

/// 游戏侧 Save API 与 Host 之间的单向请求队列。
pub struct SaveController {
    next_id: u64,
    requests: VecDeque<SaveRequest>,
}

impl SaveController {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            requests: VecDeque::new(),
        }
    }

    pub fn export(
        &mut self,
        target: &str,
        lifecycle: &mut impl SaveLifecycleCallbacks,
    ) -> Result<SaveRequestId, SaveControllerError> {
        self.request(SaveOperation::Export, target, lifecycle)
    }

    pub fn import(
        &mut self,
        target: &str,
        lifecycle: &mut impl SaveLifecycleCallbacks,
    ) -> Result<SaveRequestId, SaveControllerError> {
        self.request(SaveOperation::Import, target, lifecycle)
    }

    /// Host 按请求顺序取得所有权；Core 不在这里执行平台 I/O。
    pub fn take(&mut self) -> Option<SaveRequest> {
        self.requests.pop_front()
    }

    /// Host 完成 I/O 与 capture／restore 后回报结果，再触发 after。
    pub fn complete(
        &mut self,
        request: SaveRequest,
        outcome: SaveOutcome,
        lifecycle: &mut impl SaveLifecycleCallbacks,
    ) -> Result<SaveCompletion, SaveControllerError> {
        let completion: SaveCompletion = SaveCompletion { request, outcome };
        lifecycle
            .after(&completion)
            .map_err(SaveControllerError::Lifecycle)?;
        Ok(completion)
    }

    fn request(
        &mut self,
        operation: SaveOperation,
        target: &str,
        lifecycle: &mut impl SaveLifecycleCallbacks,
    ) -> Result<SaveRequestId, SaveControllerError> {
        let mut target: String = target.to_owned();
        lifecycle
            .before(operation, &mut target)
            .map_err(SaveControllerError::Lifecycle)?;
        if target.trim().is_empty() {
            return Err(SaveControllerError::EmptyTarget);
        }
        let id: SaveRequestId = SaveRequestId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(SaveControllerError::RequestIdExhausted)?;
        self.requests.push_back(SaveRequest {
            id,
            operation,
            target,
        });
        Ok(id)
    }
}

impl Default for SaveController {
    fn default() -> Self {
        Self::new()
    }
}

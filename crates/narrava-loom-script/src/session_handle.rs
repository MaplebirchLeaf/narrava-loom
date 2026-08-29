//! 对 Binding 隐藏 Native Session 的编译生命周期与 Script Adapter 类型。

use narrava_loom_protocol::{
    HostErrorDto, RUNTIME_PROTOCOL_VERSION, RuntimeCommand, RuntimeRequest, RuntimeResponse,
    RuntimeSessionId, RuntimeUpdate,
};

use crate::{RuntimeSession, ScriptAdapter};
use narrava_loom_core::script::ScriptCallDispatcher;

trait RuntimeDriver {
    fn execute(&mut self, command: RuntimeCommand) -> Result<RuntimeUpdate, HostErrorDto>;
    fn take_notices(&mut self) -> Vec<HostErrorDto>;
}

impl<'hir, 'source, Adapter> RuntimeDriver for RuntimeSession<'hir, 'source, Adapter>
where
    Adapter: ScriptAdapter + ScriptCallDispatcher + 'static,
{
    fn execute(&mut self, command: RuntimeCommand) -> Result<RuntimeUpdate, HostErrorDto> {
        RuntimeSession::execute(self, command)
    }

    fn take_notices(&mut self) -> Vec<HostErrorDto> {
        RuntimeSession::take_notices(self)
    }
}

/// 可跨语言复制和登记的不透明 Session handle。
///
/// Handle 只携带稳定身份，不借用编译产物，也不暴露 Rust 生命周期。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeSessionHandle {
    id: RuntimeSessionId,
}

impl RuntimeSessionHandle {
    pub fn new(id: RuntimeSessionId) -> Self {
        Self { id }
    }

    pub fn id(&self) -> &RuntimeSessionId {
        &self.id
    }
}

/// Native registry 内实际驱动 Session 的生命周期绑定资源。
///
/// 编译产物的借用只停留在 Native 层；跨语言调用方仅持有 [`RuntimeSessionHandle`]。
pub struct RuntimeSessionDriver<'runtime> {
    handle: RuntimeSessionHandle,
    driver: Box<dyn RuntimeDriver + 'runtime>,
}

impl<'runtime> RuntimeSessionDriver<'runtime> {
    pub fn new<'hir, 'source, Adapter>(
        id: RuntimeSessionId,
        session: RuntimeSession<'hir, 'source, Adapter>,
    ) -> Self
    where
        'hir: 'runtime,
        'source: 'runtime,
        Adapter: ScriptAdapter + ScriptCallDispatcher + 'static,
    {
        Self {
            handle: RuntimeSessionHandle::new(id),
            driver: Box::new(session),
        }
    }

    pub fn handle(&self) -> &RuntimeSessionHandle {
        &self.handle
    }

    /// Native Host 已经选中本 handle 时的简化命令入口。
    pub fn execute(&mut self, command: RuntimeCommand) -> Result<RuntimeUpdate, HostErrorDto> {
        self.driver.execute(command)
    }

    /// 取走上一条成功命令留下的非阻塞平台提示。
    pub fn take_notices(&mut self) -> Vec<HostErrorDto> {
        self.driver.take_notices()
    }

    /// 跨语言 request envelope 入口；拒绝误路由到另一局 Session 的请求。
    pub fn dispatch(&mut self, request: RuntimeRequest) -> Result<RuntimeResponse, HostErrorDto> {
        if request.protocol_version != RUNTIME_PROTOCOL_VERSION {
            return Err(HostErrorDto::new(
                "runtime_session.protocol_version",
                format!(
                    "不支持 Runtime Protocol v{}；当前版本为 v{}",
                    request.protocol_version, RUNTIME_PROTOCOL_VERSION
                ),
            ));
        }
        if request.session != *self.handle.id() {
            return Err(HostErrorDto::new(
                "runtime_session.id_mismatch",
                "请求的 Session ID 与当前 Runtime handle 不匹配",
            ));
        }
        let update: RuntimeUpdate = self.driver.execute(request.command)?;
        Ok(RuntimeResponse {
            protocol_version: RUNTIME_PROTOCOL_VERSION,
            session: self.handle.id().clone(),
            update,
        })
    }
}

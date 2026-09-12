//! 开发快照与脚本执行；所有入口均在 Rust 侧校验 developer 权限。

use crate::{TauriHost, WorkerRequest, receive_worker, worker_stopped};
use narrava_loom_protocol::{HostDebugSnapshotDto, HostErrorDto, HostUpdateDto, RuntimeCommand};
use std::sync::mpsc;

impl TauriHost {
    pub fn debug_cancel(&self) -> Result<(), HostErrorDto> {
        if !self.developer {
            return Err(HostErrorDto::new(
                "tauri_host.developer_disabled",
                "控制台需要启用 developer",
            ));
        }
        self.console_cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub async fn debug_complete(
        &self,
        path: String,
    ) -> Result<Vec<narrava_loom_protocol::HostDebugValueDto>, HostErrorDto> {
        if !self.developer {
            return Err(HostErrorDto::new(
                "tauri_host.developer_disabled",
                "控制台需要启用 developer",
            ));
        }
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Complete { path, reply })
            .map_err(|_| worker_stopped())?;
        receive_worker(result).await?
    }

    /// 与游戏输入共用 Worker 队列、State 事务和 Reaction 安全点。
    pub async fn debug_execute(
        &self,
        source: String,
    ) -> Result<Option<HostUpdateDto>, HostErrorDto> {
        if !self.developer {
            return Err(HostErrorDto::new(
                "tauri_host.developer_disabled",
                "控制台需要启用 developer",
            ));
        }
        self.execute_update(RuntimeCommand::DebugScript { source })
            .await
    }

    /// 返回已提交的 State、Location、随机状态和统一日志。
    pub async fn debug_snapshot(&self) -> Result<HostDebugSnapshotDto, HostErrorDto> {
        if !self.developer {
            return Err(HostErrorDto::new(
                "tauri_host.developer_disabled",
                "控制台需要启用 developer",
            ));
        }
        let (reply, result) = mpsc::channel();
        self.requests
            .send(WorkerRequest::Inspect(reply))
            .map_err(|_| worker_stopped())?;
        receive_worker(result).await?
    }
}

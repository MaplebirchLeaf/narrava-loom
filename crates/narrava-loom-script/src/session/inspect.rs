//! 只读取已提交状态；不执行表达式、作者 getter 或故事命令。

use narrava_loom_core::{
    inspect::{StateInspection, inspect_state},
    location::Place,
    random::RandomState,
};
use narrava_loom_protocol::{HostDebugSnapshotDto, HostErrorDto, HostLogRecordDto};
use serde_json::{Value, json};

use super::RuntimeSession;

impl RuntimeSession<'_, '_> {
    /// 只读成员补全，不执行输入表达式。
    pub fn console_completions(
        &mut self,
        path: &str,
    ) -> Result<Vec<narrava_loom_protocol::HostDebugValueDto>, HostErrorDto> {
        self.ensure_idle()?;
        if path.len() > 512 {
            return Ok(Vec::new());
        }
        self.script
            .console_completions(path, &mut self.state)
            .map_err(crate::ScriptError::into_host_error)
    }

    /// 宿主开发工具的拥有型快照。Pending 期间拒绝读取，避免暴露半完成事务。
    pub fn debug_snapshot(&self) -> Result<HostDebugSnapshotDto, HostErrorDto> {
        self.ensure_idle()?;
        let inspection: StateInspection = inspect_state(&self.state);
        let mut truncated: bool = inspection.truncated;
        let definitions = self.state.location().places();
        let count: usize = definitions.len();
        truncated |= count > 128;
        let places: Vec<Value> = definitions
            .take(128)
            .map(|place: &Place| {
                truncated |= place.bounds.len() > 128;
                json!({
                    "id": preview_text(&place.id, &mut truncated),
                    "name": place.name.as_ref().map(|name| preview_text(name, &mut truncated)),
                    "parent": place.parent.as_ref().map(|parent| preview_text(parent, &mut truncated)),
                    "entry": place.entry,
                    "bounds": place.bounds.iter().take(128).collect::<Vec<_>>(),
                    "vertices": place.bounds.len(),
                })
            })
            .collect();
        let random: RandomState = self.state.random_state();
        let current: Option<String> = self
            .story
            .current()
            .map(|passage| preview_text(passage.name, &mut truncated));
        let position: Option<Value> =
            self.state
                .location_state()
                .position
                .as_ref()
                .map(|position| {
                    json!({
                        "place": preview_text(&position.place, &mut truncated),
                        "point": position.point, "environment": position.environment,
                    })
                });
        let mut logs: Vec<HostLogRecordDto> = self.log_records();
        for record in &mut logs {
            record.target = preview_text(&record.target, &mut truncated);
            record.message = preview_text(&record.message, &mut truncated);
            if let Some(error) = &mut record.diagnostic {
                error.code = preview_text(&error.code, &mut truncated);
                error.message = preview_text(&error.message, &mut truncated);
                if let Some(location) = &mut error.location {
                    location.source = preview_text(&location.source, &mut truncated);
                }
            }
        }
        Ok(HostDebugSnapshotDto {
            evaluation: self.console_result.clone(),
            current,
            state: inspection.data,
            location: json!({"current": position, "places": places, "count": count}),
            random: json!({"seed": random.seed().to_string(), "state": random.state().to_string()}),
            truncated,
            logs,
        })
    }

    /// 日志读取不消费作者订阅，也不随故事回滚。
    pub fn log_records(&self) -> Vec<HostLogRecordDto> {
        self.script.log_records()
    }

    /// 平台 IO、音频错误与 Runtime 诊断进入同一有界日志。
    pub fn record_host_error(&self, error: &HostErrorDto) {
        self.script.record_host_error(error);
    }
}

/// 地点显示名称与字段值同样受预览长度限制；定义本身保持完整。
fn preview_text(text: &str, truncated: &mut bool) -> String {
    let mut characters = text.chars();
    let mut result: String = characters.by_ref().take(2048).collect();
    if characters.next().is_some() {
        *truncated = true;
        result.push_str("…[已省略]");
    }
    result
}

//! Runtime 提供拥有型只读数据；Host 只负责整理可滚动的检查文本。

use narrava_loom_protocol::{HostDebugSnapshotDto, HostLogLevelDto};

pub(crate) fn snapshot_lines(snapshot: &HostDebugSnapshotDto) -> Vec<String> {
    let mut lines: Vec<String> = vec![format!(
        "Passage: {}",
        snapshot.current.as_deref().unwrap_or("（尚未开始）")
    )];
    if snapshot.truncated {
        lines.push(String::from("状态过大，部分内容已截断。"));
    }
    for (title, value) in [
        ("State", &snapshot.state),
        ("Location", &snapshot.location),
        ("Random", &snapshot.random),
    ] {
        lines.push(String::new());
        lines.push(title.to_owned());
        let json: String = serde_json::to_string_pretty(value).expect("JSON Value 必须可序列化");
        lines.extend(json.lines().map(str::to_owned));
    }
    lines.push(String::new());
    lines.push(format!("Logs ({})", snapshot.logs.len()));
    for record in &snapshot.logs {
        let level: &str = match record.level {
            HostLogLevelDto::Trace => "trace",
            HostLogLevelDto::Debug => "debug",
            HostLogLevelDto::Info => "info",
            HostLogLevelDto::Warn => "warn",
            HostLogLevelDto::Error => "error",
        };
        let message: String = record
            .diagnostic
            .as_ref()
            .map_or_else(|| record.message.clone(), ToString::to_string);
        lines.push(format!(
            "#{} {level} [{}] {message}",
            record.sequence, record.target
        ));
    }
    lines
}

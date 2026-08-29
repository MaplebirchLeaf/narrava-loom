//! Narrava Runtime 与 Host 共享的纯数据协议。
//!
//! 本 crate 故意不依赖 Core、Script Runtime 或任何 Host。所有公开类型都是
//! 拥有型、可序列化值，可直接映射到 IPC 与其他语言。

use std::fmt;

use serde::{Deserialize, Serialize};

/// Runtime request/response envelope 的当前协议版本。
pub const RUNTIME_PROTOCOL_VERSION: u16 = contract::RUNTIME_PROTOCOL_VERSION;

pub mod contract {
    //! 由 canonical Script Contract 生成的跨语言名称目录。
    include!("contract_generated.rs");
}

/// 跨 Runtime/Host 边界的稳定错误值。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostErrorDto {
    /// 可供 Host 稳定分流的错误码。
    pub code: String,
    /// 面向玩家或开发者的可显示消息。
    pub message: String,
}

impl HostErrorDto {
    /// 从稳定代码与可显示消息构造错误。
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

impl fmt::Display for HostErrorDto {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

/// Binding 用来登记一局 Runtime 的跨语言不透明身份。
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuntimeSessionId(String);

impl RuntimeSessionId {
    /// 建立非空且适合日志、IPC 与外部 registry 使用的身份。
    pub fn new(value: impl Into<String>) -> Result<Self, HostErrorDto> {
        let value: String = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(HostErrorDto::new(
                "runtime_session.invalid_id",
                "Session ID 只允许 1 至 128 个 ASCII 字母、数字、连字符或下划线",
            ));
        }
        Ok(Self(value))
    }

    /// 读取稳定文本身份。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Host 可渲染的平台无关节点。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HostNodeDto {
    /// 无额外字形语义的文本。
    Text { key: String, text: String },
    /// 作者显式声明的硬换行。
    HardBreak { key: String },
    /// 带语义字形、调色板索引或延迟的文本。
    StyledText {
        key: String,
        text: String,
        styles: Vec<String>,
        color: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        delay: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        heading: Option<u8>,
    },
    /// 通过逻辑 Resource 路径加载的图像。
    Image {
        key: String,
        resource: String,
        alt: String,
        caption: Option<String>,
    },
    /// 将子节点路由到具名 Host 区域。
    Region {
        key: String,
        region: String,
        nodes: Vec<HostNodeDto>,
    },
    /// 可按稳定 key 定位的普通容器。
    Container {
        key: String,
        nodes: Vec<HostNodeDto>,
    },
    /// 版本化 Host capability，不支持时使用 fallback。
    Component {
        key: String,
        capability: String,
        version: u16,
        properties: serde_json::Value,
        fallback: Vec<HostNodeDto>,
    },
    /// 替换已有区域或容器内容。
    Replace {
        key: String,
        target: HostReplaceTargetDto,
        nodes: Vec<HostNodeDto>,
    },
    /// 不进入 Story 导航的客户端动作。
    Action {
        key: String,
        label: String,
        action: String,
        role: String,
    },
    /// 两个明确值之间切换的复选输入。
    Checkbox {
        key: String,
        id: String,
        unchecked: serde_json::Value,
        checked: serde_json::Value,
        selected: bool,
    },
    /// 同组互斥的单选输入。
    Radiobutton {
        key: String,
        id: String,
        group: String,
        value: serde_json::Value,
        selected: bool,
    },
    /// 自由文本输入。
    Textbox {
        key: String,
        id: String,
        value: String,
    },
    /// 链接角色的 Story 导航。
    Navigation {
        key: String,
        id: String,
        label: String,
        target: String,
    },
    /// 按钮角色的 Story 导航。
    Button {
        key: String,
        id: String,
        label: String,
        target: String,
    },
    /// 返回 Runtime 已验证的上一语境。
    SafeReturn {
        key: String,
        id: String,
        target: String,
    },
}

/// Replace 的可移植定位目标。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum HostReplaceTargetDto {
    /// 替换整个具名区域。
    Region(String),
    /// 替换指定稳定 key 的容器。
    Key(String),
}

/// 一次 Runtime 事务产生的完整渲染更新。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostUpdateDto {
    /// 产生该更新的当前 Passage。
    pub current: String,
    /// 按渲染顺序排列的顶层节点。
    pub nodes: Vec<HostNodeDto>,
}

/// Host 交给 RuntimeSession 的平台无关命令。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RuntimeCommand {
    /// 启动一局游戏。
    Start,
    /// 激活上一份 Surface 公开的交互。
    Activate { interaction: String },
    /// 向上一份 Surface 公开的输入提交值。
    Input {
        interaction: String,
        value: serde_json::Value,
    },
    /// 通过 Runtime 注入的平台 IO 执行存档导入或导出。
    Save {
        operation: SaveOperation,
        target: String,
    },
    /// 选择已安装语言；具体语言包装载仍由平台 adapter 完成。
    SelectLanguage { locale: String },
    /// 恢复指定的挂起操作；平台操作必须携带拥有型完成结果。
    Resume {
        operation: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<PendingResult>,
    },
    /// 取消指定的挂起操作。
    Cancel { operation: u64 },
}

/// Runtime 支持的存档方向。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveOperation {
    Export,
    Import,
}

impl SaveOperation {
    /// Script Contract 使用的稳定操作名。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Export => "export",
            Self::Import => "import",
        }
    }
}

/// Runtime 等待 Host 完成的平台操作。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PendingOperation {
    /// Host 等待毫秒数后以同一 operation ID 恢复。
    Delay { operation: u64, milliseconds: u64 },
    /// Host 把可选 document 写入或读出 target；Runtime 不接触文件系统。
    Save {
        operation: u64,
        direction: SaveOperation,
        target: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        document: Option<String>,
    },
    /// Host 完成语言包装载或平台确认后恢复；Runtime 再原子提交语言。
    SelectLanguage { operation: u64, locale: String },
}

impl PendingOperation {
    /// Host 恢复或取消时必须原样交回的不透明身份。
    pub fn id(&self) -> u64 {
        match self {
            Self::Delay { operation, .. }
            | Self::Save { operation, .. }
            | Self::SelectLanguage { operation, .. } => *operation,
        }
    }
}

/// Host 完成 [`PendingOperation`] 后返回的拥有型结果。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PendingResult {
    /// 操作成功；Save import 使用 document 交回读取内容。
    Save {
        #[serde(skip_serializing_if = "Option::is_none")]
        document: Option<String>,
    },
    /// 语言平台操作成功。
    SelectLanguage,
    /// 平台操作失败，保留稳定错误值。
    Failed { error: HostErrorDto },
}

/// RuntimeSession 对一条命令的拥有型结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RuntimeUpdate {
    /// 产生了一份可展示更新。
    Ready { update: HostUpdateDto },
    /// 命令已应用，但没有新的展示更新。
    Applied,
    /// Runtime 已保存 continuation，等待 Host 操作。
    Pending { operation: PendingOperation },
}

/// 跨语言 Binding 送入某一局 Runtime 的完整请求。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuntimeRequest {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u16,
    pub session: RuntimeSessionId,
    pub command: RuntimeCommand,
}

impl RuntimeRequest {
    pub fn new(session: RuntimeSessionId, command: RuntimeCommand) -> Self {
        Self {
            protocol_version: RUNTIME_PROTOCOL_VERSION,
            session,
            command,
        }
    }
}

/// 某一局 Runtime 返回给 Binding 的完整响应。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuntimeResponse {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u16,
    pub session: RuntimeSessionId,
    pub update: RuntimeUpdate,
}

#[cfg(test)]
mod tests;

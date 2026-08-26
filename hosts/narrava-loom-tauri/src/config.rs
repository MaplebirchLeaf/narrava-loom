//! Tauri Host 对游戏 `config.toml` 平台扩展的读取与校验。

use std::{error::Error, fmt, fs, io, path::Path};

use serde::Deserialize;

/// 未在 config.toml 指定时的窗口默认尺寸（逻辑像素）。
const DEFAULT_WIDTH: f64 = 1048.0;
const DEFAULT_HEIGHT: f64 = 640.0;
const DEFAULT_MIN_WIDTH: f64 = 360.0;
const DEFAULT_MIN_HEIGHT: f64 = 640.0;

/// 游戏 `config.toml` 中由 Tauri Host 消费的平台扩展。
#[derive(Debug)]
pub struct TauriProjectConfig {
    game_name: String,
    tauri: TauriConfig,
}

/// 顶层 `config.toml` 的原始形状（供 serde 反序列化）。
#[derive(Debug, Deserialize)]
struct ProjectFile {
    game: GameSection,
    #[serde(default)]
    host: HostSection,
}

/// `[game]` 段：游戏名。
#[derive(Debug, Deserialize)]
struct GameSection {
    name: String,
}

/// `[host]` 段：平台扩展根。
#[derive(Debug, Default, Deserialize)]
struct HostSection {
    #[serde(default)]
    tauri: TauriConfig,
}

/// `[host.tauri]` 段：拒绝未知字段以尽早暴露拼写错误。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TauriConfig {
    title: Option<String>,
    icon: Option<String>,
    #[serde(default)]
    developer: bool,
    #[serde(default)]
    window: TauriWindowConfig,
}

/// Tauri 主窗口中跨桌面平台稳定的基础选项。
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TauriWindowConfig {
    width: f64,
    height: f64,
    min_width: f64,
    min_height: f64,
    resizable: bool,
    fullscreen: bool,
    decorations: bool,
    maximized: bool,
}

/// Tauri 平台配置的读取、解析或校验错误。
#[derive(Debug)]
pub enum TauriConfigError {
    /// 无法读取 `config.toml`。
    Read(io::Error),
    /// `config.toml` 语法或结构错误。
    Parse(toml::de::Error),
    /// 字段值不合法（携带字段名）。
    Invalid(&'static str),
}

impl Default for TauriWindowConfig {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            min_width: DEFAULT_MIN_WIDTH,
            min_height: DEFAULT_MIN_HEIGHT,
            resizable: true,
            fullscreen: true,
            decorations: false,
            maximized: false,
        }
    }
}

impl TauriProjectConfig {
    /// 从开发目录读取并解析 `config.toml`。
    pub fn load(game_path: &Path) -> Result<Self, TauriConfigError> {
        let content: String =
            fs::read_to_string(game_path.join("config.toml")).map_err(TauriConfigError::Read)?;
        Self::parse(content.as_str())
    }

    /// 解析并校验 `config.toml` 文本（发行包内配置也走这里）。
    pub fn parse(content: &str) -> Result<Self, TauriConfigError> {
        let project: ProjectFile = toml::from_str(content).map_err(TauriConfigError::Parse)?;
        let config: Self = Self {
            game_name: project.game.name,
            tauri: project.host.tauri,
        };
        config.validate()?;
        Ok(config)
    }

    /// 窗口标题：优先取 `host.tauri.title`，缺省用游戏名。
    pub fn title(&self) -> &str {
        self.tauri.title.as_deref().unwrap_or(&self.game_name)
    }

    /// 窗口图标路径（相对游戏目录）。
    pub fn icon(&self) -> Option<&str> {
        self.tauri.icon.as_deref()
    }

    /// 是否启用开发者模式。
    pub fn developer(&self) -> bool {
        self.tauri.developer
    }

    /// 主窗口基础选项。
    pub fn window(&self) -> &TauriWindowConfig {
        &self.tauri.window
    }

    /// 校验 title 非空、icon 路径安全且窗口尺寸合法。
    fn validate(&self) -> Result<(), TauriConfigError> {
        if self.title().trim().is_empty() {
            return Err(TauriConfigError::Invalid("host.tauri.title"));
        }
        if let Some(icon) = self.icon()
            && (!is_safe_relative_path(icon) || !has_supported_icon_extension(icon))
        {
            return Err(TauriConfigError::Invalid("host.tauri.icon"));
        }
        self.tauri.window.validate()
    }
}

impl TauriWindowConfig {
    /// 窗口宽度（逻辑像素）。
    pub fn width(&self) -> f64 {
        self.width
    }

    /// 窗口高度（逻辑像素）。
    pub fn height(&self) -> f64 {
        self.height
    }

    /// 最小窗口宽度（逻辑像素）。
    pub fn min_width(&self) -> f64 {
        self.min_width
    }

    /// 最小窗口高度（逻辑像素）。
    pub fn min_height(&self) -> f64 {
        self.min_height
    }

    /// 窗口是否可调整大小。
    pub fn resizable(&self) -> bool {
        self.resizable
    }

    /// 启动时是否全屏。
    pub fn fullscreen(&self) -> bool {
        self.fullscreen
    }

    /// 是否显示系统窗口装饰。
    pub fn decorations(&self) -> bool {
        self.decorations
    }

    /// 启动时是否最大化。
    pub fn maximized(&self) -> bool {
        self.maximized
    }

    /// 校验尺寸为正有限值且最小尺寸不超过实际尺寸。
    fn validate(&self) -> Result<(), TauriConfigError> {
        let sizes: [f64; 4] = [self.width, self.height, self.min_width, self.min_height];
        if sizes
            .iter()
            .any(|value: &f64| !value.is_finite() || *value <= 0.0)
            || self.min_width > self.width
            || self.min_height > self.height
        {
            return Err(TauriConfigError::Invalid("host.tauri.window"));
        }
        Ok(())
    }
}

impl fmt::Display for TauriConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "无法读取 Tauri 游戏配置：{error}"),
            Self::Parse(error) => write!(formatter, "无法解析 Tauri 游戏配置：{error}"),
            Self::Invalid(field) => write!(formatter, "Tauri 游戏配置字段无效：{field}"),
        }
    }
}

impl Error for TauriConfigError {}

/// icon 必须是纯相对路径（无绝对路径、无反斜杠、无 `..`）。
fn is_safe_relative_path(value: &str) -> bool {
    let path: &Path = Path::new(value);
    !value.is_empty()
        && !value.contains('\\')
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

/// icon 扩展名必须是 `png` 或 `ico`。
fn has_supported_icon_extension(value: &str) -> bool {
    matches!(
        Path::new(value)
            .extension()
            .and_then(|value| value.to_str()),
        Some("png" | "ico")
    )
}

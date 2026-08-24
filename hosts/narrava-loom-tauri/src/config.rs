use std::{error::Error, fmt, fs, io, path::Path};

use serde::Deserialize;

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

#[derive(Debug, Deserialize)]
struct ProjectFile {
    game: GameSection,
    #[serde(default)]
    host: HostSection,
}

#[derive(Debug, Deserialize)]
struct GameSection {
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct HostSection {
    #[serde(default)]
    tauri: TauriConfig,
}

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

#[derive(Debug)]
pub enum TauriConfigError {
    Read(io::Error),
    Parse(toml::de::Error),
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
    pub fn load(game_path: &Path) -> Result<Self, TauriConfigError> {
        let content: String =
            fs::read_to_string(game_path.join("config.toml")).map_err(TauriConfigError::Read)?;
        Self::parse(content.as_str())
    }

    pub fn parse(content: &str) -> Result<Self, TauriConfigError> {
        let project: ProjectFile = toml::from_str(content).map_err(TauriConfigError::Parse)?;
        let config: Self = Self {
            game_name: project.game.name,
            tauri: project.host.tauri,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn title(&self) -> &str {
        self.tauri.title.as_deref().unwrap_or(&self.game_name)
    }

    pub fn icon(&self) -> Option<&str> {
        self.tauri.icon.as_deref()
    }

    pub fn developer(&self) -> bool {
        self.tauri.developer
    }

    pub fn window(&self) -> &TauriWindowConfig {
        &self.tauri.window
    }

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
    pub fn width(&self) -> f64 {
        self.width
    }

    pub fn height(&self) -> f64 {
        self.height
    }

    pub fn min_width(&self) -> f64 {
        self.min_width
    }

    pub fn min_height(&self) -> f64 {
        self.min_height
    }

    pub fn resizable(&self) -> bool {
        self.resizable
    }

    pub fn fullscreen(&self) -> bool {
        self.fullscreen
    }

    pub fn decorations(&self) -> bool {
        self.decorations
    }

    pub fn maximized(&self) -> bool {
        self.maximized
    }

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

fn is_safe_relative_path(value: &str) -> bool {
    let path: &Path = Path::new(value);
    !value.is_empty()
        && !value.contains('\\')
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn has_supported_icon_extension(value: &str) -> bool {
    matches!(
        Path::new(value)
            .extension()
            .and_then(|value| value.to_str()),
        Some("png" | "ico")
    )
}

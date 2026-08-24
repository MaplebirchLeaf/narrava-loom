//! 游戏项目配置的读取与基础字段验证。

use serde::Deserialize;
use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use crate::i18n::is_language_tag_well_formed;

/// 游戏目录中的完整项目配置。
#[derive(Deserialize)]
pub struct ProjectConfig {
    pub game: GameConfig,
}

/// `[game]` 中可直接展示和验证的游戏信息。
#[derive(Deserialize)]
pub struct GameConfig {
    pub id: String,
    pub name: String,
    pub version: String,
    /// 游戏源语言（Twee 原文语言），作为翻译基准与回退终点。
    pub default_locale: String,
}

/// 由 Config、Mod 与语言包共同复用的游戏身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameIdentity {
    id: String,
    version: semver::Version,
}

/// 建立游戏身份时可由调用者稳定分类的错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameIdentityError {
    InvalidId,
    InvalidVersion { message: String },
}

/// 模组、语言包和存档可共同使用的目标游戏兼容约束。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameCompatibility {
    id: String,
    versions: semver::VersionReq,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameCompatibilityError {
    InvalidId,
    InvalidVersionRequirement { message: String },
}

impl GameIdentity {
    pub fn new(id: impl Into<String>, version: &str) -> Result<Self, GameIdentityError> {
        let id: String = id.into();
        if !is_valid_game_id(&id) {
            return Err(GameIdentityError::InvalidId);
        }

        let version: semver::Version =
            semver::Version::parse(version).map_err(|error: semver::Error| {
                GameIdentityError::InvalidVersion {
                    message: error.to_string(),
                }
            })?;
        Ok(Self { id, version })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn version(&self) -> &semver::Version {
        &self.version
    }
}

impl GameCompatibility {
    pub fn new(id: impl Into<String>, versions: &str) -> Result<Self, GameCompatibilityError> {
        let id: String = id.into();
        if !is_valid_game_id(&id) {
            return Err(GameCompatibilityError::InvalidId);
        }

        let versions: semver::VersionReq =
            semver::VersionReq::parse(versions).map_err(|error: semver::Error| {
                GameCompatibilityError::InvalidVersionRequirement {
                    message: error.to_string(),
                }
            })?;
        Ok(Self { id, versions })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn versions(&self) -> &semver::VersionReq {
        &self.versions
    }

    pub fn matches(&self, identity: &GameIdentity) -> bool {
        self.id == identity.id && self.versions.matches(&identity.version)
    }
}

impl fmt::Display for GameIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => write!(formatter, "游戏 ID 不能为空或包含空白"),
            Self::InvalidVersion { message } => write!(formatter, "游戏版本无效: {message}"),
        }
    }
}

impl Error for GameIdentityError {}

impl fmt::Display for GameCompatibilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => write!(formatter, "目标游戏 ID 不能为空或包含空白"),
            Self::InvalidVersionRequirement { message } => {
                write!(formatter, "目标游戏版本约束无效: {message}")
            }
        }
    }
}

impl Error for GameCompatibilityError {}

fn is_valid_game_id(id: &str) -> bool {
    !id.is_empty() && !id.chars().any(char::is_whitespace)
}

/// 配置读取、TOML 解析或字段验证错误。
#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Invalid {
        path: PathBuf,
        field: &'static str,
        message: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "无法读取 {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(formatter, "无法解析 {}: {source}", path.display())
            }
            Self::Invalid {
                path,
                field,
                message,
            } => write!(formatter, "{} 中的 {field} 无效: {message}", path.display()),
        }
    }
}

impl Error for ConfigError {}

impl ProjectConfig {
    /// 读取游戏目录下的 `config.toml` 并完成字段验证。
    pub fn load(project: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path: PathBuf = project.as_ref().join("config.toml");
        let content: String =
            fs::read_to_string(&path).map_err(|source: io::Error| ConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        Self::parse(&path, &content)
    }

    /// 解析来自 `game.nar` 等可信容器位置的配置文本。
    pub fn parse(path: &Path, content: &str) -> Result<Self, ConfigError> {
        let config: Self =
            toml::from_str(content).map_err(|source: toml::de::Error| ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        config.validate(path)?;

        Ok(config)
    }

    /// 返回供 Mod、语言包与 Save 共同使用的已解析游戏身份。
    pub fn identity(&self) -> Result<GameIdentity, GameIdentityError> {
        GameIdentity::new(self.game.id.clone(), &self.game.version)
    }

    fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        match self.identity() {
            Ok(_) => {}
            Err(GameIdentityError::InvalidId) => {
                return Err(ConfigError::Invalid {
                    path: path.to_path_buf(),
                    field: "game.id",
                    message: "不能为空或包含空白".to_owned(),
                });
            }
            Err(GameIdentityError::InvalidVersion { message }) => {
                return Err(ConfigError::Invalid {
                    path: path.to_path_buf(),
                    field: "game.version",
                    message,
                });
            }
        }

        if self.game.name.trim().is_empty() {
            return Err(ConfigError::Invalid {
                path: path.to_path_buf(),
                field: "game.name",
                message: "不能为空".to_owned(),
            });
        }

        if !is_language_tag_well_formed(&self.game.default_locale) {
            return Err(ConfigError::Invalid {
                path: path.to_path_buf(),
                field: "game.default_locale",
                message: "必须是有效的语言标签".to_owned(),
            });
        }

        Ok(())
    }
}

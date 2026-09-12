//! 宿主无关的地点、多边形边界与可存档玩家位置。

mod geometry;
mod location;

use std::fmt;

use serde::{Deserialize, Serialize};

pub use location::Location;

/// 共享世界整数坐标，允许负数，与显示像素无关。
pub type Point = [i64; 2];

/// 各坐标轴绝对值的上限，兼容 ECMAScript 安全整数。
pub const MAX_COORDINATE: i64 = 9_007_199_254_740_991;

/// 地点定义；引用使用 `id`，`name` 仅用于显示。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Place {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub bounds: Vec<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<Point>,
}

/// 室内外叙事环境，不改变坐标系。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Inside,
    Outside,
}

/// 随游戏状态保存的权威玩家位置。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocationPosition {
    pub place: String,
    pub point: Point,
    #[serde(default)]
    pub environment: Option<Environment>,
}

/// 可变世界状态，与地点定义分开保存。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocationState {
    pub position: Option<LocationPosition>,
}

/// 世界数据校验错误，包含稳定错误码与具体原因。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocationError {
    code: &'static str,
    message: String,
}

impl LocationError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for LocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LocationError {}

#[cfg(test)]
mod tests;

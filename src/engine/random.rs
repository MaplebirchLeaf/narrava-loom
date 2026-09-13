//! Engine 的根种子与 SplitMix64 执行进度；State 只借用 Engine 的取样入口。

use super::Engine;
use serde::{Deserialize, Serialize};

/// 完整种子与游标使用 u64 保存；脚本边界不得将游标转换为浮点数。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineSnapshot {
    seed: u64,
    state: u64,
}

impl EngineSnapshot {
    /// 从根种子建立初始执行进度。
    pub(super) const fn new(seed: u64) -> Self {
        Self { seed, state: seed }
    }

    /// 使用高 53 位构造 [0, 1)，避免舍入到 1 或丢失平台间一致性。
    fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value: u64 = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

impl Engine {
    /// 为一局游戏接收根种子；Twee 和脚本共享此 Engine 的序列。
    pub const fn new(seed: u64) -> Self {
        Self {
            random: std::cell::Cell::new(EngineSnapshot::new(seed)),
            replay: std::cell::Cell::new(None),
        }
    }

    /// 未指定根种子时，为新游戏生成种子。
    pub fn from_entropy() -> Self {
        Self::new(fastrand::u64(..))
    }

    /// 当前游戏的根种子，不暴露重设入口。
    pub fn seed(&self) -> u64 {
        self.random.get().seed
    }

    /// 消费唯一游戏序列；重绘期间只消费临时序列。
    pub fn next_random(&self) -> f64 {
        let replay: Option<EngineSnapshot> = self.replay.get();
        let mut snapshot: EngineSnapshot = replay.unwrap_or_else(|| self.random.get());
        let unit: f64 = snapshot.next_unit();
        if replay.is_some() {
            self.replay.set(Some(snapshot));
        } else {
            self.random.set(snapshot);
        }
        unit
    }

    /// 捕获已提交序列，供事务、历史与存档使用。
    pub fn snapshot(&self) -> EngineSnapshot {
        self.random.get()
    }

    /// 恢复完整根种子与序列，清除临时重绘。
    pub fn restore(&self, snapshot: EngineSnapshot) {
        self.random.set(snapshot);
        self.end_replay();
    }

    /// 隔离重绘序列，避免消费已提交的游戏进度。
    pub fn begin_replay(&self, snapshot: EngineSnapshot) {
        self.replay.set(Some(snapshot));
    }

    /// 临时序列的当前位置，用于跨正文和公共区域继续重绘。
    pub fn replay_snapshot(&self) -> Option<EngineSnapshot> {
        self.replay.get()
    }

    /// 结束重绘，继续消费游戏序列。
    pub fn end_replay(&self) {
        self.replay.set(None);
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(0)
    }
}

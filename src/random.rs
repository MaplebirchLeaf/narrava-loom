//! 可存档的 SplitMix64 随机序列；算法与单位值转换是存档协议的一部分。

use serde::{Deserialize, Serialize};

/// 完整种子与游标使用 u64 保存；脚本边界不得将游标转换为浮点数。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RandomState {
    seed: u64,
    state: u64,
}

impl RandomState {
    /// 从明确种子开始；默认种子 0 保证新 State 可复现。
    pub const fn new(seed: u64) -> Self {
        Self { seed, state: seed }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn state(&self) -> u64 {
        self.state
    }

    /// 使用高 53 位构造 [0, 1)，避免舍入到 1 或丢失平台间一致性。
    pub fn next_unit(&mut self) -> f64 {
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

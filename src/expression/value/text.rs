//! Web 兼容的 UTF-16 文本值。
//!
//! Rust String 不能表示孤立代理项，而 ECMAScript 的切片和索引可以产生它们。
//! 本模块集中维护码元边界、大小写转换和无损存储，避免普通 Value 逻辑误用 UTF-8。

/// Web 字符串的 UTF-16 码元存储。
///
/// 它可以保留切片产生的孤立代理项，这是 Rust `String` 无法表达的合法 Web 字符串状态。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextValue {
    units: Vec<u16>,
}

impl TextValue {
    pub fn from_units(units: Vec<u16>) -> Self {
        Self { units }
    }

    pub fn len(&self) -> usize {
        self.units.len()
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    pub fn as_units(&self) -> &[u16] {
        &self.units
    }

    /// 边界按 UTF-16 码元计算，并限制在当前文本范围内。
    pub fn slice_units(&self, start: usize, end: usize) -> Self {
        let start: usize = start.min(self.units.len());
        let end: usize = end.max(start).min(self.units.len());
        Self {
            units: self.units[start..end].to_vec(),
        }
    }

    /// 仅在没有孤立代理项时转换回 Rust Unicode 字符串。
    pub fn to_unicode_string(&self) -> Option<String> {
        String::from_utf16(&self.units).ok()
    }

    pub(in crate::expression) fn append(&mut self, other: &Self) {
        self.units.extend_from_slice(&other.units);
    }

    pub(in crate::expression) fn contains(&self, other: &Self) -> bool {
        other.is_empty()
            || self
                .units
                .windows(other.len())
                .any(|window: &[u16]| window == other.units)
    }

    pub(in crate::expression) fn starts_with(&self, other: &Self) -> bool {
        self.units.starts_with(&other.units)
    }

    pub(in crate::expression) fn ends_with(&self, other: &Self) -> bool {
        self.units.ends_with(&other.units)
    }

    pub(in crate::expression) fn to_lowercase(&self) -> Self {
        self.map_case(char::to_lowercase)
    }

    pub(in crate::expression) fn to_uppercase(&self) -> Self {
        self.map_case(char::to_uppercase)
    }

    fn map_case<I>(&self, map: impl Fn(char) -> I) -> Self
    where
        I: Iterator<Item = char>,
    {
        let mut units: Vec<u16> = Vec::with_capacity(self.units.len());
        for decoded in char::decode_utf16(self.units.iter().copied()) {
            match decoded {
                Ok(character) => {
                    for mapped in map(character) {
                        let mut buffer: [u16; 2] = [0; 2];
                        units.extend_from_slice(mapped.encode_utf16(&mut buffer));
                    }
                }
                Err(error) => units.push(error.unpaired_surrogate()),
            }
        }
        Self { units }
    }
}

impl From<&str> for TextValue {
    fn from(value: &str) -> Self {
        Self {
            units: value.encode_utf16().collect(),
        }
    }
}

impl From<String> for TextValue {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

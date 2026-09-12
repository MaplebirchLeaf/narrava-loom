//! 将 State 值图转换为有界、独立的只读预览；不求值表达式或调用脚本。

use std::collections::HashSet;

use serde_json::{Map, Value as Json};

use crate::{expression::value::Value, state::State};

const MAX_NODES: usize = 1024;
const MAX_DEPTH: usize = 12;
const MAX_ITEMS: usize = 128;
const MAX_TEXT: usize = 2048;

/// 预览中的省略是显示限制，不改变被检查的原始 State。
pub struct StateInspection {
    pub data: Json,
    pub truncated: bool,
}

/// 读取四个命名空间；循环、函数和非 JSON 数值使用明确的显示标记。
pub fn inspect_state(state: &State) -> StateInspection {
    let mut inspector: Inspector = Inspector {
        remaining: MAX_NODES,
        ancestors: HashSet::new(),
        truncated: false,
    };
    let data: Json = serde_json::json!({
        "variables": inspector.entries(state.variables_entries(), 0),
        "temporary": inspector.entries(state.temporary_entries(), 0),
        "setup": inspector.value(state.setup_get(), 0),
        "global": inspector.entries(state.global_entries(), 0),
    });
    StateInspection {
        data,
        truncated: inspector.truncated,
    }
}

struct Inspector {
    remaining: usize,
    ancestors: HashSet<(u8, usize)>,
    truncated: bool,
}

impl Inspector {
    fn omitted(&mut self) -> Json {
        self.truncated = true;
        Json::String("[已省略]".into())
    }

    fn text(&mut self, text: &str) -> Json {
        let mut result: String = text.chars().take(MAX_TEXT).collect();
        if text.chars().nth(MAX_TEXT).is_some() {
            self.truncated = true;
            result.push_str("…[已省略]");
        }
        Json::String(result)
    }

    fn entries<'a>(
        &mut self,
        entries: impl IntoIterator<Item = (&'a str, &'a Value)>,
        depth: usize,
    ) -> Json {
        let mut result: Map<String, Json> = Map::new();
        for (index, (name, value)) in entries.into_iter().enumerate() {
            if index >= MAX_ITEMS || self.remaining == 0 {
                self.truncated = true;
                break;
            }
            // 省略超长字段名，避免截短后的两个键相互覆盖。
            if name.chars().nth(MAX_TEXT).is_some() {
                self.truncated = true;
                continue;
            }
            result.insert(name.to_owned(), self.value(value, depth + 1));
        }
        Json::Object(result)
    }

    fn value(&mut self, value: &Value, depth: usize) -> Json {
        if self.remaining == 0 || depth >= MAX_DEPTH {
            return self.omitted();
        }
        self.remaining -= 1;
        match value {
            Value::Undefined => Json::String("[undefined]".into()),
            Value::Null => Json::Null,
            Value::Boolean(value) => Json::Bool(*value),
            Value::Number(value) => serde_json::Number::from_f64(*value)
                .map(Json::Number)
                .unwrap_or_else(|| Json::String(format!("[{value}]"))),
            Value::String(value) => {
                // 只解码需要显示的前缀，不为长字符串分配完整 UTF-8 副本。
                let mut characters = char::decode_utf16(value.as_units().iter().copied());
                let prefix: Result<String, _> = characters.by_ref().take(MAX_TEXT).collect();
                match prefix {
                    Ok(mut text) => {
                        if characters.next().is_some() {
                            self.truncated = true;
                            text.push_str("…[已省略]");
                        }
                        Json::String(text)
                    }
                    Err(_) => Json::String("[无效 UTF-16]".into()),
                }
            }
            Value::ScriptCallable(callable) => {
                let Json::String(name) = self.text(callable.name()) else {
                    unreachable!()
                };
                Json::String(format!("[函数 {name}]"))
            }
            Value::Callable(_) => Json::String("[原生函数]".into()),
            Value::Namespace(_) => Json::String("[原生命名空间]".into()),
            Value::Array(array) => {
                let key: (u8, usize) = (0, array.identity());
                if !self.ancestors.insert(key) {
                    return Json::String("[循环引用]".into());
                }
                // 借用原集合的有限前缀，避免为了查看少量元素复制整个大数组。
                let result: Json = array.with_ref(|values: &[Value]| {
                    let mut items: Vec<Json> = Vec::new();
                    for value in values.iter().take(MAX_ITEMS) {
                        if self.remaining == 0 {
                            break;
                        }
                        items.push(self.value(value, depth + 1));
                    }
                    if items.len() < values.len() {
                        items.push(self.omitted());
                    }
                    Json::Array(items)
                });
                self.ancestors.remove(&key);
                result
            }
            Value::Object(object) => {
                let key: (u8, usize) = (1, object.identity());
                if !self.ancestors.insert(key) {
                    return Json::String("[循环引用]".into());
                }
                let result: Json = object.with_ref(|values: &[(String, Value)]| {
                    self.entries(
                        values.iter().map(|(name, value)| (name.as_str(), value)),
                        depth,
                    )
                });
                self.ancestors.remove(&key);
                result
            }
        }
    }
}

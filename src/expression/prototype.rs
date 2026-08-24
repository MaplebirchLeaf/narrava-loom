//! Expression 的只读内置原型身份与继承关系。

use super::value::Value;

/// 原型身份不直接暴露宿主 JavaScript 对象。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Prototype {
    Object,
    Array,
    String,
    Number,
    Boolean,
    Function,
}

impl Prototype {
    /// 只有引擎登记的名称能作为 `instanceof` 右侧。
    pub(super) fn from_name(name: &str) -> Option<Self> {
        match name {
            "Object" => Some(Self::Object),
            "Array" => Some(Self::Array),
            "String" => Some(Self::String),
            "Number" => Some(Self::Number),
            "Boolean" => Some(Self::Boolean),
            "Function" => Some(Self::Function),
            _ => None,
        }
    }

    fn parent(self) -> Option<Self> {
        match self {
            Self::Object => None,
            Self::Array | Self::String | Self::Number | Self::Boolean | Self::Function => {
                Some(Self::Object)
            }
        }
    }
}

/// 判断值的直接原型或任一只读父原型是否匹配。
pub(super) fn is_instance(value: &Value, expected: Prototype) -> bool {
    let mut current: Option<Prototype> = match value {
        Value::Array(_) => Some(Prototype::Array),
        Value::Object(_) => Some(Prototype::Object),
        Value::String(_) => Some(Prototype::String),
        Value::Number(_) => Some(Prototype::Number),
        Value::Boolean(_) => Some(Prototype::Boolean),
        Value::Callable(_) | Value::ScriptCallable(_) => Some(Prototype::Function),
        Value::Namespace(_) => Some(Prototype::Object),
        Value::Undefined | Value::Null => None,
    };

    while let Some(prototype) = current {
        if prototype == expected {
            return true;
        }
        current = prototype.parent();
    }

    false
}

//! Expression 求值结果的基础值模型。

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

mod text;

pub use text::TextValue;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ValueReferenceKey {
    Array(usize),
    Object(usize),
}

/// Array、Object 迁移前共用的内部引用句柄。
///
/// 克隆句柄只增加引用，不复制其中的集合；因此身份和修改都能跨克隆保留。
#[derive(Clone, Debug)]
pub(crate) struct ValueReference<T> {
    inner: Rc<RefCell<T>>,
}

impl<T> ValueReference<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            inner: Rc::new(RefCell::new(value)),
        }
    }

    pub(crate) fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }

    pub(crate) fn identity(&self) -> usize {
        Rc::as_ptr(&self.inner) as usize
    }

    pub(crate) fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.inner.borrow())
    }

    pub(crate) fn with_mut<R>(&self, write: impl FnOnce(&mut T) -> R) -> R {
        write(&mut self.inner.borrow_mut())
    }
}

/// Narrava Array 的共享引用值。
///
/// `Clone` 只克隆句柄；集合内容由 Expression 内部通过受控借用访问。
#[derive(Clone, Debug)]
pub struct ArrayValue {
    reference: ValueReference<Vec<Value>>,
}

impl ArrayValue {
    pub fn new(values: Vec<Value>) -> Self {
        Self {
            reference: ValueReference::new(values),
        }
    }

    pub fn len(&self) -> usize {
        self.reference.with(Vec::len)
    }

    pub fn is_empty(&self) -> bool {
        self.reference.with(Vec::is_empty)
    }

    pub fn snapshot(&self) -> Vec<Value> {
        self.reference.with(Clone::clone)
    }

    pub fn same_identity(&self, other: &Self) -> bool {
        self.reference.same_identity(&other.reference)
    }

    pub(crate) fn identity(&self) -> usize {
        self.reference.identity()
    }

    pub(super) fn with<R>(&self, read: impl FnOnce(&Vec<Value>) -> R) -> R {
        self.reference.with(read)
    }

    pub(crate) fn with_mut<R>(&self, write: impl FnOnce(&mut Vec<Value>) -> R) -> R {
        self.reference.with_mut(write)
    }
}

impl PartialEq for ArrayValue {
    fn eq(&self, other: &Self) -> bool {
        self.same_identity(other)
            || self.with(|left: &Vec<Value>| other.with(|right: &Vec<Value>| left == right))
    }
}

/// Narrava Object 的共享引用值，属性顺序与源码和后续插入顺序一致。
#[derive(Clone, Debug)]
pub struct ObjectValue {
    reference: ValueReference<Vec<(String, Value)>>,
}

impl ObjectValue {
    pub fn new(properties: Vec<(String, Value)>) -> Self {
        Self {
            reference: ValueReference::new(properties),
        }
    }

    pub fn len(&self) -> usize {
        self.reference.with(Vec::len)
    }

    pub fn is_empty(&self) -> bool {
        self.reference.with(Vec::is_empty)
    }

    pub fn snapshot(&self) -> Vec<(String, Value)> {
        self.reference.with(Clone::clone)
    }

    pub fn same_identity(&self, other: &Self) -> bool {
        self.reference.same_identity(&other.reference)
    }

    pub(crate) fn identity(&self) -> usize {
        self.reference.identity()
    }

    pub(super) fn with<R>(&self, read: impl FnOnce(&Vec<(String, Value)>) -> R) -> R {
        self.reference.with(read)
    }

    pub(crate) fn with_mut<R>(&self, write: impl FnOnce(&mut Vec<(String, Value)>) -> R) -> R {
        self.reference.with_mut(write)
    }
}

impl PartialEq for ObjectValue {
    fn eq(&self, other: &Self) -> bool {
        self.same_identity(other)
            || self.with(|left: &Vec<(String, Value)>| {
                other.with(|right: &Vec<(String, Value)>| left == right)
            })
    }
}

/// Expression 的首轮运行时值。
///
/// 数字使用 `f64` 接近 Web 的 `number`；对象属性使用有序列表，保留源码顺序。
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Array(ArrayValue),
    Callable(NativeCallable),
    ScriptCallable(ScriptCallable),
    Namespace(NativeNamespace),
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    Object(ObjectValue),
    String(TextValue),
}

/// 宿主脚本函数在 Core 中的稳定身份。
///
/// Core 不保存 JavaScript Function 或特定脚本引擎对象；Binding 使用 `id`
/// 找回真正的函数，`name` 仅用于诊断与调试。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScriptCallable {
    id: u64,
    name: String,
}

impl ScriptCallable {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

/// 引擎内置命名空间不会连接到宿主 JavaScript 全局对象。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeNamespace {
    Object,
}

/// 引擎提供的原生方法；接收者在成员读取时绑定。
#[derive(Clone, Debug)]
pub struct NativeCallable {
    kind: NativeCallableKind,
}

/// 原生 callable 可以是绑定接收者的方法，也可以是全局标准函数。
#[derive(Clone, Debug, PartialEq)]
pub(super) enum NativeCallableKind {
    Function(NativeFunction),
    Method {
        receiver: Box<Value>,
        method: NativeMethod,
    },
}

/// 由 Expression 函数表保留的全局函数身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeFunction {
    Abs,
    Boolean,
    Ceil,
    Clamp,
    Defined,
    Empty,
    Entries,
    Either,
    Floor,
    Keys,
    Max,
    Min,
    Number,
    ObjectAssign,
    ObjectHasOwn,
    Random,
    Round,
    String,
    Values,
}

impl NativeFunction {
    pub(super) fn argument_range(self) -> std::ops::RangeInclusive<usize> {
        match self {
            Self::Clamp => 3..=3,
            Self::Either => 1..=usize::MAX,
            Self::Max | Self::Min => 1..=usize::MAX,
            Self::Random => 0..=0,
            Self::ObjectAssign => 1..=usize::MAX,
            Self::ObjectHasOwn => 2..=2,
            Self::Abs
            | Self::Boolean
            | Self::Ceil
            | Self::Defined
            | Self::Empty
            | Self::Entries
            | Self::Floor
            | Self::Keys
            | Self::Number
            | Self::Round
            | Self::String
            | Self::Values => 1..=1,
        }
    }
}

/// 已登记到内置原型表的方法身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeMethod {
    ArrayAt,
    ArrayConcat,
    ArrayIncludes,
    ArrayIndexOf,
    ArrayJoin,
    ArrayPop,
    ArrayPush,
    ArrayShift,
    ArraySlice,
    ArraySplice,
    ArrayUnshift,
    StringEndsWith,
    StringIncludes,
    StringSlice,
    StringSplit,
    StringStartsWith,
    StringToLowerCase,
    StringToUpperCase,
    StringTrim,
}

impl NativeMethod {
    /// 参数数量属于方法签名，不由通用调用器猜测。
    pub(super) fn argument_range(self) -> std::ops::RangeInclusive<usize> {
        match self {
            Self::ArrayConcat | Self::ArrayPush | Self::ArraySplice | Self::ArrayUnshift => {
                0..=usize::MAX
            }
            Self::ArraySlice => 0..=2,
            Self::StringSlice | Self::StringSplit => 0..=2,
            Self::ArrayIndexOf => 1..=2,
            Self::ArrayJoin => 0..=1,
            Self::ArrayPop
            | Self::ArrayShift
            | Self::StringToLowerCase
            | Self::StringToUpperCase
            | Self::StringTrim => 0..=0,
            Self::ArrayAt
            | Self::ArrayIncludes
            | Self::StringEndsWith
            | Self::StringIncludes
            | Self::StringStartsWith => 1..=1,
        }
    }
}

impl NativeCallable {
    pub(crate) fn bind(receiver: Value, method: NativeMethod) -> Self {
        Self {
            kind: NativeCallableKind::Method {
                receiver: Box::new(receiver),
                method,
            },
        }
    }

    pub(super) fn function(function: NativeFunction) -> Self {
        Self {
            kind: NativeCallableKind::Function(function),
        }
    }

    pub(super) fn into_kind(self) -> NativeCallableKind {
        self.kind
    }

    pub(super) fn same_identity(&self, other: &Self) -> bool {
        match (&self.kind, &other.kind) {
            (NativeCallableKind::Function(left), NativeCallableKind::Function(right)) => {
                left == right
            }
            (
                NativeCallableKind::Method { method: left, .. },
                NativeCallableKind::Method { method: right, .. },
            ) => left == right,
            _ => false,
        }
    }

    fn detached_clone_with(&self, cloned: &mut HashMap<ValueReferenceKey, Value>) -> Self {
        let kind: NativeCallableKind = match &self.kind {
            NativeCallableKind::Function(function) => NativeCallableKind::Function(*function),
            NativeCallableKind::Method { receiver, method } => NativeCallableKind::Method {
                receiver: Box::new(receiver.detached_clone_with(cloned)),
                method: *method,
            },
        };
        Self { kind }
    }
}

impl PartialEq for NativeCallable {
    fn eq(&self, other: &Self) -> bool {
        self.same_identity(other)
    }
}

impl Value {
    pub fn array(values: Vec<Value>) -> Self {
        Self::Array(ArrayValue::new(values))
    }

    pub fn object(properties: Vec<(String, Value)>) -> Self {
        Self::Object(ObjectValue::new(properties))
    }

    pub fn string(value: impl Into<TextValue>) -> Self {
        Self::String(value.into())
    }

    /// 判断 Value 图是否只含可进入存档数据的值。
    ///
    /// Callable 与内置命名空间都属于运行环境能力；共享或循环集合会按身份去重检查。
    pub fn is_saveable(&self) -> bool {
        self.is_saveable_with(&mut HashSet::new())
    }

    fn is_saveable_with(&self, visited: &mut HashSet<ValueReferenceKey>) -> bool {
        match self {
            Self::Callable(_) | Self::ScriptCallable(_) | Self::Namespace(_) => false,
            Self::Array(array) => {
                let key: ValueReferenceKey = ValueReferenceKey::Array(array.reference.identity());
                if !visited.insert(key) {
                    return true;
                }
                array.with(|values: &Vec<Value>| {
                    values
                        .iter()
                        .all(|value: &Value| value.is_saveable_with(visited))
                })
            }
            Self::Object(object) => {
                let key: ValueReferenceKey = ValueReferenceKey::Object(object.reference.identity());
                if !visited.insert(key) {
                    return true;
                }
                object.with(|properties: &Vec<(String, Value)>| {
                    properties
                        .iter()
                        .all(|(_name, value): &(String, Value)| value.is_saveable_with(visited))
                })
            }
            Self::Undefined | Self::Null | Self::Boolean(_) | Self::Number(_) | Self::String(_) => {
                true
            }
        }
    }

    /// 克隆完整 Value 图，同时与原 Array/Object 引用身份脱离。
    pub fn detached_clone(&self) -> Self {
        let mut cloned: HashMap<ValueReferenceKey, Value> = HashMap::new();
        self.detached_clone_with(&mut cloned)
    }

    /// 使用同一图映射克隆多个根值，保留根之间的共享引用。
    pub fn detached_clone_many(values: &[Value]) -> Vec<Value> {
        let mut cloned: HashMap<ValueReferenceKey, Value> = HashMap::new();
        values
            .iter()
            .map(|value: &Value| value.detached_clone_with(&mut cloned))
            .collect()
    }

    fn detached_clone_with(&self, cloned: &mut HashMap<ValueReferenceKey, Value>) -> Self {
        match self {
            Self::Array(array) => {
                let key: ValueReferenceKey = ValueReferenceKey::Array(array.reference.identity());
                let existing: Option<&Value> = cloned.get(&key);
                if let Some(existing) = existing {
                    return existing.clone();
                }

                let detached: Value = Self::array(Vec::new());
                let _previous: Option<Value> = cloned.insert(key, detached.clone());
                let values: Vec<Value> = array
                    .snapshot()
                    .iter()
                    .map(|value: &Value| value.detached_clone_with(cloned))
                    .collect();
                let Self::Array(target) = &detached else {
                    unreachable!("Array 占位值必须保持 Array 类型")
                };
                target.with_mut(|target_values: &mut Vec<Value>| *target_values = values);
                detached
            }
            Self::Object(object) => {
                let key: ValueReferenceKey = ValueReferenceKey::Object(object.reference.identity());
                let existing: Option<&Value> = cloned.get(&key);
                if let Some(existing) = existing {
                    return existing.clone();
                }

                let detached: Value = Self::object(Vec::new());
                let _previous: Option<Value> = cloned.insert(key, detached.clone());
                let properties: Vec<(String, Value)> = object
                    .snapshot()
                    .iter()
                    .map(|(name, value): &(String, Value)| {
                        (name.clone(), value.detached_clone_with(cloned))
                    })
                    .collect();
                let Self::Object(target) = &detached else {
                    unreachable!("Object 占位值必须保持 Object 类型")
                };
                target.with_mut(|target_properties: &mut Vec<(String, Value)>| {
                    *target_properties = properties;
                });
                detached
            }
            Self::Callable(callable) => Self::Callable(callable.detached_clone_with(cloned)),
            Self::ScriptCallable(callable) => Self::ScriptCallable(callable.clone()),
            Self::Namespace(namespace) => Self::Namespace(*namespace),
            Self::Undefined => Self::Undefined,
            Self::Null => Self::Null,
            Self::Boolean(value) => Self::Boolean(*value),
            Self::Number(value) => Self::Number(*value),
            Self::String(value) => Self::String(value.clone()),
        }
    }

    /// 空值合并把 `null` 和 `undefined` 视为同一类空值。
    pub fn is_nullish(&self) -> bool {
        matches!(self, Self::Null | Self::Undefined)
    }

    /// 条件判断遵循 Web 真假值规则；空数组和空对象也是真值。
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Boolean(value) => *value,
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::String(value) => !value.is_empty(),
            Self::Array(_)
            | Self::Callable(_)
            | Self::ScriptCallable(_)
            | Self::Namespace(_)
            | Self::Object(_) => true,
        }
    }

    /// 返回脚本层可见的 Narrava `typeof` 名称。
    ///
    /// Narrava 单独区分 `null`、数组和普通对象，不继承 JS 的历史兼容结果。
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Undefined => "undefined",
            Self::Null => "null",
            Self::Array(_) => "array",
            Self::Callable(_) => "function",
            Self::ScriptCallable(_) => "function",
            Self::Namespace(_) => "object",
            Self::Object(_) => "object",
            Self::Boolean(_) => "boolean",
            Self::Number(_) => "number",
            Self::String(_) => "string",
        }
    }
}

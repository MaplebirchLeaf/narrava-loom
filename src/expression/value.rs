//! Expression 求值结果的基础值模型。

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

mod text;

pub use text::TextValue;

/// 存档检查与深度克隆中，按身份去重集合引用的键，区分 Array 与 Object。
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
    /// 把值包装进共享引用句柄。
    pub(crate) fn new(value: T) -> Self {
        Self {
            inner: Rc::new(RefCell::new(value)),
        }
    }

    /// 两个句柄是否指向同一内部值。
    pub(crate) fn same_identity(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }

    /// 内部指针值，用于存档去重等身份判断。
    pub(crate) fn identity(&self) -> usize {
        Rc::as_ptr(&self.inner) as usize
    }

    /// 受控只读访问内部值。
    pub(crate) fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.inner.borrow())
    }

    /// 受控可变访问内部值。
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
    /// 从元素列表创建共享数组值。
    pub fn new(values: Vec<Value>) -> Self {
        Self {
            reference: ValueReference::new(values),
        }
    }

    /// 返回元素数量。
    pub fn len(&self) -> usize {
        self.reference.with(Vec::len)
    }

    /// 是否不含任何元素。
    pub fn is_empty(&self) -> bool {
        self.reference.with(Vec::is_empty)
    }

    /// 克隆全部元素；结果与共享引用脱离关系。
    pub fn snapshot(&self) -> Vec<Value> {
        self.reference.with(Clone::clone)
    }

    /// 两个句柄是否指向同一共享数组。
    pub fn same_identity(&self, other: &Self) -> bool {
        self.reference.same_identity(&other.reference)
    }

    /// 内部身份指针，用于存档去重等内部判断。
    pub(crate) fn identity(&self) -> usize {
        self.reference.identity()
    }

    /// 存档等 Core 内部边界在借用期间读取元素，避免先克隆整个集合。
    pub(crate) fn with_ref<R>(&self, read: impl FnOnce(&[Value]) -> R) -> R {
        self.reference.with(|values: &Vec<Value>| read(values))
    }

    /// 受控只读访问共享元素。
    pub(super) fn with<R>(&self, read: impl FnOnce(&Vec<Value>) -> R) -> R {
        self.reference.with(read)
    }

    /// 受控可变访问共享元素，修改会跨克隆保留。
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
    /// 属性仍按插入顺序保存；首次按名读取后建立共享哈希索引，写入时失效。
    index: Rc<RefCell<Option<HashMap<String, usize>>>>,
}

impl ObjectValue {
    /// 从有序属性列表创建共享对象值，顺序与源码和插入顺序一致。
    pub fn new(properties: Vec<(String, Value)>) -> Self {
        Self {
            reference: ValueReference::new(properties),
            index: Rc::new(RefCell::new(None)),
        }
    }

    /// 返回属性数量。
    pub fn len(&self) -> usize {
        self.reference.with(Vec::len)
    }

    /// 是否不含任何属性。
    pub fn is_empty(&self) -> bool {
        self.reference.with(Vec::is_empty)
    }

    /// 克隆全部属性；结果与共享引用脱离关系。
    pub fn snapshot(&self) -> Vec<(String, Value)> {
        self.reference.with(Clone::clone)
    }

    /// 按属性名进行平均 O(1) 查找；返回 Value 句柄克隆而不复制集合图。
    pub fn get(&self, name: &str) -> Option<Value> {
        let position: usize = self.position(name)?;
        self.reference
            .with(|properties| properties.get(position).map(|(_, value)| value.clone()))
    }

    /// 查询自身属性是否存在，不进入任何原型语义。
    pub fn contains_key(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    /// 写入自身属性并返回旧值；已有属性保持原位置，新属性追加到末尾。
    pub fn insert(&self, name: impl Into<String>, value: Value) -> Option<Value> {
        let name: String = name.into();
        if let Some(position) = self.position(&name) {
            return self.reference.with_mut(|properties| {
                *self.index.borrow_mut() = None;
                properties
                    .get_mut(position)
                    .map(|(_, current)| std::mem::replace(current, value))
            });
        }
        self.with_mut(|properties| properties.push((name, value)));
        None
    }

    /// 删除自身属性并返回旧值；后续哈希索引会按剩余顺序惰性重建。
    pub fn remove(&self, name: &str) -> Option<Value> {
        let position: usize = self.position(name)?;
        self.with_mut(|properties| Some(properties.remove(position).1))
    }

    fn position(&self, name: &str) -> Option<usize> {
        if self.index.borrow().is_none() {
            let index: HashMap<String, usize> = self.reference.with(|properties| {
                properties
                    .iter()
                    .enumerate()
                    .map(|(position, (name, _))| (name.clone(), position))
                    .collect()
            });
            *self.index.borrow_mut() = Some(index);
        }
        self.index
            .borrow()
            .as_ref()
            .and_then(|index| index.get(name).copied())
    }

    /// 两个句柄是否指向同一共享对象。
    pub fn same_identity(&self, other: &Self) -> bool {
        self.reference.same_identity(&other.reference)
    }

    /// 内部身份指针，用于存档去重等内部判断。
    pub(crate) fn identity(&self) -> usize {
        self.reference.identity()
    }

    /// 存档等 Core 内部边界在借用期间读取属性，避免先克隆整个集合。
    pub(crate) fn with_ref<R>(&self, read: impl FnOnce(&[(String, Value)]) -> R) -> R {
        self.reference
            .with(|properties: &Vec<(String, Value)>| read(properties))
    }

    /// 受控只读访问共享属性。
    pub(super) fn with<R>(&self, read: impl FnOnce(&Vec<(String, Value)>) -> R) -> R {
        self.reference.with(read)
    }

    /// 受控可变访问共享属性，修改会跨克隆保留。
    pub(crate) fn with_mut<R>(&self, write: impl FnOnce(&mut Vec<(String, Value)>) -> R) -> R {
        // 先失效可保证 write panic 时也不会留下指向旧位置的索引。
        *self.index.borrow_mut() = None;
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

/// Expression 的运行时值。
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
    /// 以 Binding 登记的稳定 ID 与诊断名创建句柄。
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }

    /// 返回 Binding 侧找回真实函数对象的 ID。
    pub fn id(&self) -> u64 {
        self.id
    }

    /// 返回仅供诊断与调试使用的函数名。
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
    Clone,
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
    /// 每个函数的固定参数数量或范围，作为调用前的检查依据。
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
            | Self::Clone
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
    /// 把接收者与方法绑定为可调用值。
    pub(crate) fn bind(receiver: Value, method: NativeMethod) -> Self {
        Self {
            kind: NativeCallableKind::Method {
                receiver: Box::new(receiver),
                method,
            },
        }
    }

    /// 由全局函数身份创建可调用值。
    pub(super) fn function(function: NativeFunction) -> Self {
        Self {
            kind: NativeCallableKind::Function(function),
        }
    }

    /// 解包为全局函数或绑定方法。
    pub(super) fn into_kind(self) -> NativeCallableKind {
        self.kind
    }

    /// 函数按身份比较；绑定方法只比较方法名，不比较接收者。
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

    /// 使用共享图映射克隆自身；调用方维护去重表，保留根之间的共享引用。
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
    /// 创建共享数组值。
    pub fn array(values: Vec<Value>) -> Self {
        Self::Array(ArrayValue::new(values))
    }

    /// 创建共享对象值。
    pub fn object(properties: Vec<(String, Value)>) -> Self {
        Self::Object(ObjectValue::new(properties))
    }

    /// 创建字符串值；接受 `&str`、`String` 或现成的 `TextValue`。
    pub fn string(value: impl Into<TextValue>) -> Self {
        Self::String(value.into())
    }

    /// 判断 Value 图是否只含可进入存档数据的值。
    ///
    /// Callable 与内置命名空间都属于运行环境能力；共享或循环集合会按身份去重检查。
    pub fn is_saveable(&self) -> bool {
        self.is_saveable_with(&mut HashSet::new())
    }

    /// 带访问集合的存档检查；已访问过的共享引用直接视为可保存，避免循环递归。
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

    /// 使用共享图映射克隆自身；共享引用在图中只克隆一次。
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

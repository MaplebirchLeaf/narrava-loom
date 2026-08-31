# Expression

Expression 是 Twee Macro 和 Runtime 共用的受控表达式语言。它借用 JavaScript 的常用写法，
但不执行任意 JavaScript，也不暴露宿主原型或对象。可用运算符、函数和方法的精确
清单见 [API 与语法速查](../reference/api-and-syntax.md)。

## 管线

```text
source → Lexer → Token → Parser → AST → Evaluator → Value
```

Token 和 AST 都保留 UTF-8 字节 Span。Macro 参数中的局部 Span 通过 `DiagnosticLocator`
映射回 Twee Source。Lexer、Parser 和 Evaluator 返回结构化错误，不用日志或默认值
吞掉失败。

Parser 明确定义优先级和结合性。赋值、幂运算和三目运算右结合；成员、索引、
调用和可选链作为一条链解析。`??` 与 `&&`/`||` 混用时必须显式分组。

## Value

Runtime Value 包含：

- `undefined`、`null`、`boolean`、`number`、`string`；
- Array 与 Object；
- 受控的原生或脚本 Callable。

`null` 与 `undefined` 保持独立，只在 nullish 语义中归为一类。`number` 使用 `f64`，
保留 `NaN`、无穷值和有符号零。

Array 与 Object 由引用句柄持有。克隆 Value 不会深复制集合；别名可见同一引用的
修改。两个内容相同但独立创建的集合不相等。`clone(value)` 显式深复制 Value
图，保留图内别名和循环，不与原图共享集合身份。

Array 是稠密序列。索引必须是规范非负十进制位置；赋值可替换现有元素或在
`length` 位置追加，不允许空洞。Object 键使用受控标量字符串，并保留属性顺序。

## 读写上下文

Evaluator 本身不拥有 State。`EvaluationContext` 提供变量、`setup`、全局名称、函数与
写入授权。纯字面量和运算可在无 State 上下文中求值；变量、赋值和更新必须由
Runtime 注入所有者。

可写目标先解析根和完整路径，再求右值并一次提交根。路径中的索引只求值一次。
转换、参数、路径或授权失败时不得留下部分修改。

对共享 Array/Object 的修改还需要引用写入授权。这个检查使别名可见性与 Runtime
事务保持一致。Array 元素不支持 `delete`，以维持稠密序列不变量。

## 运算与比较

- 严格相等不转换类型；非严格相等只执行受控 Web 标量转换，并保留 `null == undefined`。
- Array、Object 和 Callable 按 Runtime 身份比较，不用 Rust 结构相等代替引用语义。
- `&&`、`||`、`??` 和三目运算只求被选中的分支。
- `in`/`notin` 检查 Array 元素、Object 自有键或 String 子串，不使用 JavaScript 原型链语义。
- `typeof` 返回 Narrava Value 类别；Array、Object 和 `null` 不沿用 JavaScript 的历史分类。

算术、位运算、移位和关系比较的数值转换是引擎语义，不委托给宿主 JavaScript。

## 成员、调用与可选链

成员访问只解析 Narrava 登记的属性和方法。未知成员、非法目标和不可调用值会报错。
函数和方法在执行前校验参数数量、类型和写入授权。

`target?.member`、`target?.[index]` 和 `callee?.()` 只在目标为 `null` 或 `undefined` 时
短路。短路状态穿过同一链，且不求值索引或参数；括号会结束传播。可选链不得
吞掉非空目标上的真实错误。

## 原型与安全边界

`Array`、`String`、`Number`、`Boolean` 和 `Function` 具有只读内置原型身份，其父原型
为 `Object`。`instanceof` 只接受登记原型名称。

Expression 不暴露 `__proto__`、`constructor`、`prototype`、`Object.setPrototypeOf()`
或宿主原型。未登记全局和成员不会回退到 JavaScript 环境。

## 不支持的语法

`delete` 只用于受支持的绑定和 Object 成员。`void`、`new`、逗号运算、`await`、
`yield`、计算属性字面量、展开项和管道运算符不属于当前 Expression API。

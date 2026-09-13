# Twee Expression 参考

Expression 在 Core 中求值，适用于 Twee Macro 参数；不能直接套用全部 JavaScript API。
脚本环境见[脚本 API](script-api.md)，求值实现见[编译与表达式](../architecture/compiler.md)。

## 内置函数

| 函数 |   参数 | 结果 |
| ----------------------------------- | -----: | ------------------------------------------------------------ |
| `abs(value)` |      1 | 绝对值 |
| `boolean(value)` |      1 | 转换为布尔值 |
| `ceil(value)` |      1 | 向上取整 |
| `clamp(value, min, max)` |      3 | 限制数值范围 |
| `clone(value)` |      1 | 深拷贝值图，断开原 Array/Object 引用并保留拷贝内部共享和循环 |
| `defined(value)` |      1 | 是否不是 `undefined` |
| `empty(value)` |      1 | 字符串、Array 或 Object 是否为空 |
| `entries(value)` |      1 | Array/Object 的键值对 |
| `either(...values)` | 至少 1 | 随机返回一个参数 |
| `floor(value)` |      1 | 向下取整 |
| `keys(value)` |      1 | Array/Object 的键 |
| `max(...values)` | 至少 1 | 最大数值 |
| `min(...values)` | 至少 1 | 最小数值 |
| `number(value)` |      1 | 转换为数值 |
| `random()` |      0 | `[0, 1)` 随机数 |
| `round(value)` |      1 | Web 语义四舍五入 |
| `string(value)` |      1 | 转换为字符串 |
| `values(value)` |      1 | Array/Object 的值 |
| `Object.assign(target, ...sources)` | 至少 1 | 按顺序写入 Object |
| `Object.hasOwn(target, key)` |      2 | 是否有自身属性 |

## 集合与字符串

- Array 属性：`length`
- Array 只读方法：`at(index)`、`concat(...values)`、`includes(value)`、
  `indexOf(value, fromIndex?)`、`join(separator?)`、`slice(start?, end?)`
- Array 可写方法：`pop()`、`push(...values)`、`shift()`、`splice(...)`、
  `unshift(...values)`
- String 属性：`length`，按 UTF-16 码元计数
- String 方法：`includes(text)`、`slice(start?, end?)`、`split(separator?, limit?)`、
  `startsWith(text)`、`endsWith(text)`、`trim()`、`toLowerCase()`、`toUpperCase()`

可写 Array 方法只有在当前 Expression 上下文允许写入时才能执行。

## 值与运算符

- 常量：`true`、`false`、`null`、`undefined`
- 数据：Number、String、Array、Object
- 变量：`$name` 持久游戏变量、`_name` 临时变量、`@name` Macro 局部变量
- 读取：成员 `object.name`、索引 `value[index]`、调用 `function(...)`
- 可选链：`?.`、可选索引和可选调用
- 算术：`+ - * / // % **`
- 位运算：`& | ^ << >> >>>`
- 比较：`< <= > >= == != === !== <=>`；别名为 `lt`、`lte`、`gt`、`gte`、`equ`、
  `is`、`isnot`
- 逻辑与空值：`&& || ??`，以及 `and`、`or`、`not`
- 成员判断：`in`、`notin`、`instanceof`、`between`
- 条件：`condition ? yes : no`
- 赋值：`=` 以及算术、位、逻辑、空值复合赋值；支持前置/后置 `++`、`--`

## 求值边界

赋值、幂运算和三目运算右结合；`??` 与 `&&` / `||` 混用必须显式分组。
`in` / `notin` 检查数组元素、对象自有键或字符串子串，不查询 JavaScript 原型链。
集合按引用身份比较，`clone(value)` 深复制值图并保留拷贝内部的共享与循环。
数组为稠密序列，不能删除数组元素或写入空洞；字符串长度按 UTF-16 码元计数。

`?.` 只对 null/undefined 短路，非空目标的访问错误仍会返回；括号结束同一链的短路传播。
可写操作受当前执行上下文授权，参数或目标失败不留下部分修改。

`random()` 与 `either(...)` 使用 Engine 游戏序列，规则见[根种子](../author/flow.md#根种子)。
`void`、`new`、逗号运算、`await`、`yield`、展开项和计算属性字面量不属于 Expression。

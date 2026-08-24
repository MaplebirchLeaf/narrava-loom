# Narrava Expression

> 状态：基础结构实现中
>
> 更新日期：2026-08-21

## 职责

Expression 独立负责 Macro 参数中的表达式词法、优先级、AST 和求值语义。语法方向接近 TypeScript/JavaScript，以适配最终的 Web 运行环境；`for`、`while` 等 Macro 不自行解析运算符。

接近 TS/JS 不等于直接执行 JavaScript 源码。Narrava 仍生成自己的 Expression AST，以保持诊断、编译和运行行为可控。

Expression 可以在执行时通过受控上下文访问 State，但不拥有 State，也不负责 Macro 注册。`.twee` 表达式的普通全局名称在运行时从 `State.global` 沙箱解析；`setup` 是 `State.setup` 的明确入口。引擎保留的内置函数仍由受控 Expression 函数表解析，不穿透浏览器全局对象。

只读 `EvaluationContext` 边界已经建立，提供 `global(name)`、`setup()` 与 `variable(scope, name)` 三类借用读取。`evaluate_with()` 会把同一 Context 贯穿完整子表达式；原有 `evaluate()` 使用空 Context，继续服务纯表达式。求值结果按当前拥有型 Value 模型克隆借用值，Context 本身不被 Expression 持有。内置函数名优先于外部全局；未知普通名称返回 `UnknownGlobal`，不会回退到浏览器全局对象。

`setup` 缺失返回 `MissingSetup`，因为它是必须由运行时建立的明确根对象。`$`、`_`、`@` 缺失则求值为 `undefined`，从而允许 `defined($missing)` 安全检查变量存在性。三种变量仍通过 `VariableScope` 分开请求：Context 可以分别映射到 `State.variables`、`State.temporary` 与当前 Macro 局部域，Expression 不合并或拥有这些存储。

Macro Runtime 已通过只读 `MacroEvaluationContext` 提供当前调用的 `@args` 数组，因此普通索引表达式可直接求值 `@args[0]`。Expression 只读取组合后的 Context，不知道调用帧结构，也不让 State 保存 Macro 局部数据。

公共 `parse_list()` 使用顶层空白解析零个或多个 Expression，供声明采用标准实参列表的 Macro 定义复用。括号、数组、对象与调用内部的空白不会分隔实参；Macro 实参之间不使用逗号或分号。包含空白的复合 Expression 应使用括号明确边界，例如 `"Maple" ($count + 1) { active: true }`。

规划中的反引号 Template String 与 JavaScript 采用相同的显式插值边界：`` `$name` `` 是普通文本，`` `${$name}` `` 才读取变量。Template String 仍属于 Narrava Expression AST，不会交给 JavaScript `eval`。单双引号字符串继续保持普通字符串语义，不执行 `${...}`。

Expression 自身允许顶层位移运算；但嵌入 `<<...>>` Macro Header 时，三种位移及其复合赋值必须位于圆括号内，避免 `>>` 与 Macro 结束符冲突。该限制由 Macro 参数边界检查，不改变 Expression AST 的一般语法。

可写能力通过独立 `WritableEvaluationContext: EvaluationContext` 扩展，并由 `evaluate_with_mut()` 显式启用；普通 `evaluate()` 与 `evaluate_with()` 不会隐藏写入。裸全局和 `$`、`_`、`@` 已支持直接 `=`、全部复合赋值与更新。算术、字符串拼接和位运算复合赋值与普通 Binary 共用同一 Value 运算入口；`&&=`、`||=`、`??=` 先读取目标一次，短路时不求值右侧也不调用写入。成功写入后返回新值，短路则返回原值。

Object 点成员目标已使用根路径写回：路径必须源自裸全局或三类变量，根 Value 只从 Context 克隆一次；嵌套属性在副本内读取、修改后只提交一次根。直接 `=` 可新增末级自身属性，中间属性必须已经存在且为 Object；复合赋值和更新要求末级属性也存在。临时字面量没有可提交根，因此不能作为写入路径。`setup` 根仍单独接入。

动态 Object 键与 Array 元素索引已进入同一根路径模型。索引表达式在右值之前按路径顺序求值一次；Object 使用受控标量字符串键，Array 只接受规范非负十进制索引。当前 Array 是稠密 `Vec`：直接赋值可替换现有元素或在 `length` 位置追加，不允许跨空洞跳写；负数、小数、前导零字符串和间隔位置返回 `InvalidArrayIndex`。Object、Array 可以在嵌套路径中交替出现。

`setup` 已作为独立 `AssignmentRoot` 接入，并通过 `WritableEvaluationContext::set_setup()` 写回，不借道 global 或变量表。`setup.member` 与 `setup[index]` 支持直接赋值、复合赋值和更新；只读 Context 返回根位置的 `MissingWriteContext`。Parser 仍禁止 `setup = value`，因此 setup 根本身不能整体替换。

Evaluator 在进入引用身份迁移前开始按职责收束。`expression/evaluator/target.rs` 现在完整负责可写目标的路径解析、根读取、成员或索引读取与修改，以及一次性根提交；主 Evaluator 只保留赋值、复合赋值和更新运算的求值顺序。后续引用句柄会沿这个目标边界接入，避免继续扩大单文件职责。

Evaluator 的第二轮职责拆分已形成四个边界。`expression/evaluator/native_functions.rs` 负责内置函数的参数范围、分派与具体求值，包括 Object 静态函数、随机、转换、数值和集合函数；`expression/evaluator/native_methods.rs` 负责 Array/String 原生方法，包括修改授权；`expression/evaluator/operations.rs` 负责单元、二元、算术、区间、成员、关系与相等运算；`expression/evaluator/conversion.rs` 负责 Web 标量、32 位整数与字符串字面量解码。主 `evaluator.rs` 只保留 AST 求值、链访问和 callable 编排，并从拆分前的 2130 行降到 769 行。

内部 `ValueReference<T>` 已建立引用身份的最小契约：克隆句柄不会深复制集合，`same_identity()` 使用句柄身份判断，并且任一克隆的修改可由其他克隆观察。Array 与 Object 分别通过公开的受控包装 `ArrayValue`、`ObjectValue` 接入，统一由 `Value::array()`、`Value::object()` 构造。两类引用值的别名共享成员修改，严格与同类型非严格比较使用引用身份；Array 的 `includes()`、`indexOf()` 与 `in` 也按相同身份规则搜索。两个内容相同但独立创建的集合仍不相等。

共享引用的路径写入不会绕过 Context 权限。`AssignmentPath::commit_value()` 先无副作用验证完整成员或索引路径，再提交一次根以取得 Context 接受，最后才修改共享集合。只读 Context 返回 `MissingWriteContext`，主动拒绝写入的 Context 返回 `ContextWriteRejected`；两种失败都不会在 Array 或 Object 别名中残留提前修改。

变量前缀同时决定解析结果和运行时所有者：`$name` 访问 `State.variables`，`_name` 访问 `State.temporary`，`@name` 访问当前 Macro 调用上下文。Expression 不把三者合并为一个变量表。

`scripts` 若要让值或函数在 Twee 中成为全局可见内容，必须通过 State API 登记到对应命名空间。`export` 与 `import` 不承担向 Twee 或 State 注入名称的职责。

## 当前实现

- Expression 已按职责拆分：`expression.rs` 是薄公开入口，`expression/ast.rs` 保存 AST，`expression/lexer.rs` 保存词法逻辑，`expression/parser.rs` 保存解析错误、优先级和 AST 构建逻辑；公开类型、`lex()` 与 `parse()` 仍由原入口重导出。
- Lexer 为所有 Token 保留 UTF-8 字节 Span；空白会被明确跳过，未知字符及缺少名称的变量前缀会直接报错，不允许静默丢失源码。
- 数字和字符串 Token 保留源码原文；字符串支持单双引号和反斜杠保护，具体值由 Evaluator 转换。
- Parser 已支持基础值、`State.global` 名称、`setup`、三种变量、分组、数组和对象。
- 后缀链已支持普通及可选的成员、索引与调用，并为可选后缀保留独立 AST 节点。
- `is_assignable_target()` 已统一判断赋值、自增和自减的合法目标。
- Parser 已支持 `!`/`not`、`~`、一元 `+`、一元 `-` 与 `typeof`，并将别名归一化为 `UnaryOperator`。
- Lexer 与 Parser 已支持前缀、后缀 `++`、`--`；AST 保留增减类型和位置，并在解析时验证可赋值目标。
- Parser 已支持右结合 `**`，并建立后续二元运算共用的 `Binary` AST。
- Parser 已支持同级左结合的 `*`、`/`、`//`、`%`，并保持 `**` 更高优先级。
- Parser 与 Evaluator 已支持同级左结合的二元 `+`、`-`，并保持乘法层更高优先级；`+` 在任一操作数为 String 时执行受控标量拼接，否则执行 Number 加法。
- Lexer 与 Parser 已支持同级左结合的 `<<`、`>>`、`>>>`，并按最长 Token 优先识别。
- Parser 已支持大小比较、`lt`/`lte`/`gt`/`gte` 别名、`<=>`、`instanceof`、`in`、`notin` 与四种 `between`；连续比较左结合。
- Parser 已支持 `==`、`!=`、`===`、`!==`，并将 `equ`、`is`、`isnot` 归一化为对应相等运算；相等层低于比较层。
- Parser 已支持左结合的 `&`、`^`、`|` 三层按位运算，优先级依次降低，并整体低于相等层。
- Parser 已支持左结合的 `&&`、`and`、`||`、`or`；英文别名归一化，逻辑与高于逻辑或，短路由求值器实现。
- Parser 已支持左结合的 `??`，并拒绝它与 `&&`、`||` 的无括号混用；分组会明确隔开检查边界。
- Parser 已支持右结合的 `condition ? value : fallback`，并保留独立 `Conditional` AST；运行时只求值被选择的分支。
- Parser 已支持规划中的全部右结合赋值运算，包括 `&&=`、`||=`、`??=`；统一使用独立 `Assignment` AST 和可写目标检查，短路写入语义留给求值器。
- 独立 Value 子模块已建立，首轮标量包含 `Undefined`、`Null`、`Boolean`、`Number(f64)` 与 `String`，并提供空值判断。
- 独立 Evaluator 子模块已建立；不依赖 State 的字面量、运算、成员、索引、调用和首批内置函数可直接求值。变量、赋值与更新等需要运行时上下文的 AST 仍返回带 Span 的 `UnsupportedExpression`。
- AST 节点保留整体 Span；属性键、成员名、数组元素、对象值、索引和参数保留各自位置。
- 当前不支持尾随逗号、展开项、对象简写或计算属性；结构缺失通过对应的 `ParseError` 报告。

## 基础值与字面量

首轮支持字符串、数字、布尔值、`null`、`undefined`、数组和对象字面量。数组项与对象值都是完整 Expression，例如 `[@aa, _bb]` 与 `{ name: @name, score: $score }`。数组项使用逗号分隔，当前不接受尾随逗号。对象键首轮只允许裸标识符或字符串；计算属性、展开项和简写属性暂不加入。

## 首轮运算符

优先级从高到低：

1. 分组：`(...)`
2. 成员、索引、调用和可选链：`.`、`[...]`、`(...)`、`?.`、`?.[...]`、`?.(...)`
3. 后缀自增、自减：`++`、`--`
4. 一元运算：`!`、`not`、`~`、`typeof`、一元 `+`、一元 `-`、前缀 `++`、前缀 `--`
5. 幂运算：`**`
6. 乘除与取模：`*`、`/`、`//`、`%`
7. 加减：`+`、`-`
8. 位移：`<<`、`>>`、`>>>`
9. 比较：`<`、`<=`、`>`、`>=`、`<=>`、`lt`、`lte`、`gt`、`gte`、`instanceof`、`in`、`notin`、四种 `between`
10. 相等：`==`、`!=`、`===`、`!==`、`is`、`equ`、`isnot`
11. 按位与：`&`
12. 按位异或：`^`
13. 按位或：`|`
14. 逻辑与：`&&`、`and`
15. 逻辑或：`||`、`or`
16. 空值合并：`??`
17. 条件运算：`condition ? value : fallback`
18. 赋值及复合赋值：`=`、`+=`、`-=`、`*=`、`/=`、`//=`、`%=`、`**=`、`<<=`、`>>=`、`>>>=`、`&=`、`^=`、`|=`、`&&=`、`||=`、`??=`

`gt`、`gte`、`lt`、`lte` 分别归一化为 `>`、`>=`、`<`、`<=`。`===` 与 `!==` 表示严格相等和严格不等。`is` 归一化为 `===`，`isnot` 归一化为 `!==`；`equ` 归一化为非严格相等 `==`。`not` 归一化为逻辑非 `!`，`and` 与 `or` 分别归一化为 `&&` 与 `||`。`==` 与 `!=` 的类型转换规则在实现求值器前单独确定。Expression AST 只保留规范化后的运算符。

不支持双词形式 `is not`，避免与一元逻辑非 `not` 产生词法歧义。

`instanceof`、`in` 与 `notin` 只在比较运算符位置具有特殊含义，其他位置仍可作为属性名等普通标识符。`notin` 保留独立 AST 运算符，求值时只读取左右操作数一次。

`in` 与 `notin` 使用 Narrava 成员语义，不采用 JavaScript 的属性索引语义：数组按严格相等检查元素，对象只检查自身键，字符串检查子串。`notin` 只反转结果。数组元素若为 Array 或 Object，同样按引用身份比较；受控原型属性不参与首轮成员判断。

逻辑非只使用 `!` 与 `not`。`~` 只表示按位取反，不能作为逻辑非。

`**`、条件运算和赋值采用右结合。自增、自减和所有赋值运算的左侧必须是可赋值目标。Expression 通过执行上下文修改目标，不拥有 State。

可赋值目标包括 `State.global` 名称、`$`、`_`、`@` 变量、普通成员和普通索引；分组继承内部目标。`setup` 根本身、字面量、调用以及包含可选链的目标不可赋值。`setup.name` 属于普通成员，因此可赋值。

前缀 `++value`、`--value` 返回更新后的值；后缀 `value++`、`value--` 返回更新前的数值化结果。两种形式都会读取目标一次并写回一次。Evaluator 已支持裸全局与 `$`、`_`、`@` 目标，并复用 Web 标量数值转换；集合返回目标位置的 `InvalidNumericConversion`。

`??` 不允许在没有括号时直接与 `&&` 或 `||` 混用，与 TypeScript/JavaScript 保持一致。

可选链在目标为 `null` 或 `undefined` 时短路并返回 `undefined`，不继续执行后续成员、索引或调用。表达式中的 `in` 由 Expression Parser 处理；`<<for value in expression>>` 中的 `in` 由 `for` Macro 参数结构处理，两者不共用解析入口。

`//` 表示整除，`//=` 表示整除后写回。正数行为是舍去小数部分；负数结果究竟向零截断还是向负无穷取整，在实现求值器前用独立测试确定，当前规划不提前锁定。

`notin` 与 `in` 使用相同优先级，表示对成员判断结果取反。Parser 可以先生成独立的 `NotIn` AST，再在语义阶段归一化，避免重复求值左右两侧。

区间判断使用日常数学区间边界，不引入 `..` 或 `..=`：

```text
value between() lower upper  // lower <  value <  upper
value between(] lower upper  // lower <  value <= upper
value between[) lower upper  // lower <= value <  upper
value between[] lower upper  // lower <= value <= upper
```

`value`、`lower` 与 `upper` 都是 Expression，并在独立的 `Between` AST 中各保存一次。四种 `between` 与其他比较运算同级。比较运算允许连续书写，并按左结合生成 AST。

`left <=> right` 是三向比较：小于返回 `-1`，相等返回 `0`，大于返回 `1`。它与其他比较运算同级；跨类型比较和 `NaN` 等异常值的行为在实现求值器前单独确定。

三目条件运算 `condition ? value : fallback` 保留。管道运算符 `|>` 暂不加入，等出现明确运行场景后再决定。

## 首轮明确排除

`delete`、`void`、`new`、逗号运算、`await` 与 `yield` 暂不加入。它们依赖对象生命周期、异步或生成器语义，不是首轮叙事表达式的必要能力。

## 内置函数

Expression 首轮提供小型标准函数集合，不通过 Macro 重复实现：

- 集合：`keys()`、`values()`、`entries()`；
- 数值：`min()`、`max()`、`clamp()`、`abs()`、`floor()`、`ceil()`、`round()`；
- 类型转换：`number()`、`string()`、`boolean()`；
- 判断：`defined()`、`empty()`；
- 随机：`random()`、`either()`。

函数由 Expression 执行上下文解析，不直接调用浏览器全局函数。随机函数使用 Runtime 提供的随机源，使存档、回放和测试能够控制结果。各函数的参数数量、空集合和类型错误规则在实现标准函数表时逐项确定。

随机状态通过独立可变 `RandomSource::next_unit()` 注入，不进入只读 `EvaluationContext`。`evaluate_with_random()` 同时借用 State 查询 Context 与随机源；`random()` 不接参数并返回 `[0, 1)` 的单位值，`either(first, ...rest)` 至少接收一个值并按单位值选择。随机源缺失返回 `MissingRandomSource`；NaN、无穷或不在 `[0, 1)` 的结果返回 `InvalidRandomValue`。因此 Runtime 可以用同一随机序列实现存档、回放和确定性测试。

内置函数使用独立 `NativeFunction` 身份，与绑定接收者的 `NativeMethod` 共用 callable 外壳，但不会伪造方法接收者。`defined(value)` 只有在参数为 `undefined` 时返回 `false`，包括 `null` 在内的其他值都返回 `true`。`empty(value)` 对 `undefined`、`null`、空字符串、空数组和空对象返回 `true`；数字、布尔值、函数与非空集合返回 `false`，因此它不等同于真假值转换。两者都严格接收一个参数，并具有正常的 Function callable 身份。

`keys(collection)`、`values(collection)`、`entries(collection)` 已支持 Object 与 Array，并严格接收一个参数。Object 保持属性声明顺序；Array 的键为从零开始的十进制字符串。`entries()` 的每项是 `[key, value]` 二元数组。首轮不对 String 或标量执行宿主式装箱，非法目标返回参数位置上的 `InvalidCollectionTarget`。

受控 `Object` 内置命名空间已作为 `Value::Namespace` 建立，不是 AST 特例，也不映射到宿主 JavaScript `Object`。`Object.hasOwn(target, key)` 严格接收 Narrava Object 与一个可受控转换的标量键，只检查自身有序属性；Array 不冒充 Object，原型成员也不参与。点访问与 `Object["hasOwn"]` 共用命名空间成员表，未知方括号成员返回 `undefined`。

`abs(number)`、`floor(number)`、`ceil(number)`、`round(number)` 已实现并严格接收一个 Number，不隐式转换 String 或 Boolean。`round()` 使用 Web 规则：半数向正无穷取整，所以 `round(-1.5)` 为 `-1`；`-0.5` 至 `0` 的负数结果保留负零。非法类型返回参数位置上的 `InvalidNumericArgument`。

`min(...numbers)` 与 `max(...numbers)` 至少接收一个 Number，保留 NaN 传播以及正负零的 Web 选择规则。`clamp(value, lower, upper)` 固定接收三个 Number并使用闭区间；下界大于上界时返回覆盖两个边界参数的 `InvalidRange`，不自动交换边界。

`number(value)`、`string(value)`、`boolean(value)` 已复用 Expression 的受控标量转换。`number()` 支持空值、Boolean、Number 与 String，包括进制前缀和 NaN 结果；Array、Object、Function 返回 `InvalidNumericConversion`。`string()` 支持标量并拒绝集合与函数，不调用宿主 `toString()`。`boolean()` 使用 Narrava 真假值规则，空数组与空对象仍为真。三者都严格接收一个参数。

首轮不再提供重复的 `Math` 全局对象。数值能力统一使用上述内置函数；类型原型只承载与具体值相关的属性和方法，例如 `.length`、`.includes()`、`.trim()`，避免 `abs()` 与 `Math.abs()` 两套入口并存。

## 受控原型链

Narrava Value 使用引擎自己的只读原型表，使成员调用和 `instanceof` 不依赖浏览器 JavaScript 原型：

```text
Array   → Object
String  → Object
Number  → Object
Boolean → Object
Function → Object
```

集合长度使用 `.length`，不再重复提供 `len()`。通用能力保留为函数，类型能力放在原型属性或方法上，例如 `@items.includes(_target)`、`@text.trim()` 和 `@items.length`。

已确认的首轮类型能力如下：

- `Object.assign()`、`Object.hasOwn()`；
- Array：`.at()`、`.includes()`、`.indexOf()`、`.slice()`、`.concat()`、`.join()`、`.push()`、`.pop()`、`.shift()`、`.unshift()`、`.splice()`；
- String：`.includes()`、`.startsWith()`、`.endsWith()`、`.trim()`、`.slice()`、`.split()`、`.toLowerCase()`、`.toUpperCase()`；
- Array 与 String：只读 `.length`。

不加入 `Array.isArray()`，因为 Narrava 的 `typeof` 已将数组明确返回为 `array`。`keys()`、`values()`、`entries()` 继续使用全局内置函数，不重复提供 `Object.keys()`、`Object.values()`、`Object.entries()`。修改型原生函数统一要求显式可写求值入口，并在触碰共享引用前调用 `WritableEvaluationContext::authorize_reference_write()`；默认实现拒绝，Runtime 必须主动授权。

`Object.assign(target, ...sources)` 已作为第一个修改型原生函数完成。目标与每个来源首轮都必须是 Narrava Object，不执行宿主装箱；来源按参数顺序处理，已有键在原位置覆盖，新键按出现顺序追加，返回值保持目标的同一引用。无目标、非法目标或来源、只读 Context 与拒绝授权均有独立错误测试，失败时不会留下部分修改。

Array `.push(...values)` 已接入同一修改授权。它接受零个或多个任意 Narrava Value，按参数顺序追加到共享 Array，并返回追加后的 Number 长度；零参数不改变内容，只返回当前长度。通过别名调用或读取时观察到同一修改，只读与拒绝授权均不会留下追加结果。原生方法调用边界现在携带 `EvaluationSession`，但只有登记为修改型的方法会请求引用写入授权。

Array `.pop()` 已完成。它不接受参数，取得引用写入授权后移除并返回共享 Array 的末项；空数组返回 `undefined`。返回的 Array 或 Object 元素保留原引用身份，所有数组别名同步观察长度变化；参数错误、只读 Context 和拒绝授权均发生在修改之前。

Array `.shift()` 已完成。它不接受参数，取得引用写入授权后移除并返回共享 Array 的首项；空数组返回 `undefined`。非空数组使用稠密 Vec 的首项移除语义，后续元素依次前移；返回值引用身份、别名可见性和失败前不修改规则与 `.pop()` 一致。

Array `.unshift(...values)` 已完成。它接受零个或多个任意 Narrava Value，取得引用写入授权后按参数原顺序插入共享 Array 开头，并返回插入后的 Number 长度；零参数不改变内容，只返回当前长度。别名可见性与授权失败不修改规则和 `.push()` 对称。

Array `.splice(start, deleteCount?, ...values)` 已完成。零参数不删除；只给起点时删除到末尾；起点支持负数和 Web 标量数值转换，删除数量限制在零到剩余长度之间，后续值按参数顺序插入。它在完整转换和授权通过后一次性替换共享 Array 区间，并返回包含被删除值的新 Array，因此数值错误或授权失败不会留下部分修改。

方法按运行时依赖分三层推进：

1. 无副作用且不需要回调：Array 的 `.at()`、`.includes()`、`.indexOf()`、`.slice()`、`.concat()`、`.join()`，String 的查找、裁剪、大小写转换与 `.split()`，以及 `Object.hasOwn()`（已完成）；
2. 已完成引用身份和写入授权：`Object.assign()`、Array 的 `.push()`、`.pop()`、`.shift()`、`.unshift()`、`.splice()`；
3. 需要稳定调用帧和回调：`.find()`、`.findIndex()`、`.every()`、`.some()`、`.map()`、`.filter()`、`.reduce()`。

不同时提供语义重复的方法：String 首轮只保留 `.slice()`，不再加入 `.substring()`；排序、正则替换和区域设置比较分别等待比较器回调、RegExp 与 I18n 边界。这样方法表保持小而可预测。

Expression 不暴露 `__proto__`、`constructor`、`prototype` 或 `Object.setPrototypeOf()`，也不能穿透到宿主 JavaScript 原型。内置原型由引擎登记；脚本后续通过独立的受控 `Prototype` API 添加成员，而不是直接修改浏览器的 `Array.prototype`、`String.prototype` 等宿主对象。原型 Registry 独立于 State，State 只保存脚本显式导入的值。

只读内置原型身份已建立：`Array`、`String`、`Number`、`Boolean`、`Function` 的父原型均为 `Object`，`Object` 没有父原型；`null` 与 `undefined` 没有原型。Evaluator 已支持这些身份的 `instanceof`，右侧必须是已登记的原型名称，未知或动态右值返回 `InvalidPrototype`。NativeCallable 已属于 `Function`，因此内置方法同时满足 `instanceof Function` 与 `instanceof Object`。Narrava 将标量也视为其类型原型的实例，不沿用 JavaScript 原始值与装箱对象分离的历史行为。

Evaluator 已支持 Array 与 String 的只读 `.length` 和 `.includes()`、Array 的 `.at()`、`.slice()`、`.indexOf()`、`.concat()`、`.join()`，以及 String 的 `.startsWith()`、`.endsWith()`、`.slice()`、`.split()`、`.trim()`、`.toLowerCase()`、`.toUpperCase()`。数组 `.includes()` 使用严格相等，不进行隐式类型转换，Array 与 Object 搜索值按引用身份比较；`.at()` 使用 Web 标量数值转换和向零截断，支持负索引，`NaN` 视为零，越界和无穷索引返回 `undefined`。Array `.slice()` 接受零至两个边界，支持省略结束位置、负边界和 Web 标量数值转换，并返回新数组。`.indexOf()` 使用同一严格比较，接受可选起点并返回索引或 `-1`。`.concat()` 接受任意数量参数，数组参数只展开一层，其他值作为单个元素追加，并返回新数组。`.join()` 接受可选分隔符；省略或传入 `undefined` 时使用逗号，空值元素输出空文本，嵌套数组以逗号递归连接，对象和函数不执行宿主字符串转换。三种字符串查找均接受一个 String，并执行 UTF-16 码元查找。String `.slice()` 与 `.length` 共用 UTF-16 单位，支持产生并保留孤立代理项。`.split()` 不接受 RegExp；省略或传入 `undefined` 分隔符时返回原字符串数组，空分隔符按 UTF-16 码元拆分，可选 limit 使用 Web `ToUint32`。`.trim()` 不接受参数，并使用 Web 的 WhiteSpace 与 LineTerminator 集合，而不是 Rust 更宽的 Unicode 空白集合。大小写转换使用不依赖区域设置的 Unicode 默认映射，并返回新字符串。每个原生方法自行声明最少和最多参数数量；参数数量、参数类型和不可调用值均保留明确错误位置。成员读取会产生绑定接收者的原生可调用值，`typeof` 返回 `function`。其他尚未登记的成员返回指向成员名称的 `UnknownMember`。

Expression、Macro、State 与 Story 的基础链已经收束。`ScriptCallable` 现以稳定句柄进入 Value，`typeof` 为 `function`，并由 `ScriptRuntimeContext` 把调用交还 Binding；求值器仍不拥有 State 或真实 JavaScript Function。宿主脚本原型扩展尚未开放，不能穿透到 JavaScript 原型链。

String 的内部表示已迁移为 `TextValue` 持有的 UTF-16 码元序列。Rust `String` 无法表达 Web 切片可能产生的孤立高、低代理项，因此不再作为完整运行时字符串表示。字符串长度、严格比较、排序、查找、拼接、trim、大小写转换、数组 join 和标量数值转换均已改为经过 `TextValue`；孤立代理项可继续参与码元比较和拼接，但不能被误转成 Rust Unicode 字符串。

Evaluator 已支持普通索引读取。Array 与 String 只把规范的非负十进制属性名视为元素索引，负数、小数、前导零字符串和越界位置返回 `undefined`；String 单项结果保留一个 UTF-16 码元。Object 把可受控转换的标量键转为字符串并只读取自身属性。Array 与 String 的方括号成员名复用受控原型成员表，例如 `items["length"]`；未知方括号成员返回 `undefined`。不可索引目标返回 `InvalidIndexTarget`。

Object 已支持点号读取自身属性，包括连续读取嵌套对象。普通点访问找不到属性时返回带成员位置的 `UnknownMember`；这与方括号动态查询缺失键时返回 `undefined` 的规则不同。Object 原型成员仍等待受控 Prototype Registry，不会回退到宿主 JavaScript 原型。

`Value::detached_clone()` 已为 State 快照提供对象图级复制：新 Array/Object 与原图身份分离，图内部的共享引用和循环关系继续保留；绑定原生方法的接收者也通过同一图复制。`detached_clone_many()` 让多个根值共用同一图映射，因此不同 `$` 变量之间的别名关系不会丢失。普通 `Value::clone()` 仍保留运行时引用身份，不能冒充快照。

直接可选成员 `target?.member`、可选索引 `target?.[index]` 与可选调用 `callee?.()` 已进入 Evaluator。目标为 `null` 或 `undefined` 时返回 `undefined`，短路时不会求值索引或调用参数。短路状态会继续穿过同一条成员、索引和调用链，例如 `null?.profile().name`，并与属性值本身恰好为 `undefined` 的情况分开保存。括号会结束短路传播，因此 `(null?.profile).name` 仍按普通成员读取报错。非空目标不会吞掉未知成员、非法目标或不可调用值等真实错误。

## 实现顺序

1. Expression Token、Span 与三种变量（已完成）；
2. 字符串、数值、布尔值、`null`、`undefined`、数组、对象、State 名称、变量引用与括号（已完成）；
3. 成员访问、索引、调用、可选链与可赋值目标（已完成）；
4. 一元运算、`typeof`、自增、自减与幂运算（名称和变量目标已完成）；
5. 算术、整除、三种位移、三向比较、其他比较、`instanceof`、`in`、`notin`、`between`、相等与按位运算（已完成）；
6. 逻辑运算、空值合并、条件运算与英文别名（已完成）；
7. 赋值及复合赋值（Parser 与 Evaluator 已完成，包括 global、setup、三类变量及成员索引路径）；
8. 求值接口、内置函数、受控原型与 State 访问边界。

每一步都需要优先级和错误位置测试。变量引用的具体书写形式在实现对应步骤前单独确定。

Parser 首轮覆盖审查已完成。求值代码不继续堆入已较大的 `expression.rs`；下一阶段从独立的 Expression Value 子模块开始拆分职责。

原 `expression.rs` 已完成职责拆分：AST、Lexer、Parser 和测试均已迁出。Evaluator 也已拆成主调度、目标、原生函数、原生方法、运算和转换模块。项目现有测试全部集中在 `src/tests/`；需要访问私有实现的测试仍由对应生产模块通过 `#[path]` 挂载，因此不扩大公开 API。

Value 中的 `Null` 与 `Undefined` 必须保持独立；二者只在 `is_nullish()` 等明确语义中归为一类。`Number` 首轮使用 `f64` 对齐 Web `number`，`NaN` 与无穷值规则在数值求值阶段固定。

Value 已加入拥有自身数据的数组与对象。对象以有序属性列表保存，保持源码声明顺序；重复键遵循 Web 对象字面量语义，由后出现的值覆盖先前值，但不移动键原有位置。Evaluator 可以递归求值不依赖运行时上下文的复合字面量，内部变量仍返回变量自身的 `UnsupportedExpression` 位置。

Value 的真假值已按 Web 规则固定：`undefined`、`null`、`false`、正负零、`NaN` 与空字符串是假值，数组和对象始终是真值。Evaluator 已实现 `!`、`not`、一元 `+`、一元 `-` 与 `~`；标量数值转换支持十进制、`Infinity` 以及字符串中的 `0x`、`0o`、`0b` 前缀，按位运算使用 Web 的 32 位整数折返规则。数组和对象的隐式数值转换暂时报 `InvalidNumericConversion`，等待受控原型规则后再开放。

`typeof` 已覆盖当前 Value：`undefined`、`null`、`boolean`、`number`、`string`、`array`、`object` 与 `function`。Narrava 明确区分 `null`、数组和普通对象，不继承 JavaScript 将三者混入 `object` 的历史兼容结果。未知裸全局仍是错误；缺失 `$`、`_`、`@` 变量由显式 EvaluationContext 返回 `undefined`，具体存储归 State 与 Macro 上下文所有。

Evaluator 已实现 `+`、`-`、`*`、`/`、`//`、`%` 与 `**`。`+` 在任一标量操作数是字符串时执行字符串拼接，否则执行数值加法；`//` 把商向零截断。除零、`NaN` 与无穷值沿用 Web `number` 结果。数组和对象不会在算术中隐式调用原型转换，而是按所需目标类型返回 `InvalidNumericConversion` 或 `InvalidStringConversion`，并指向具体操作数。

Evaluator 已实现 `&`、`|`、`^`、`<<`、`>>` 与 `>>>`。普通按位运算及有符号移位先把两侧折返为 32 位整数；移位数量只使用低 5 位，因此等价于对 32 取模。`>>>` 把左侧视为无符号 32 位整数，并以非负 `Number` 返回结果。

Evaluator 已实现 `<`、`<=`、`>`、`>=`；`lt`、`lte`、`gt`、`gte` 在 Parser 中归一化到同一组运算符。两侧都是字符串时按 Web 的 UTF-16 码元顺序比较，否则执行标量数值转换；任一数值为 `NaN` 时四种关系比较均返回 `false`。

Evaluator 已实现 `between()`、`between(]`、`between[)`、`between[]`。值、下界和上界按源码顺序各求值一次，两侧复用普通关系比较的字符串、数值转换和错误 Span 规则；开闭边界分别归一化为 `<` 或 `<=`。

Evaluator 已实现 `in` 与 `notin` 的数组元素、对象自身键和字符串子串判断。数组元素使用严格相等，不进行隐式类型转换；非法右侧返回 `InvalidMembershipTarget` 并指向容器表达式。

三向比较 `<=>` 返回 `-1`、`0` 或 `1`，并与关系比较共享字符串和数值排序规则。由于 `NaN` 没有可靠顺序，三向比较不会把它伪装成相等，而是返回指向对应操作数的 `UnorderedComparison`。

Evaluator 已实现严格相等 `===`/`is`、严格不等 `!==`/`isnot`、非严格相等 `==`/`equ` 与非严格不等 `!=`。严格比较不转换类型；非严格比较只执行 Web 标量转换，并保留 `null == undefined`。Array 与 Object 按运行时引用身份比较，绝不使用 Rust 的结构相等冒充脚本引用相等。原生 Callable 按登记的 `NativeFunction` 或 `NativeMethod` 身份比较；绑定接收者只参与调用，不制造新的方法身份。后续 ScriptCallable 使用独立脚本函数引用身份，不与原生登记项混合。

Evaluator 已实现 `&&`/`and`、`||`/`or` 与 `??` 的短路求值，并返回被选中的操作数本身。`&&` 和 `||` 使用 Value 真假值规则，`??` 只把 `null` 与 `undefined` 视为空；未选择的右侧不会发生运行时名称访问或其它求值行为。

条件运算 `condition ? consequent : alternate` 已实现单分支求值。条件使用 Value 真假值规则，只有选中的分支会被求值，结果保持该分支的原始 Value 类型。

Evaluator 已支持 `\\`、`\'`、`\"`、`\n`、`\r`、`\t`、`\b`、`\f`、`\v`、`\0`、`\xNN` 与 `\uNNNN`。Unicode 辅助平面字符使用相邻 UTF-16 高低代理对表示，例如 `\uD83D\uDE00`；孤立代理项、缺位、非十六进制数字和未知转义都会返回包含准确源码位置的 `InvalidStringEscape`。

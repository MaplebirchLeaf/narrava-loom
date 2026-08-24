# Narrava Macro

> 状态：基础结构实现中
>
> 更新日期：2026-08-22

## 职责

Macro 分为三个互相解耦的部分：

```text
Twee Macro Parser
→ MacroNode
→ Expression Parser（按 Macro 定义需要）
→ Runtime Macro Definitions
```

- Twee Macro Parser 只识别 `<<name ...>>...<</name>>` 结构；
- Expression Parser 只处理表达式；
- Macro Definitions 独立管理定义并参与 Macro 分派；
- State 只在执行上下文中按需提供变量访问。

## 当前实现

- `MacroNode` 已保留名称、原始参数、正文节点和 Span；
- 已支持同一行、空正文、完整闭合的 Macro；
- 已支持跨行、Text 正文、完整闭合的 Macro；
- 已支持跨行 Macro 嵌套，包括同名 Macro 的分层闭合；
- 已报告缺少闭合、闭合名称不匹配和孤立闭合符；
- `[[文本|链接]]` 是 `link` 的原始参数，不是独立语法；两侧允许保留变量引用，由 `link` Macro 在运行时求值；
- Runtime 已建立独立的 `MacroDefinitions<Definition>`；
- Definitions 容器提供 `add`、`update`、`del`、`get`、`has` 五个基础接口；
- `add` 对同名定义执行替换，Macro 名称区分大小写；
- `update` 只替换已有定义，不存在时返回 `MissingDefinition`，避免脚本拼错名称后意外新增 Macro；
- `MacroDefinition` 记录 `Inline` / `Container` 结构和 `Sync` / `Async` 执行方式；
- `MacroLocalScopes` 已提供调用层的 `enter`、`leave`、`get`、`set`、`del`；
- `MacroLocalScopes.enter_call()` 已建立 Widget 调用帧并完整保存 `@args`；
- `MacroEvaluationContext` 已把调用帧以只读方式接入 Expression，支持 `@args[index]`；
- `MacroDefinition.argument_kind` 要求注册时显式选择 Raw 或 Argument List；
- Argument List 已在全部参数准备成功后一次性进入调用帧，失败时不产生半组 `@args`；
- Definitions 缺失定义与无活动 Local Scope 错误已提供稳定 Diagnostic 转换；
- Handler 已接入 Definition 分派、State／Story 受控 Context、Macro 局部域与 Runtime continuation；Host 调度和公开 Resume 输入仍未接通。

`Inline` 不接收正文，例如 `<<set $value = 1>>`；`Container` 接收正文并要求闭合标签，例如 `<<if condition>>...<</if>>`。Twee AST 与 HIR 使用 `MacroSyntaxKind` 显式保留这项源码事实，因此 `<<name>>` 与空正文的 `<<name>><</name>>` 不会因为正文数组同为空而混淆。显式自闭合 `<<name />>` 是否作为可选拼写由 Twee Parser 后续决定，不影响 Definition 类型。

`set` 接受两种完全等效的赋值拼写：`<<set $test = "test">>` 与 `<<set $test to "test">>`。其中 `to` 只由 `set` 参数解析器在顶层识别，并统一降低为 Expression 的 `=`；它不是通用 Expression 运算符，也不会切分字符串、分组或集合内部的同名文本。

Native Twee 正文不会因为出现 `$`、`_`、`@` 或 `${...}` 而自动求值；它们都只是 Text。变量只有进入 `print` 等显式 Expression Macro 参数时才会读取。`print` 已实现以下语义：

先执行 `<<set $forest_name to "奇幻">>`：

| Twee 源码 | 语义 Text |
|---|---|
| `你在$forest_name森林里。` | `你在$forest_name森林里。` |
| `你在<<print $forest_name>>森林里。` | `你在奇幻森林里。` |
| ``你在<<print `$forest_name`>>森林里。`` | `你在$forest_name森林里。` |
| `你在<<print ${$forest_name}>>森林里。` | `你在奇幻森林里。` |
| `你在${$forest_name}森林里。` | `你在${$forest_name}森林里。` |

`print` 的反引号形式是完整字面文本，因此 `` `$forest_name` `` 不读取变量。`print ${expression}` 是 `print` 自己的显式求值参数形式，只在 Macro 参数边界生效；相同字符出现在普通 Twee 正文中仍是字面 Text。`print` 求值后产生宿主无关 Text，不产生 HTML，也不会把输出再次当作 Macro 源码执行。

Macro 外壳与位移运算符共享 `<`、`>` 字符。为让源码和 Parser 都保持确定性，位于 Macro 参数中的 `<<`、`>>`、`>>>`、`<<=`、`>>=`、`>>>=` 必须出现在显式圆括号内；Macro Header Scanner 只把分组深度为零的 `>>` 识别为外壳结束符。例如：

| Macro 源码 | 结果 |
|---|---|
| `<<set $test = ($test << 3)>>` | 有效 |
| `<<set ($test <<= 3)>>` | 有效 |
| `<<print ($test >> 3)>>` | 有效 |
| `<<set $test << 3>>` | 无效：既不是赋值，位移也没有分组 |

这项附加限制只属于 Macro Header；`${$test >> 3}` 等普通 Expression 嵌入不需要为了 Twee 外壳额外加括号。

HIR 已使用专用 `Set` 节点保存归一化后的赋值 Expression。目标和值分别通过 Expression Parser；目标复用统一的 `is_assignable_target()`，缺少分隔符、非法目标或无效值均保留对应参数位置的 Diagnostic。

## 控制边界

| Macro | 作用边界 | 当前状态 |
|---|---|---|
| `continue` | 跳过最近循环的本轮剩余正文 | 已实现 |
| `break` | 结束最近循环，继续执行循环之后的正文 | 已实现 |
| `return expression` | 结束最近的可返回值调用单元并返回值 | 语法保留；调用单元尚未设计 |
| `exit` | 结束最近的 Widget 或 Passage 执行域，不携值 | HIR 与逻辑信号已实现 |
| `silently` | 执行正文及副作用，但丢弃该段语义输出 | Runtime 已实现 |

`return` 不属于循环，也不属于文本块 Widget。它出现在循环中时会穿过循环，是因为它最终由函数等可返回值调用边界消费。Twee Parser 保留无闭合标签的 `<<return>>` 与 `<<return expression>>`，HIR 使用保留的专用节点保存可选 Expression，防止它被动态 Macro 定义覆盖；Runtime 暂不执行该节点。

## 循环

- `<<while expression>>...<</while>>`
- `<<for key in expression>>...<</for>>`：遍历键或属性名；
- `<<for value of expression>>...<</for>>`：遍历值；
- `<<for value range start to end>>...<</for>>`：包含终点的数值范围；
- `<<for value range start to end step amount>>...<</for>>`：显式指定步长。

Narrava 的 `for` 不采用 `初始化; 条件; 更新` 三段式，也不使用 `..`、`..=`。`start`、`end` 与 `amount` 都是 Expression；步长省略时根据起点与终点选择 `1` 或 `-1`。

`<<break>>` 与 `<<continue>>` 只允许出现在 `for` 或 `while` 正文中。`while` 每轮重新求值条件；循环执行器需要可配置的迭代上限，防止游戏内容意外锁死 Runtime。

HIR 已使用专用 `HirWhile`、`Break`、`Continue` 节点。循环上下文通过显式深度传递；进入 `for` 或 `while` 才增加深度，经过 `if` 或普通 Macro 时保持不变。循环外控制语句和控制语句携带参数都会产生 Diagnostic。

## 条件分支

```twee
<<if condition>>
  ...
<<elseif condition>>
  ...
<<else>>
  ...
<</if>>
```

`elseif` 和 `else` 是 `if` 的结构化子句，不注册为独立 Runtime Macro。只执行第一个条件成立的分支。

## Switch

```twee
<<switch expression>>
<<case value>>
  ...
<<case otherValue>>
  ...
<<default>>
  ...
<</switch>>
```

`case` 与 `default` 是 `switch` 的结构化子句。默认使用严格相等语义，只执行第一个匹配分支，不支持隐式贯穿；首轮不需要 `break` 结束 case。

Twee Parser 已让 `case`、`default` 共用 `<</switch>>`。HIR 使用专用 `HirSwitch` 保存被比较 Expression、有序 case Expression 与可选 default 正文；孤立子句、首个 case 前正文、default 参数、重复 default 或 default 后 case 都会被拒绝。严格匹配行为在 MIR／Runtime 实现时复用 Expression 的严格相等语义。

## Widget

```twee
:: Widgets [widget]
<<widget "name">>
  ...
<</widget>>
```

`[widget]` 是引擎保留 Tag。Widget 只能作为该类 Passage 的顶层节点定义；定义容器允许排版空白，但不接受可见文本或其他节点，Widget 正文只在调用时展开。缺少 Tag、嵌套定义和无效容器内容分别返回 `hir.widget_tag_required`、`hir.nested_widget`、`hir.invalid_widget_content`。

Twee Widget 名称全局唯一、区分大小写；第二个同名定义返回 `hir.duplicate_widget`。动态环境仍可显式使用 `Macro.add()` 更新 Definitions，这不改变 Twee 的静态规则。

`register_story_widgets()` 按源码顺序收集顶层 Widget，并返回 `WidgetRegistrationReport { registered, replaced }`。`replaced` 仅统计对收集前已有动态定义的替换；Twee 内部重名已在 HIR 阶段被拒绝。

HIR 已使用专用 Widget 节点保存名称与正文。定义使用引号包围名称，引号内容仍必须符合 Expression 标识符的 ASCII 规则并区分大小写；Widget 不声明命名形参，名称后的额外内容会被拒绝。调用 Widget 时，全部位置实参保存在当前调用帧的 `@args` 数组中，通过 `@args[0]`、`@args[1]` 等索引读取；越界读取沿用 Array 的 `undefined` 语义。

`@args` 是调用帧保留名，普通局部变量 API 不允许覆盖或删除。`MacroEvaluationContext` 在求值开始时组合基础 State 上下文与当前 Macro 帧：global、setup、`$`、`_` 继续交给基础上下文，`@` 和 `@args` 只由 Macro 提供。

采用标准 Expression 实参的 Macro 使用顶层空白分隔，例如 `<<name "Maple" ($count + 1) { active: true }>>`。复合 Expression 用括号明确边界，实参之间不使用逗号或分号；数组、对象和函数调用内部仍使用各自语法。公共 `parse_list()` 负责生成有序 AST 列表；具体 Macro 仍可声明自己的 Raw 参数格式，因此 HIR 不会强制把 `link` 的 `[[文本|目标]]` 等专用语法解析为 Expression。

`MacroDefinition` 通过必填的 `MacroArgumentKind` 固定参数契约：`Raw` 把整段原文交给自定义 Handler；`ArgumentList` 使用顶层空白分隔，每一项可以是普通 Expression 或 `[[显示文本|目标]]`。参数格式属于可替换的 Runtime Definition，不写入通用 HIR，也不由调用文本自动猜测。

Argument List 已建立独立解析结果，并按调用源码顺序保存 Expression 与 Interaction Target。Expression 分段解析时同时记录它在宏参数原文中的字节偏移，后续 Diagnostic 可映射回正确位置；位于参数起点的 `[[` 专用于 Interaction Target。

参数准备层已把上述结果转换为有序 `Value`：Expression 使用 Runtime 注入的求值函数，Interaction Target 对 scripts Handler 表现为只有 `label`、`target` 两个字段的对象，不附加 `kind`。求值函数不固定为只读入口，后续可接入可写 Context、随机源或异步调度；失败时仍保留对应 Expression 的宏参数原文偏移。

`InteractionTarget` 可由 `link`、`button`、`choice` 等交互 Macro 复用。Parser 只校验完整外壳、`|` 分隔符和两侧非空内容，并保留原文供后续求值。Core `link()` 把单个 Interaction Target 转换为 `PresentationNode::Navigation`；容器入口 `link_with_body()` 还会用同一 ID 原子登记延迟正文、目标与捕获值，不创建平台控件。Host 激活后的执行顺序仍待接通。

`label` 与 `target` 已支持两种动态写法：整段 `$`、`_`、`@` 变量引用，以及普通文本中的 `${expression}`。两者共用 Runtime 注入的 Expression 求值函数和 Narrava 受控标量文本转换；`undefined`、`null`、Boolean、Number、String 可转换，Array、Object、Function 不会被隐式文本化。普通裸文本仍按字面值处理，因此 `Map` 不会被误读为 global。解析结果分别保存 `label`、`target` 在 Macro 参数原文中的字节起点，用于映射解析、求值和文本转换错误。

`${expression}` 的结束位置由共享插值扫描器识别。扫描器会跳过圆括号、方括号、花括号和带转义的单双引号内容，因此对象字面量与字符串中的 `}` 不会提前结束插值；扫描器只决定边界，Expression 的括号匹配与语法错误仍由 Parser 负责。

插值替换严格保留周围字面空白：`前往 ${$LocationName}` 与 `前往${$LocationName}` 分别得到带空格和不带空格的结果，Runtime 不自动插入、删除或折叠空白。

Runtime Macro 参数错误已统一收束为 `MacroArgumentIssue`，包含稳定 Diagnostic 与宏参数片段内 Span。Argument List 解析、Interaction Expression 解析／求值、不可文本化以及未闭合插值都通过同一结果映射；Expression 的内部 Span 会叠加对应参数或字段偏移，调用方再使用 `DiagnosticLocator` 附加完整 Twee 文件位置。

代码结构上，参数契约、Argument List Parser、Interaction Target、参数准备与参数 Diagnostic 已集中到 `macro_runtime/arguments` 子系统；`macro_runtime.rs` 继续从原路径公开重导出这些 API，避免内部拆分影响 Runtime 或 scripts 桥的后续调用边界。

其中 Argument List 与 Interaction Target 的边界解析位于 `macro_runtime/arguments/parser.rs`；参数求值、Interaction 动态文本处理和原子建立调用帧位于 `macro_runtime/arguments/prepare.rs`。`arguments.rs` 只保留公共参数契约、错误与 Diagnostic 映射；父层公开 API 仍不改变。

Macro 调用帧、`@` 局部变量生命周期与 `MacroEvaluationContext` 已集中到 `macro_runtime/context.rs`。该子模块只组合 Macro 局部域与外部 `EvaluationContext`，不会取得 State 所有权；父模块同样保持原名重导出。

因此 scripts 注册的自定义 Macro 可以接收混合参数，例如 `<<xxx "前置" [[确认前往|Map]] ($count + 1)>>...<</xxx>>`。Argument List 保持参数顺序，Runtime 将普通 Expression 求值为 Value，并将 Interaction Target 准备为结构化对象；整组参数准备成功后才一次性建立调用帧，任一 Expression 或 Interaction 插值失败都不会覆盖或污染外层 `@args`。

Widget 是可重复展开的 HIR 正文块，不是返回值函数。Runtime 使用 `RuntimeMacroHandler::Widget` 将正文与原生 Handler 保存到同一个 `MacroDefinitions`；同名定义遵循 `Macro.add()` 的替换规则。

`<<exit>>` 不接收参数，用于提前结束最近的执行域。Widget 调用建立执行域，因此 Widget 内的 exit 只结束本次 Widget 展开；Passage 顶层的 exit 结束当前 Passage。`if`、`switch`、`for`、`while` 与 `silently` 不建立执行域，只传播该信号。

Widget 正文执行入口已建立独立 `@args` 帧；正常完成、`exit` 和执行错误都会清理本次帧并恢复调用者局部域。`exit` 在该边界转换为普通完成，其他控制信号按各自所有者继续传播。

通用 `HirMacro` 可通过 `execute_widget_macro()` 查询共享 Definitions、在调用者 Context 中准备 Argument List，并执行其中保存的 Widget 正文。Widget Definition 固定为 Inline 调用；调用处携带正文时会在参数求值和 Widget 副作用前被拒绝。`RuntimeExecutionContext` 统一完成原生 Handler 与 Widget Handler 路由。

上层 `RuntimeExecutionContext.execute_body()` 已接入该 Widget 调用入口，同时仅借用 Definitions、State、Story 与 Local。Widget 正文直接调用另一个 Widget 时会递归使用相同 Definitions，并为每次调用建立独立 `@args` 帧；`if`、`switch`、`while` 与 `for` 内部的动态 Macro 都回到同一入口。

## 局部变量

Narrava 不提供 `let` 或 `const` Macro。变量前缀对应三种不同所有者：

- `$name`：由 `State.variables` 存储和控制；
- `_name`：由 `State.temporary` 存储和控制；
- `@name`：属于当前 Macro 调用上下文。

Widget 可以直接创建和修改 `@` 变量；离开当前调用后自动删除，不进入 State 或存档。三种前缀由 Expression Parser 识别，但变量读取和写入交给对应所有者。

嵌套 Macro 按当前调用域到外层调用域读取 `@` 变量，写入只进入当前调用域。每次调用的局部域互相隔离，并在同步结果返回或异步结果完成后销毁。

每条独立执行链拥有自己的 `MacroLocalScopes`，不能把一个全局作用域栈共享给同时等待的异步调用。Runtime continuation 将暂停位置、执行身份和局部域绑定到原执行链。

Handler 通过 `MacroInvocation` 接收名称、原始参数、准备后的 `Value` 参数、正文和 Runtime 借出的 Context。正文用 `MacroInvocationBody::Inline`／`Container` 明确区分，不使用含义模糊的可选值。Context 的具体能力由 Runtime 决定，因此 Handler 可以获得受控的 State、Story 或 Presentation 写入能力，但不会取得这些领域的所有权。

Handler 返回 `MacroHandlerOutcome::Complete(output)` 或 `MacroHandlerOutcome::Pending(handle)`。Pending handle 由调度器定义：Rust 内置实现可映射执行任务，scripts 桥可映射 JavaScript Promise；公共契约本身不依赖某一种异步模型。Handler 自身的错误继续使用外层 `Result` 表达，完成、暂停与失败不会混为同一种状态。

Macro 控制器可以为经过 Definitions 分派的名称注册简短的 `before`／`after` 生命周期 Hook。Hook 不是 Twee 控制流 Macro，也不建立新的作用域：

```text
准备并求值参数
→ 建立本次 @args 调用帧
→ before（可修改 @args）
→ Handler
→ after（可修改本次 Macro 输出）
→ 合并进 Passage Presentation
```

`before` 看到的 `@args` 与 Handler 接收的参数必须是同一组值；修改完成后 Handler 才开始执行。`after` 只拥有本次 Macro 的独立输出缓冲区，可以检查、替换、追加或过滤其中的语义节点，不能修改后续 Twee 节点或其他 Macro 已产生的输出。

同步 Macro 的 before、Handler、after 各执行一次。异步 Macro 的 before 只在首次调用时执行；Pending 和再次 Pending 不重复触发；after 只在最终 Complete 后执行一次。before 失败时不进入 Handler，Handler 失败时不进入 after，after 失败时整次 Macro 调用失败并由外层 Engine 事务处理。

`MacroLifecycleSubscriptions` 已提供 `before(name, callback)`、`after(name, callback)` 和 `off(id)`。同一名称可以登记多个 Hook，并严格按注册顺序读取；订阅 ID 只标识当前进程内的一次登记。MacroName 区分大小写。

`MacroLifecycleController` 已把订阅集合实现为 Runtime 所需的 `MacroLifecycleCallbacks`。它分别接收 before 与 after 调用适配器，按名称选出订阅并依注册顺序执行。订阅中保存的 Hook 因此可以是 Rust Handler、scripts Callback 身份或 Binding 句柄；Core 不保存 JavaScript Function，也不依赖某个平台脚本引擎。

编译器直接降低为 HIR 的固有逻辑、动作与输出语法不经过 Definitions，因此不允许生命周期订阅：`if`、`elseif`、`else`、`switch`、`case`、`default`、`for`、`while`、`break`、`continue`、`set`、`unset`、`run`、`include`、`goto`、`print`、`silently`、`return`、`capture`、`exit` 和定义语法 `widget`。大写或其他不同名称仍按普通 MacroName 处理，但必须存在对应动态 Definition 才能真正执行。

同步入口 `execute_prepared_sync_macro_with_lifecycle()` 已固定 `before → Handler → after` 顺序，并接收有序 Hook 序列。调用帧在 before 前建立，before 通过受控 `args_mut()` 修改的同一组值会成为 Handler 参数和 `@args`；多个 after 依次接收上一个 Hook 修改后的输出。任一阶段失败都会清理当前帧并保留明确阶段；Async Definition 由独立的可暂停入口处理。

`RuntimeExecutionContext` 已通过最小 `MacroLifecycleCallbacks` 边界接入 Widget 和同步 Native 调用。before 在 Handler 前修改当前 `@args`，after 在成功后转换该 Macro 的隔离 `PresentationOutput`；任一 Hook 失败都会清理调用帧并丢弃本次半成品输出。`if`、`set`、`for` 等编译器固有节点不进入这条回调路径。订阅控制器已经实现该边界；Runtime 不直接保存 Rust 闭包、JavaScript Function 或平台对象。

同步 Native／scripts Definition 已通过 `NativeMacroCallbacks` 进入统一 Runtime 分派。Binding 接收不透明 Handler 身份、结构化 `MacroInvocation` 和受控 `MacroLogicContext`，可以读取准备后的参数、访问 State／Story／`@` 能力并返回 `BodyExecution`。输出在成功前保持隔离；缺少 Binding Adapter、正文形态不符、参数失败或 Handler Diagnostic 都会显式返回错误。

Async Definition 使用 `AsyncNativeMacroCallbacks` 首次分派。Binding 可以立即 Complete，也可以返回不透明 Pending 句柄；Pending 会与 MacroName、执行身份和完整局部域共同保存，`@args` 不会留在共享 Runtime。`resume_async_native_macro()` 在最终 Complete 时执行同一名称的 after；再次 Pending 只替换平台句柄并保留名称与局部域。Handler 或 after 失败都会退出当前帧并返还外层作用域。

MIR 用 `InvokeMacro` 保存动态调用，并在 VM 中形成不前进位置的 `MacroPending`。该状态只是 VM 与 Macro 控制器之间的调用边界，不等于 Handler 的异步 Pending。控制器读取 Bytecode 自持有的 Macro HIR，通过 `RuntimeExecutionContext::execute_macro()` 复用统一 Definition、参数帧、Widget／Native 与生命周期分派，再用 `complete_macro()` 把成功输出交回同一位置。同步完成与异步 Pending／Resume／Cancel 均已接入 Engine 和 Host continuation；未配置控制器时明确回滚。

Widget 内的 `include` 由 `execute_macro_with_includes()` 在 Macro 隔离缓冲区内展开，因此包含内容保持源码位置并接受同一次 after 处理。返回的 `RuntimeMacroExecution::includes_entered` 由 Engine 合并进全局 include 预算；超限会回滚 State 与 Story。

`dispatch_macro()` 负责 Definition 到 Handler 的最小分派。调用前比较 Definition 与 Invocation 的 Inline／Container 形态，错误时不进入 Handler；调用后检查 Sync／Async 声明，Sync 返回 Pending 会被拒绝并保留原暂停句柄，便于调度器执行取消或清理。Async Handler 可以直接 Complete，也可以 Pending；声明为 Async 不强制每次调用都暂停。

分派层拥有的契约错误使用稳定 Diagnostic：正文形态不匹配为 `macro.body_kind_mismatch`，Sync Handler 非法返回 Pending 为 `macro.unexpected_pending`。Handler 自身错误不由分派层重新解释，而是通过注入的转换函数保留内置 Handler 或 scripts 桥定义的错误码与消息。

`prepare_macro_call()` 已把 Definition 查询和参数准备串成独立阶段。ArgumentList 会先完整解析并求值为有序 `Value`；Raw 保留原始参数且完全跳过 Expression Parser。返回的 `PreparedMacroCall` 只保存 Definition 引用与调用输入，不提前建立 `@` 调用帧，因此定义缺失、解析失败或求值失败都不会改变局部作用域。

`execute_prepared_macro()` 消费准备结果后建立当前调用帧，并把同一组参数同时提供给 Handler 的 `arguments` 与 Expression 的 `@args`。同步 Complete 或 Handler／分派错误只弹出本次新增帧，嵌套调用的外层帧保持不变。

内置逻辑 Handler 使用 `execute_prepared_logic_macro()`：它先建立当前调用帧，再组合 `MacroLogicContext`，因此 Handler 看到的 `arguments` 与 `@args` 来自同一份准备结果。同步完成或失败仍会立即清理本次帧；Pending 继续把完整局部链移交给暂停对象。

Async Pending 不把局部作用域留在共享 Runtime 栈中，而是把整条 `MacroLocalScopes` 移入 `SuspendedMacroScopes`，与调度句柄一起由 `MacroCallOutcome::Pending` 返回。必须移动完整链而非仅最内层帧，因为暂停的 Widget 仍可能读取外层调用的 `@` 变量。

`resume_macro_suspension()` 已建立最小恢复边界。恢复回调暂时借用完整局部链；Complete 会退出当前暂停帧并返回结果和外层作用域，再次 Pending 会重新封装整条链，失败会返回原错误以及已清理当前帧的外层作用域。这样调度器无论走哪条路径都能继续拥有或明确释放局部状态。

暂停状态现已收束为 `MacroSuspension`，身份、调度句柄和 `SuspendedMacroScopes` 不能再被分开传递。`RuntimeExecutionIdentity` 由 Story 执行实例编号和该 Story 内的执行链编号组成；它归 Runtime 所有，Macro 暂停只复用该身份。`resume_macro_suspension()` 必须接收预期身份并进行完整比较。身份不一致时恢复回调不会执行，错误会原样保留整个暂停对象，供调度器重新路由或取消。

身份不匹配使用稳定 Diagnostic `macro.resume_identity_mismatch`，其中明确记录实际与预期的 Story／执行链编号。`MacroResumeError::diagnostic()` 对身份错误和 Handler 恢复错误使用同一个只读入口；Handler 错误码仍由所属实现转换，诊断过程不会消耗暂停句柄、局部状态或失败后的外层作用域。空的 `MacroLocalScopes` 不能生成 `SuspendedMacroScopes`，公开 `suspend()` 会返回 `MacroLocalError::NoActiveScope`，从类型入口阻止无当前调用帧的无效暂停状态。

`MacroSuspension` 本身不是完整 Runtime／VM continuation。`RuntimeMacroContinuation` 已进一步把它与相同身份、停在 `InvokeMacro` 的完整 `MirExecutionFrame` 绑定，并在失败时返还全部所有权；但该组合仍没有 Engine 检查点、Passage 生命周期和待确认导航，因此不能直接暴露为 `HostInput::Resume`。下一层 Engine continuation 必须显式拥有这些事务状态。

异步等待前产生的 include／goto 不再依赖活动 Story 借用：`StoryRuntimePending` 可以随 continuation 保存，并在恢复时校验所属编译结果。它只保存请求，不确认导航，也不复制 Story history。

`EngineMirContinuation` 已成为上述 VM/Macro 暂停、pending 请求、State/Story 检查点与 Passage 进度的共同所有者。它现在能重新附着请求并调用 Runtime 恢复：再次 Pending 重建同一 Engine continuation，Complete 进入仍持有事务检查点的 `EngineMirResumedTransaction`。后者会合并 Macro include 消耗、校验控制信号并继续 VM；Halted、goto/StopPassage 与后续 MacroPending 分派均已接通。Native／scripts 首次调用已能产生 Runtime suspension，尚待 Engine 起始分派接线，因此暂不开放 `HostInput::Resume`。

VM 级恢复现已实现：`RuntimeMacroContinuation::resume()` 复用 `resume_macro_suspension()`，并把 suspension 中的平台句柄显式移交给恢复回调。Complete 将输出交回原 MIR 指令并退出当前调用帧；再次 Pending 保留原 VM 位置和完整局部链，只使用回调返回的新调度句柄。Native 包装层已经补齐最终 after；Engine 公开首次 MIR 入口也能在 MacroPending 后经统一分派组成完整 continuation，尚待 Host 输入边界持有和恢复它。

调用准备错误已统一为 `MacroCallPreparationIssue`。Definition 缺失生成无参数 Span 的稳定 Diagnostic；ArgumentList 解析和参数求值错误复用 `MacroArgumentIssue`，继续保存相对参数原文的 Span，并可通过 `DiagnosticLocator` 映射到完整 Twee Source。统一类型不会为 Definition 错误虚构参数位置。

调用子系统已按职责拆分：`macro_runtime/call.rs` 只保留 Definition 查询、参数准备和首次 Handler 执行；`call/diagnostic.rs` 管准备错误映射；`call/lifecycle.rs` 管同步 before／Handler／after；`call/suspension.rs` 管执行身份、暂停对象和恢复生命周期。父模块继续原名重导出公开 API。

内置逻辑 Macro 使用 `MacroLogicContext` 组合能力，而不是直接取得 State 或 Story 所有权。该 Context 借用 `WritableEvaluationContext`、`MacroStoryAccess` 和当前 `MacroLocalScopes`：Expression 的 global、setup、`$`、`_` 读写交给 State，`@` 与 `@args` 交给 Macro Local；`has/include/goto` 只通过 Story 请求接口调用。Presentation 与模组能力不进入逻辑 Context。

每层 `MacroLocalScopes` 现在是独立调用帧：包含局部绑定与完整的 `@args` 位置实参。嵌套调用退出后恢复外层帧。

延迟回调只保留 `capture` 明确列出的绑定，不自动延长整个调用域的生命周期。

## Passage 操作

- `<<include passage>>`：在当前位置执行并渲染目标 Passage，不改变当前 Passage；
- `<<goto passage>>`：导航到目标 Passage，并结束当前 Passage 后续执行。

`passage` 是返回 Passage 名称的 Expression，名称比较区分大小写。循环包含和导航重入的限制在 Runtime 实现前确定。

HIR 已使用不同的 `Run`、`Include`、`Goto` 节点保存各自的 Expression AST。三者共用参数解析与 Diagnostic 映射，但保留不同控制语义：`run` 丢弃结果，`include` 不改变导航，`goto` 会在 Runtime 终止当前 Passage 后续执行。

原生 `run()` Handler 已直接执行 HIR 保存的 Expression：赋值、更新和函数调用产生的副作用会保留，表达式自身的结果统一丢弃并返回 `undefined`。求值错误不改写类型或 Span，后续可直接映射回 Twee Source。

原生 `set()` Handler 执行 HIR 已验证的赋值 Expression，并同样返回 `undefined`。`to` 与 `=` 的差异只由 HIR 降级处理，Runtime 接收的都是 `Assignment(Assign)`，不会维护第二套赋值语义。

`unset` 不等于写入 `undefined`。`WritableEvaluationContext` 已新增根绑定删除契约：普通 global、`$` 与 `_` 由 State 删除，`@` 由当前 Macro Local 删除；`@args` 仍是不可删除的保留绑定。Expression 删除路径现已支持 Object 成员和 Object 字符串索引；Array 最终索引删除会被拒绝，以免破坏稠密 Array 语义。

原生 `unset()` Handler 已调用统一删除路径，丢弃被删除的旧值并返回 `undefined`。删除失败继续使用 Expression 的原错误与 Span；它不会把删除结果暴露为 Macro 输出。

原生 `include()` 已求值 Passage 名称并通过 `MacroStoryAccess` 发出包含请求。名称只允许受控标量文本转换，Object、Array 和 Function 不会被隐式字符串化；`Story.has()` 使用原字符串并区分大小写。目标不存在使用稳定错误 `macro.missing_passage`，无效名称使用 `macro.invalid_passage_name`。

原生 `goto()` 与 `include()` 共用 Passage 名称解析和错误边界。两者不再返回含糊的渲染值，而是返回 Runtime 的 `BodyControl`：`include` 为 `Continue`，`goto` 在 Story 接受导航请求后为 `StopPassage`。目标缺失或 Story 拒绝请求时不会产生停止信号。

Runtime 逻辑分派现已直接识别 HIR 的 `Run`、`Set`、`Unset`、`Include` 和 `Goto` 节点，并调用对应原生 Handler。未接入的节点不会静默跳过，而是返回稳定错误 `runtime.unsupported_hir_node`。

`If` 已进入递归正文执行：条件使用 Narrava Expression 的 truthiness，按源码顺序选择首个真值分支；后续条件不会求值，全部为假时才执行 fallback。分支返回的 `StopPassage` 会原样传播到外层正文。

源码控制宏保持小写：`<<if>>`、`<<elseif>>`、`<<else>>`、`<<switch>>`、`<<case>>` 和 `<<default>>`；`HirIf`、`HirSwitch` 只是 Rust 类型名。Runtime `Switch` 已让主值只求值一次，case 使用 `===`／`is` 的严格相等并选择首个匹配分支，全部不匹配时执行 default。

小写 `<<while>>`、`<<break>>` 和 `<<continue>>` 已接入 Runtime。`BodyControl` 使用独立的 `BreakLoop` 与 `ContinueLoop` 信号；while 只在当前循环边界消费它们，嵌套分支返回的 `StopPassage` 继续向外传播。

小写 `<<for target of collection>>` 与 `<<for target in collection>>` 已接入。Array 的 `of` 遍历值、`in` 遍历数字索引；Object 的 `of` 按属性顺序遍历值、`in` 遍历字符串属性名。集合在循环开始时建立快照，循环正文修改原集合不会改变本次迭代序列。String 遍历暂未定义。

小写 `<<for target range start to end>>` 与可选的 `step amount` 也已接入。Range 包含终点；起点、终点和步长只求值一次。默认步长根据方向选择 `1` 或 `-1`，显式步长必须是有限、非零且朝终点移动的 Number。

Text 与 `silently` 已进入 Host-neutral Presentation 边界。`link` 已有 Navigation 语义，并能将容器正文原子登记到延迟动作所有权容器；Host 已能在同一检查点内执行同步或异步正文并导航。正文输出不做瞬时呈现，主要用于 State 与逻辑副作用；`button`、`choice` 后续只有在形成不同且稳定的跨 Host 语义时才扩展。区域替换和计时显示等语义在存在跨宿主稳定类型前不进入 Core。

## Capture

正式语法为 `<<capture @name>>...<</capture>>`，也可以一次列出多个 `@` 变量。它只为正文中创建的延迟回调保存指定变量的当前绑定，不负责复制整个 State。

`capture` 创建独立的捕获域，使 `link`、计时器等延迟正文仍能读取捕获时的绑定。`MacroLocalScopes::capture()` 已能从当前可见作用域只复制明确列出的名称，并通过 `CapturedMacroLocals::into_scopes()` 建立不携带原 `@args` 的隔离执行帧。未列出的或捕获时不存在的 `@` 变量不会被隐式保存；对象值保留 Narrava 引用身份，不进行递归复制。

`MacroInteractions` 是延迟 Macro 动作的 Core 所有者。每项 `MacroInteraction` 保存导航目标、HIR 容器正文与捕获值，并以 Presentation 的 `InteractionId` 定位。它提供明确的 `add`、`update`、`get`、`has`、`del`、`take` 与 `clear`：重复 `add` 不会覆盖旧动作，玩家激活使用 `take` 一次性取得所有权，避免同一链接正文重复执行。该容器不进入 State、Save 或平台 Presentation 数据。

HIR 已使用专用 Capture 节点保存有序且不重复的局部变量名与正文。参数至少包含一个变量，只接受以 `@` 开头的 Macro 局部变量。MIR 会把词法外层 Capture 的名称直接附到其内部动态 Macro 指令，VM 暂停时可读取这些名称；跳转不会遗留运行时捕获栈。Engine 分派时通过 `EngineMirMacroInvocation` 一次性交付 HIR 调用、执行身份与当前 Macro Local 形成的捕获值。Widget 递归执行也已在 Runtime 内维护词法捕获名称，并通过 `MacroInvocation.captures` 把 Value 快照交给嵌套 Native／scripts Macro。

## 当前 Macro 总表

这里的“当前”以源码已经能够识别的 Macro 为准。“允许 Hook”表示调用名称会经过 Macro Definitions 分派；编译器固有语法不能注册 before／after。`elseif`、`else`、`case` 与 `default` 只是所属容器的子句，不能单独调用。

| Macro | 当前状态 | 允许 Hook | 职责 |
|---|---|---:|---|
| `if`／`elseif`／`else` | Runtime 已实现 | 否 | 条件分支 |
| `switch`／`case`／`default` | Runtime 已实现 | 否 | 严格匹配分支 |
| `for` | Runtime 已实现 | 否 | `in`、`of`、`range` 循环 |
| `while` | Runtime 已实现 | 否 | 条件循环 |
| `break`／`continue` | Runtime 已实现 | 否 | 当前循环控制 |
| `set` | Runtime 已实现 | 否 | 写入变量或成员；`=` 与 `to` 等效 |
| `unset` | Runtime 已实现 | 否 | 删除允许删除的目标 |
| `run` | Runtime 已实现 | 否 | 执行 Expression 并丢弃结果 |
| `include` | Runtime 已实现 | 否 | 在当前位置执行目标 Passage |
| `goto` | Runtime 已实现 | 否 | 请求导航并结束当前 Passage 后续执行 |
| `print` | Runtime 已实现 | 否 | 求值 Expression 或输出反引号字面文本，产生语义 Text |
| `silently` | Runtime 已实现 | 否 | 保留正文副作用与控制信号，丢弃本块 Presentation 输出 |
| `slot` | Runtime 与 Host 已实现 | 否 | 建立带稳定 Presentation Key 的普通内容容器 |
| `replace` | Runtime 与 Host 已实现 | 否 | 替换固定 Region 或已存在的稳定 Key |
| `link` | Runtime 与 Host 已实现 | 否 | 产生导航；激活后执行容器正文并进入目标 Passage |
| `button` | Runtime 与 Host 已实现 | 否 | 使用按钮角色呈现同一延迟正文导航语义 |
| `exit` | Runtime 已实现 | 否 | 结束最近的 Widget 或 Passage 执行域 |
| `widget` | 定义与调用已实现 | 定义语法否 | 在 `[widget]` Passage 中定义可重复展开的正文块 |
| Widget 定义产生的名称 | Runtime 与 Hook 边界已实现 | 是 | 例如定义 `greet` 后调用 `<<greet>>` |
| Native／scripts 自定义名称 | 同步分派及异步首次调用已实现 | 是 | Binding 通过同步或异步 Callback 执行不透明 Handler 身份 |
| `return` | Parser／HIR 已保留，Runtime 未执行 | 否 | 未来结束可返回值调用单元 |
| `capture` | MIR、Engine 与 Widget Runtime 传递已接通 | 否 | 为延迟正文保存指定 `@` 绑定 |

Native Narrava 没有 `div`、`span` 或其他 HTML Macro；形似 HTML 的 Twee 内容只会成为普通 Text。

后续条件与赋值 Macro 也必须复用同一个 Expression AST，不能建立第二套运算符解析器。

`unset` 首轮只接收一个删除目标，并已进入专用 HIR 节点。编译期接受普通变量、非可选成员和非可选索引；字面量、调用、赋值结果与可选链会被拒绝。具体命名空间或对象是否允许删除由 State／对象运行时在执行时继续检查，HIR 不伪造所有权。

## 明确保留的扩展边界

- 定义可返回值调用单元后，再实现 `return` Runtime 边界；
- 只有出现不同于现有 `link` 的稳定跨 Host 语义时，再扩展新的 Interaction Macro；
- 不为未定义的调用单元或表现语义提前加入占位 API。

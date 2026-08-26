# Narrava Twee 编译器

> 状态：基础结构实现中
>
> 更新日期：2026-08-22

## 职责

Twee 编译器只负责叙事源码，不处理 State、Macro 注册表、JavaScript 或渲染。

编译器保留普通全局名称及 `$`、`_`、`@`、`setup` 等引用形式；运行时再分别交给 `State.global`、`State.variables`、`State.temporary`、Macro 局部域和 `State.setup` 解析。编译器不会读取脚本模块的 `export` 或 `import` 来推断 Twee 环境。

Macro 结构与 Expression 细节分别见 [/docs/reference/macro.md](/docs/reference/macro.md) 和 [/docs/reference/expression.md](/docs/reference/expression.md)。

```text
Twee Source
→ Token
→ Passage
→ Story
→ Semantic Analysis
→ HIR
→ MIR
→ LIR
→ Bytecode
→ Rust VM
       └→ I18n Text Catalog Metadata
```

## 当前实现

- Lexer 产生 `PassageDeclaration` 与原样保留的 `Text`。
- Native Twee 正文是宿主无关文本；HTML 标签没有内置表现语义。HTML 兼容能力留给以后单独设计。
- `/% ... %/` 是作者注释，可单行或跨行；Parser 在 Macro 组合与 HIR lowering 前移除它，未闭合时返回 `twee.unclosed_comment`。HTML `<!-- ... -->` 不是 Narrava 作者注释。
- Passage 名称保留大小写，标签按空白分隔。
- Parser 拒绝声明前游离文本和空名称。
- Passage 正文使用带 Span 的 `BodyNode`，Native Parser 当前只从正文产生 Text 与 Macro。
- AST、HIR 与 Runtime 不再保留普通正文 Interpolation 分支；动态文本统一通过显式 `print` 进入 Expression。
- 普通正文中的 `$name` 与 `${expression}` 都保持字面 Text；动态求值必须写入 `print` 等显式 Macro 参数。
- 最小 HIR 已支持 Passage、Text 与通用 Macro；Macro 参数中的 Expression 错误会返回精确 Diagnostic。
- `HirMacro` 保留名称、原始参数、Inline／Container 源码形态、嵌套正文和节点 Span，不绑定当前 Runtime Macro 定义；空正文 Container 不会退化为 Inline。
- `if`、`elseif`、`while`、`run` 的单个参数会在 HIR 转为 Expression AST；名称匹配区分大小写。
- `else` 参数必须为空；其他内置或脚本 Macro 的参数继续保持 Raw，避免编译器误解自定义语法。
- Twee `MacroNode` 已独立保留参数 Span，使逻辑 Macro 的 Expression 错误精确指向参数而非整个节点。
- Twee Parser 已让 `elseif`、`else` 作为 `if` 的子句共用 `<</if>>`，不要求虚构的独立闭合标签。
- HIR 使用 `HirIf` 保存有序条件分支和可选 fallback；子句不作为可独立分派的 Runtime Macro。
- HIR 拒绝孤立子句、重复 `else` 及 `else` 后继续出现的子句。
- HIR 使用专用 `HirFor` 区分 `in`、`of`、`range`，并递归保留循环正文。
- `for` 目标首轮只接受普通全局名称或 `$`、`_`、`@` 变量，不接受成员、索引或其他可写表达式。
- `in`、`of` 的集合以及 `range` 的起点、终点、可选步长均解析为 Expression AST，并分别保留宿主 Source Span。
- `to`、`step` 只在参数顶层作为分隔关键字；字符串、数组、对象、分组或调用内部的同名文本不会被误切分。
- HIR 使用专用 `HirWhile` 保存条件 AST 与循环正文，并使用独立 `Break`、`Continue` 节点。
- `break`、`continue` 由 Twee Parser 识别为不需要闭合标签的 Inline Macro；HIR 拒绝参数及循环外使用。
- `return` 由 Twee Parser 保留为 Inline Macro，并进入保存可选 Expression 的专用 HIR；可返回值调用域尚未定义，因此暂不生成 Runtime 控制信号。
- `set` 由 Twee Parser 识别为 Inline Macro；HIR 将顶层 `to` 和普通 `=` 统一为 `AssignmentOperator::Assign`。
- `set` 的目标和值独立解析，嵌套 Expression 内的 `to` 或等号不会被当作 Macro 分隔符。
- `print` 由 Twee Parser 识别为 Inline Macro，并降低为专用 HIR；普通 Expression、`${expression}` 与反引号字面参数产生宿主无关 Text。
- `silently` 降低为专用容器 HIR，不接受参数；跨行容器与同一行完整容器均可保留嵌套正文。
- `run`、`include`、`goto` 由 Twee Parser 识别为 Inline Macro，并分别进入专用 HIR 节点。
- 三种动作共用必需 Expression 参数解析和源码定位，但不会在 HIR 合并为同一种运行时行为。
- `unset` 由 Twee Parser 识别为 Inline Macro；HIR 保存单个删除目标 Expression，并拒绝不可写或含可选链的目标。
- HIR 只验证目标结构，State 命名空间与对象属性的实际删除权限留给 Runtime。
- Twee Parser 已让 `case`、`default` 作为 `switch` 子句共用 `<</switch>>`。
- HIR 使用 `HirSwitch` 保存被比较 Expression、有序 case 分支与可选 default，不支持隐式贯穿。
- 孤立子句、非法顺序、default 参数及 case 外游离正文会产生精确 Diagnostic。
- Parser 已识别同一行内完整闭合且正文为空的通用 Macro 外壳。
- Parser 已识别跨行、正文为 Text 且完整闭合的通用 Macro 外壳。
- Parser 已递归组合嵌套 Macro，并按层级匹配同名闭合符。
- Parser 已报告未闭合、错名闭合和孤立闭合，并保留 Macro 名称与 Span。
- Macro Header 中的位移及位移复合赋值必须位于圆括号内；后续 Scanner 只把分组深度为零的 `>>` 作为外壳结束符。
- `Start`、`StoryInit`、`Header`、`Footer`、`Bar`、`BarStowed` 是保留的特殊 Passage，声明时不能带 Tag。
- Twee ParseError 已能转换为带稳定代码、相对 Source 和精确 Span 的公共 Diagnostic。
- Twee SemanticError 与 StoryError 已能转换为公共 Diagnostic，重复 Passage 指向后出现的声明。
- `MacroNode` 保留名称、原始参数与正文节点，不绑定运行时定义。
- 当前 `twee::Story::build()` 汇总全部 Twee Source，忽略其他 SourceKind。
- 当前编译期 Story 拒绝跨 Source 的重复名称。
- 当前 `twee::Story::passage()` 按区分大小写的名称查找入口。
- 起始 Passage 固定为 `Start`，由引擎内部约定，通过编译期 Story 查询确认入口存在。
- 最小 MIR 已建立 `MirStory`、编译内 `MirPassageId`、`MirBody`、顺序指令和明确 `Halt`；当前降低 Text、两种 Print、基础分支与循环，以及 set、run、unset、include、goto 动作，其他 HIR 节点返回 `MirLowerError`，不会被静默跳过。`MirStory` 同时拥有同源 I18n 目录，可翻译输出片段保留消息 ID 与 placeholder 身份。
- LIR 已建立 VM 的可执行程序边界：`LirProgram::lower()` 为 Passage 建立区分大小写的索引，拒绝重名，并在运行前验证所有跳转地址。Engine、Host 与 VM 不再直接接收 `MirStory`。
- Bytecode 已固定 `NRVA` 魔数、格式版本、Opcode、Passage 入口表和字符串／Expression／Macro／I18n 常量目录；VM 只接受 `BytecodeProgram` 或 `BytecodeMacroBody`。
- VM 将同一 I18n 消息的连续输出片段合并为一个执行单元，所有 placeholder 先求值，再产生一个宿主无关 Text；结构配对不依赖 Source Span 唯一性。
- `if` 的每个条件降低为 `JumpIfFalse`，真分支结束使用 `Jump` 越过后续分支与 fallback；所有目标均使用 `MirInstructionPointer`，不把裸下标混入公开指令语义。
- `switch` 先以 `Evaluate` 把主值写入一个 `MirValueSlot`，各 case 再通过 `JumpIfNotStrictEqual` 严格比较；主值只求值一次，case 不会隐式贯穿。
- `MirBody` 同时保存指令与执行帧需要的临时值槽数量，VM 不需要扫描指令反推帧容量。
- `while` 的条件指令同时作为循环回边和 `continue` 目标；条件为假及当前循环的 `break` 均跳到循环结束。lowering 使用嵌套循环目标栈，内层 break 不会越过外层循环。
- `for in`、`for of` 分别以 Keys、Values 视图建立集合迭代槽；`for range` 一次性保存起点、终点与可选步长。三者共用 `NextIteration`，每轮把下一值写入已验证目标，耗尽与 break 跳到循环结束，continue 返回 NextIteration 而不重复初始化。
- `MirBody.iterator_slot_count` 明确给出执行帧容量；每个 `MirIteratorSlot` 可在 VM continuation 中独立保存当前迭代状态。
- `set` 与 `run` 共用 `EvaluateDiscard`：赋值副作用已经由 Expression AST 表达，不在 MIR 复制两种同义求值指令。`unset` 使用独立删除指令。
- `RequestInclude` 与 `RequestGoto` 保持不同 Story 语义；前者执行后返回当前链，后者结束当前 Passage 并等待 Engine 确认导航。
- `silently` 在 lowering 时把输出抑制属性写入 Text、Print 与 include 调用点，不生成可能被 break、exit 或 goto 跨过的 Begin／End 开关。State 与控制指令保持原样。
- 当前 `ExitPassage` 结束最近的 Passage／include 帧；Widget MIR 帧建立后，Widget 内 exit 将由对应 Widget 边界消费。
- 通用 `HirMacro` 降低为未绑定 Definition 的 `InvokeMacro`，继续保留名称、参数、Inline／Container 形态与正文；实际 Handler 只能由运行时 Macro 控制器解析。
- `MirExecutionPosition` 使用 Passage 身份与有边界的指令位置共同定位执行点；PassageName 查询继续区分大小写。
- `[[文本|目标]]` 只是 `<<link ...>>...<</link>>` 的原始参数，不是可脱离 Macro 使用的 Twee 语法；Narrava 不接受 `[[目标<-文本]]` 或 `[[文本->目标]]`。运行时内置 `link` 将合法参数准备为 Navigation，显示内容和目标可通过显式参数插值读取 `$`、`_`、`@` 或普通全局变量。
- AST/HIR 必须继续保存可显示 Text、链接或按钮标签、静态选项、所属 Passage、Source Span 与显式动态参数边界；这些元数据既用于 Diagnostic，也用于控制台 `I18n.export()` 生成 `i18n/` 目录中的语言文件。

## IR 边界

- HIR 保留叙事结构并完成名称解析。
- MIR 已覆盖分支、基础循环及 State／Story 动作，保留叙事控制意图与 I18n 身份。
- LIR 是经过结构验证、建立 Passage 索引的低层程序；Bytecode 再把它编码成 VM 唯一接受的操作码与常量目录。
- Bytecode 自持有 I18n ID、placeholder、Expression 与 Macro HIR，不允许执行编码丢失翻译身份，也不保留构建期源码引用。
- 翻译消息键以 PassageName 与 IR 结构路径为基础；源码行号只用于定位，不单独充当稳定身份。连续可显示文本可整理为一条消息，但不能跨越会改变执行或渲染顺序的控制节点。
- 动态 Expression 在翻译目录中降为受控 placeholder，目录同时记录 placeholder 到已编译 Expression 的对应关系。译文可以调整语序，但载入时必须校验 placeholder 集合完全一致；翻译 JSON 永远不能创建新的 Expression。
- 静态成员链会保留为可读 placeholder，例如 `$hero.profile.name` 与 `setup.build.channel`；动态索引、调用和计算表达式使用 `value_n`。
- placeholder 可选择关联动态 dictionary 名称；只有运行时实际求值为 String 的结果会查询字典，Number、Boolean、Null 与 Undefined 仅插入显示文本。字典缺项时保持原字符串。字典属于翻译数据，不进入 State，也不能改变变量本身。

`if`、循环和 `switch` 在 MIR 中降低为显式分支和跳转。Macro 调用保留名称或 ID，通过运行时 Macro 控制器分派，不能静态绑定到某个可被 `Macro.add()` 替换的实现。

当前 Bytecode 是带格式头、可直接序列化和反序列化的拥有型编码；VM 边界已经稳定。
`.nar` 保存并分别校验拥有型基础源码与可执行 Bytecode，校验后直接运行 Bytecode，并从源码
记录建立 Script Bundle，不再改变 Compiler、Engine 或 VM 的层级关系。

运行时公开名称 `Story` 属于 Passage 与导航控制器；Rust 编译期聚合类型通过
`twee::Story` 命名空间明确区分，不在当前 API 中另建同义包装。

## 输出边界

Twee Compiler 把 Native 正文完整保存为 HIR Text，并为 I18n 保留稳定身份；`${expression}` 在正文中没有特殊含义。已实现的 `print` 作为显式求值边界，将动态结果转换为额外的 Presentation Text。Compiler 不识别 DOM 标签或 CSS 语义，形似 HTML 的内容也没有 Native 表现特权。

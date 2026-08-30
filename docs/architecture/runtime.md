# Narrava Runtime

> 状态：基础结构实现中
>
> 更新日期：2026-08-23

本文描述 Core 的运行时所有权与已实现行为；表现层边界见 [/docs/architecture/protocol.md](/docs/architecture/protocol.md)。

## 公开能力与内部所有权

Host、Binding、开发者控制台、scripts 和获准的模组可以使用以下语义能力：

```text
Engine      State       Macro       Story
Logger      Event       Save        Resource
ModLoader   ModUtils    I18n
```

这些名称是公开能力，不要求 Rust 内部存在同名全局单例。Core 可以使用 Store、Definitions 或 Service 实现它们，但不得让 Binding 保存第二份状态真相。

- Engine 协调生命周期与跨领域事务。
- State 只拥有变量命名空间。
- Macro 只拥有定义、调用帧和局部变量。
- Story 只拥有 Passage、当前位置和导航时间线。
- Logger 收集结构化日志；Diagnostic 描述稳定问题数据。
- Resource 只拥有逻辑资源身份和访问生命周期。
- Renderer、平台事件循环和输入设备不属于 Core。

## Engine

Engine 是 Core 的执行协调入口。它不保存 State 数据、Story history、Macro Definitions、平台 Renderer 或第二份启动状态。

### 导航事务

导航前，Engine 同时取得 `StateCheckpoint` 和 `StorySnapshot`。成功时提交 State 修改、导航记录和语义输出；Runtime 错误、生命周期错误、请求协议不一致或执行上限耗尽时恢复两个领域。

Runtime 的 goto 先形成经过验证的请求。只有当前 Passage 返回 `StopPassage` 且存在一个 pending goto 时，Engine 才确认目标并继续执行。include 在原节点位置展开，不创建 history；未被 Runtime 消费的 include 请求不能静默提交。

`Engine::navigate_mir_chain()` 现在接收从 MIR 降低得到的 `LirProgram`，并把 VM 接入同一事务边界。每个 Engine Passage 执行建立一条 VM 调用链；Halt 提交累计 Surface，NavigationPending 转换为经过 Story 验证的 goto 请求。VM、Expression、目标映射或 include 预算失败均通过原有 `StateCheckpoint + StorySnapshot` 回滚，不建立第二套事务实现。

`Engine::navigate_mir_chain_with_macros()` 在同一循环中注入同步 Macro 控制器。Engine 只负责读取稳定暂停、调用控制器、校验 `BodyControl`、合并输出和提交或回滚；Definition、生命周期与 Binding 仍归 Macro Runtime。剩余 include 预算会传给控制器，`RuntimeMacroExecution` 再报告 Macro 内实际展开量，使 VM include 与 Widget include 共用一次 Engine 预算。未提供控制器的原入口保持明确 `MacroPending` 错误。

`EngineExecutionLimits` 显式包含：

- `passages`：一次连续导航最多执行的 Passage 数；
- `includes`：一次执行最多展开的 include 数。

两个值都允许为 `0`，调用方不能依赖隐藏默认值。

### 启动与新游戏

`Engine::start()` 只在 Story 尚无当前位置时启动，并以 `Story.current()` 作为唯一启动判断。启动顺序为：

```text
注册 Widget
→ 执行可选 StoryInit
→ Start Passage 的 Init
→ Start
→ 正文
→ Render
→ Display
```

`StoryInit` 用于逻辑初始化。它不创建 history，不发布普通 Passage 生命周期，当前实现会丢弃其 `BodyExecution.output`。StoryInit 请求 goto、遗留 include、返回异常控制信号或执行失败时，Engine 回滚初始化事务。启动方必须先由 Macro 收集 `[widget]` Passage；MIR 为这些纯定义 Passage 生成空执行体，不把 `widget` 声明误当成运行指令。

保留名称统一定义在 `story::special`：`START_PASSAGE`、`STORY_INIT_PASSAGE`、
`HEADER_PASSAGE`、`FOOTER_PASSAGE`、`BAR_PASSAGE`、`BAR_STOWED_PASSAGE`。Engine、Host、Save 和工具不得各自散落
同义字符串。Tauri Host 会在普通 Passage 更新后用隔离的 State／Story 视图渲染 `Bar` 与
`BarStowed`，因此它们可以读取当前状态，却不会修改真实 State、当前位置或导航历史。其他 Host
可通过 `HostApi::render_special_mir`采用相同边界。

`Engine::new_game_with_lifecycle()` 按以下顺序原子执行：

```text
旧 Passage End
→ State.reset_game()
→ Story.reset()
→ StoryInit
→ 新 Start Passage
```

失败会恢复调用前的完整 State 与 Story。

### Passage 生命周期

单次导航 Passage 的生命周期为：

```text
PassageInit → PassageStart → Reaction Phase → Passage Body → PassageRender → PassageDisplay → PassageEnd
```

- Init：建立本次访问上下文，允许 scripts 通过 State API 修改变量；
- Start：发布已进入 Passage 的生命周期事实；
- Reaction Phase：解析 lifecycle Reaction；`exit` 可在正文前截断，`goto` 复用正常导航事务；
- Passage Body：执行 HIR、Expression 与 Macro；
- Render：Core 已形成本跳 Surface 输出；
- Display：Host 已完成本跳显示；
- End：真正离开当前导航 Passage。

入口 `params` 只属于本次入口 Passage，不自动写入 State，也不由无参数 goto 继承。include 与特殊 Passage 不创建独立 Reaction lifecycle。

精确 `[exit]` Passage 执行逻辑，但跳过 Render／Display，并排除在 SafeReturn 目标之外。它与结束当前 Widget 或 Passage 执行域的 `<<exit>>` Macro 不是同一概念。

## State

State 提供四个独立命名空间：

| Twee | State | 用途 |
|---|---|---|
| 普通名称 | `global` | Host 或 scripts 显式登记的全局能力 |
| `setup` | `setup` | 游戏配置与启动期共享值 |
| `$name` | `variables` | 游戏进度变量 |
| `_name` | `temporary` | 当前导航过程的临时变量 |

`@name` 与 `@args` 不属于 State，它们由 Macro 调用帧保存并在离开调用域后释放。

State 的名称表通过 `get`、`has`、`set`、`del` 等明确 API 访问。scripts 不会因为 ECMAScript `import` 或 `export` 自动进入 State；必须通过 Binding 调用 State API。

`StateCheckpoint` 捕获 global、setup、variables、temporary，用于 Engine 短期事务。`StateSnapshot` 只捕获可保存的 variables，并在恢复时清理 temporary；它是 Save 语义的内存基础，不是最终持久化格式。

`State.reset_game()` 清理 variables 与 temporary，保留由启动环境建立的 global 和 setup。

## Macro

Macro 拥有 Macro Definitions、Widget HIR 正文、调用上下文、`@` 局部变量和 `@args`。定义信息不进入 State，Macro 也不取得 State 或 Story 的所有权。

`MacroLogicContext` 只组合执行当前逻辑 Macro 必需的能力：

- State 的 global、setup、`$`、`_` 读写；
- Macro Local 的 `@` 与 `@args`；
- Story 的 has、include、goto 请求。

Surface、Audio、Resource 与 Mod 能力不进入逻辑 Context。

Widget 调用建立独立 `@args` 帧；嵌套 Widget 各自隔离。`exit` 在最近的 Widget 或 Passage 边界消费，break／continue 只由最近循环消费，goto 的 StopPassage 继续传给 Engine。

动态 Twee 语法仍由 Macro 拥有。`macro_runtime::parse_fragment()` 解析显式片段，`RuntimeExecutionContext::execute_parsed_fragment()` 复用相同 HIR 执行链。普通字符串中的 `<<name>>` 只是文本，不会自动再次解析。

完整 Macro 语法与实现状态见 [/docs/architecture/macro.md](/docs/architecture/macro.md)。

## Story

Story 拥有编译后的 Passage 查询、当前位置、history、导航分支和导航请求，不直接修改 State，也不执行 Macro。

- PassageName 和 Tag 查询区分大小写；
- `request_goto()` 只验证并建立请求；
- `confirm_navigation()` 才提交 history；
- `back()`／`forward()` 移动 history 游标，不创建新记录；
- 回退后 goto 会截断旧前进分支并建立新分支；
- history 项使用不会复用的 `StoryHistoryId`；
- `visits()` 从当前有效时间线计算，不维护第二份计数；
- `reset()` 清空时间线，但不触碰 State。

精确名称 `StoryInit` 是特殊初始化 Passage，不能通过普通 goto 进入。`[widget]` Passage 只提供 Widget Definitions，不产生正文输出。其他 Tag 是作者数据；Core 只提供精确查询，不写死 `town`、`indoor` 等游戏语义。

## Surface

Runtime 用 `BodyExecution` 同时返回 `BodyControl` 和有序 `Surface`。当前语义节点包括：

- Text、HardBreak，以及可组合的 StyledText（`TextStyle + TextColor`、可见延迟 `delay`、结构性 `heading`）；
- Image 与 Region（Header／Main／Footer／Bar／BarStowed／Dialog）；
- Container 与 Replace（稳定 Surface Key）；
- Component（capability、properties 与 fallback）与 Dismiss Action；
- 状态绑定 Input（checkbox／radiobutton／textbox）；
- Navigation 与 SafeReturn。

Native Twee 正文整体产生字面 Text，`$name` 与 `${expression}` 都没有自动求值特权。固有 `print` Macro 已显式产生动态或反引号字面 Text；include 与 Widget 的输出进入同一有序执行链。`silently` 使用隔离输出缓冲区执行正文，保留 State 副作用与 `goto`、`exit` 等控制信号，但丢弃该块产生的语义输出。

普通 Passage 完成后若没有作者 Navigation，Engine 可以追加指向最近安全 history 项的 SafeReturn。Core 只规定动作语义和目标；文字、布局以及按钮、链接、3D 对象或 TUI 选项等表现由 Host 决定。

Navigation 与 SafeReturn 携带 `InteractionId`。Host 回送身份而不是自行构造 Passage 目标；Core 只从上一份 Surface 中解析真实存在的动作。完整 Surface Protocol 仍按跨宿主稳定用例增量扩展。

## scripts、Callable 与 Prototype

Narrava Core 接受 `.ts` 与 `.js`，但不内嵌浏览器、Node.js 或固定 JavaScript 引擎。Core 负责脚本源码分流、Narrava Value、函数身份、State/Macro/Logger API 和 VM 调用边界；宿主 Binding 负责 TypeScript 转译、模块加载、真实函数对象、Promise 与平台安全策略。

```text
.ts / .js Source
→ ScriptBundle
→ Host ScriptBinding
→ 显式 Narrava API
→ Engine / State / Macro / Story / Logger / Event / Save / Resource / I18n
```

`ScriptCallable` 只包含 `id` 与调试名称；真实函数保存在 Binding 的 Callable Registry，不进入 Value 图、IR、Save 或 Core 注册表。脚本扩展 Narrava 原型时使用受控 Prototype Registry，不修改宿主语言自身的原型对象。

### 启动加载

`ScriptBundle::from_sources()` 只收集 TypeScript 与 JavaScript，保留 Source 顺序、相对路径、语言和源码文本；空 Bundle 合法。Binding 实现 `ScriptBinding::load(bundle, context)`，`ScriptLoadContext` 按宿主实际配置开放：

- `state()`：完整的受控 State API；
- `global_set()`／`global_extend()`／`global_function()`：导入普通全局或函数；
- `macro_api()`：可选的 Macro 增删查改与 Hook；
- `logger()`、`events()`、`resources()`、`i18n()`：可选的结构化能力；
- `Save` 由 Binding 在运行期提供控制器对象，不导入 `State.variables`。

ECMAScript `export/import` 只组织脚本模块，不会自动进入 Twee；脚本必须通过 State API 显式导入：

```ts
State.global.set("gameTitle", "Forest")
State.global.extend({ difficulty: 2, author: "Author" })
State.global.set("sum", sum)
```

### State 与 Expression

脚本和 Twee 使用同一份 Rust `State`。Tauri Boa Binding 的 `State.*` 是原生 Host operation：每次 get/has/set/del/setup 都直接访问当前 Engine 调用所借用的 Rust State，不在 JavaScript 中保存命名空间镜像，也不在函数或 Macro 完成后全量回灌。命名空间对应关系：

| Twee / scripts 概念 | Core 所有者 |
| --- | --- |
| 普通导入名 | `State.global` |
| `$name` | `State.variables` |
| `_name` | `State.temporary` |
| `setup` | `State.setup` |
| `@name`、`@args` | 当前 Macro Local，不属于 State |

`ScriptRuntimeContext` 组合借用 State 与 `ScriptFunctionHost`，因此 VM 无需知道 JavaScript 引擎。导入的 `ScriptCallable`：`typeof` 返回 `function`，可由普通调用表达式执行，参数与返回值均为 Narrava `Value`，调用可通过同一上下文修改 State；没有 Binding 的只读求值明确返回 `MissingWriteContext`，Binding 调用失败映射为 `expression.script_call_failed`。普通脚本函数调用是同步 Expression 能力；Promise 不允许伪装成普通 Value，异步工作应注册为 `MacroExecutionKind::Async`，走已有 suspension/resume/cancel 事务链。

### 文本与动态 Macro

脚本函数返回字符串 `"<<notice>>"` 时它仍是普通文本：`<<print abc()>>` 输出该字符串，不进行第二次 Macro 解析；`<<run abc()>>` 只执行副作用，不显示返回值。需要解析动态 Twee 时，Binding 必须调用 Macro/Compiler 明确提供的解析入口；不能让任意字符串在输出阶段自动变成代码。这一规则避免翻译文本、玩家输入或 Mod 数据意外获得执行权限。

### Macro API

`ScriptMacroApi` 复用 Core 的 `MacroDefinitions` 与 `MacroLifecycleSubscriptions`，提供 `add/update/del/get/has` 与 `before/after/off`。定义包含三个维度：`MacroBodyKind::Inline|Container`、`MacroArgumentKind::Raw|ArgumentList`、`MacroExecutionKind::Sync|Async`。Handler 与 Hook 都保存 `ScriptCallable`；before 可修改当前调用帧的 `@args`，after 接收并替换该 Macro 的隔离语义输出。`if`、`set`、`for`、`while` 等编译器固有逻辑不允许注册 Hook。Async Macro 使用现有不透明 Pending 句柄，Binding 把 Promise 映射为 resume/cancel，不把 Promise 塞进 Core。

### Logger 与错误

加载期可选注入 `Logger`，脚本写入普通 `LogEvent` 或附带 `Diagnostic` 的事件。边界错误保持稳定分类：源码编译与模块错误归 Binding；Expression 调用错误归 `EvalError`；Macro 定义、正文形态、同步/异步违规与 Hook 错误归 Macro Diagnostic。Binding 可以保留更详细的引擎堆栈，但不能让平台异常对象穿过 Core 公共类型。

### 完成边界

Core Script 契约已闭合：源码分流、加载契约、批量 State 导入、不可保存函数身份、Expression 调用、Macro CRUD、生命周期 Hook、同步/异步所有权，以及 Engine、Story、Logger、Event、Save、Resource、I18n 边界均有稳定类型和测试。ECMAScript 执行由共享 `narrava-loom-script` crate 提供（Boa 引擎 + Oxc 去除 TypeScript 类型），Tauri 与 TUI 复用同一运行时，并把真实 Function、Promise Macro、Host delay、State、Save、Resource、I18n 与事件桥接到各自 Host。作者侧声明在 `bindings/typescript/narrava.d.ts`；后续平台能力必须继续沿用同一窄 Binding 边界，不能让 DOM、Tauri 对象或 JavaScript 引擎类型进入 Core。

## I18n、Mod、Resource、Save 与 Event

- I18n 从稳定文本 ID 解析当前语言文本，并回退到 `default_locale` 原文；
- ModLoader 管理模组启停、排序、依赖和有效构建事务；
- ModUtils 是模组可用的受控工具集合；
- Save 保存可持久化 State、Story 时间线及游戏兼容元数据；
- Event 发布已发生的结构化事实，不保存领域状态。

### Resource 契约

Core 只拥有 Resource 的逻辑路径、字节、媒体类型、完整性和生命周期；URL、Blob、解码、DOM 与缓存属于 Host。逻辑路径使用 `/`，拒绝绝对路径、空段、`.`、`..`、反斜杠和重复路径；Core 不按扩展名拒绝资源，媒体类型由显式值或受控扩展表推断，未知时为 `application/octet-stream`。`ResourceCatalog::discover()` 只读取路径、媒体类型和文件大小；`read()`/UTF-8 `text()` 首次访问单个磁盘文件时才读取并缓存成功结果，I/O 错误不会伪装成“资源不存在”。`.nar`/`.nres` 的内存 backing 使用共享不可变字节，跨 Host adapter clone 不复制整份内容；返回字节不允许调用者修改目录内部数据。`.nar` 对清单、源码、Bytecode 和每个资源分别校验哈希。基础 Core 只有 `game` 来源；模组来源与覆盖顺序由 ModLoader 组合。

### Event 契约

Event 使用稳定序号、名称和拥有型 `Value` 载荷。`emit` 先保存事实，再投递给当时存在且匹配的订阅；`subscribe` 返回进程内稳定 ID；`take` 一次性取走待处理事件；`unsubscribe` 释放订阅及队列；`clear` 清空历史与队列但不重置序号。ScriptCallable 等不可拥有平台函数的数据不得作为跨边界事件载荷。Tauri Host 把五阶段 Passage 生命周期发布为保留事件 `passage:init/start/render/display/end`，统一载荷为 Passage 名和 tags，作者不能伪造保留名。

I18n 已完成文本目录、NMSG／字典 JSON、`.nlang` 导入导出、自动 fallback、Runtime 替换和稳定 Diagnostic。Save 已能捕获 `$variables` 与 Story 时间线、编码 JSON 并原子恢复。ModLoader 已移出 Core，独立 `narrava-loom-modloader` 当前只保留单向依赖边界；Story／Resource 模组合成属于该附属项目，不计入 Core 完成度。详细边界见 [/docs/architecture/i18n.md](/docs/architecture/i18n.md)、[/docs/architecture/save.md](/docs/architecture/save.md) 与 [/docs/architecture/modloader.md](/docs/architecture/modloader.md)。

有效构建顺序固定为先 I18n、后 `.nmod`：模组清单和依赖可以预先验证，但模组内容修改只作用于当前语言已经修正后的候选内容。

## VM 与部署

当前 Narrava VM 只接收由 LIR 编码的 Bytecode，只拥有执行帧、指令位置和暂停状态。State、Macro Definitions、Story history、资源缓存和 Host 表现状态由各自领域拥有。

Rust 原生库和 WASM 都是 Core 的部署形式；它们不改变 VM 的语义，也不意味着 Core 依赖浏览器、DOM 或 JavaScript 引擎。Bytecode 可进一步形成拥有型发布编码，但不是 WASM 的替代品。

## 当前下一步

`src/lib.rs` 已建立可嵌入 Core。Host API 已支持启动、验证玩家动作、异步 Resume／Cancel、取得 Surface 输出和只读访问 State，并统一以 Diagnostic 返回失败。执行链身份由 Runtime 的 `RuntimeExecutionIdentity` 所有，`RuntimeExecutionLocation` 将它与 `MirExecutionPosition` 组合；Host 令牌只传递身份，不携带 VM frame、检查点或平台句柄。

`RuntimeMacroContinuation` 已把执行身份、停在 `InvokeMacro` 的完整 `MirExecutionFrame` 与 `MacroSuspension` 绑定为一个 VM 级所有权单元。构造时同时验证执行链身份和 MacroPending 位置；失败会返还 frame 与 suspension，不丢失调度句柄或局部作用域。它尚未拥有 Engine 检查点、Passage 生命周期和待确认导航，因此不能单独成为 Host Resume 输入。

`RuntimeMacroContinuation::resume()` 已接通 Handler 恢复。恢复回调会显式取得 suspension 保存的平台句柄，不能再依赖闭包侧信道猜测当前任务。Complete 会退出当前 Macro 调用帧，把隔离输出写回原 `InvokeMacro` 并精确推进一次，同时交还控制信号、include 消耗与外层局部作用域；再次 Pending 会用回调返回的新句柄重建 continuation，VM 位置保持不变。Handler 或 VM 完成失败会连同未继续的 frame 返回。

`StoryRuntimeRequests` 原本借用活动 Story，不能跨异步等待保存。现在可通过 `into_pending()` 转为独立拥有的 `StoryRuntimePending`，其中保留 include 队列、goto 请求与所属编译结果；`from_pending()` 只允许重新附着到同一 Story，失败会原样返还请求。该类型将作为 Engine continuation 的待确认请求组件。

`EngineMirContinuation` 已组合 `RuntimeMacroContinuation`、StateCheckpoint、StorySnapshot、`StoryRuntimePending` 与 `EngineMirProgress`。Progress 保存当前 Passage、已确认入口链、此前输出、隔离复制的入口参数、已执行 Passage 数、Macro/HIR include 消耗和预算；VM include 仍由 frame 自己统计，避免恢复后重复计数。当前支持取消等待，也能重新附着 pending Story 请求并恢复 Handler。再次 Pending 会重建完整 Engine continuation；Complete 形成 `EngineMirResumedTransaction`，继续持有检查点并可回滚。

`EngineMirResumedTransaction::continue_vm()` 已校验 Macro 控制信号与合并后的 include 预算，并继续原 frame，直到 Halt、NavigationPending、下一 MacroPending 或 StopPassage。所有失败都返还装箱的完整事务，仍可由上层回滚。

Halted 边界已可通过 `commit_halted()` 提交。提交前拒绝遗留 include 或意外 goto，记录导航语义，按同步 Engine 的规则补 SafeReturn，并执行 Render／Display；成功后合并此前导航链输出并返还外层 Macro scopes。请求或生命周期失败会恢复 StateCheckpoint 与 StorySnapshot。

NavigationPending 与携带 goto 请求的 StopPassage 已由 `continue_navigation()` 接通：执行当前 Passage End，确认 Story 请求，清空 temporary，执行目标 Init／Start，建立新 VM frame，并继续到下一个稳定边界。Passage 预算沿用原事务，Macro/VM include 预算在新 Passage 重置；任何请求、生命周期、确认或 MIR 映射失败都会回滚整条链。

恢复链遇到的下一 MacroPending 已可通过 `dispatch_macro()` 重新进入控制器。回调接收原 `HirMacro`、同一 `RuntimeExecutionIdentity`、State、重新附着的 Story 请求和外层 scopes；同步 Complete 会把输出交回 VM 并继续运行，Pending 会把新 `MacroSuspension` 重新封装为完整 `EngineMirContinuation`。错误会保留可回滚事务或无效 suspension 的全部所有权。

Binding 通常不需要逐个消费上述边界。`HostApi::drive_stable()` 会循环处理后续 Macro、导航和 Halted 提交；`resume_and_drive()` 再把当前异步 Handler 的恢复纳入同一次调用。两者只在得到可呈现的 `HostUpdate` 或新的异步执行令牌时返回，平台 Pending 句柄始终由后端容器持有，不进入 Surface 或前端。

执行帧为每个 `MirIteratorSlot` 保存独立状态。集合 Prepare 建立键或值快照，暂停期间替换原 State 集合不会改变当前迭代顺序；range 保存 current、end、step 与错误位置，每次 NextIteration 只推进一个值。

include 会先推进调用者位置，再压入目标 `MirPassageFrame`；被包含 Passage Halt 后弹栈并继续调用者，整条链共用同一 Surface。goto 不压入 Passage，而是保存导航目标并进入稳定 `NavigationPending`；后续单步不会执行 goto 后正文或重复求值目标，等待 Engine 消费后以新的导航事务继续。

MIR silently 不使用运行时开关栈。每个可能产生输出的指令携带 Visible／Suppressed 属性，静默 include 的子帧继承 Suppressed；因此控制跳转不会把静默状态泄漏到块后正文。表达式和 State 副作用仍执行。当前 ExitPassage 结束最近的 Passage/include 帧：被 include 的 Passage exit 后返回调用者，根 Passage exit 等同于当前根帧完成。

动态 Macro 已进入 MIR 的 `InvokeMacro`，但指令不绑定 Definition。VM 到达该指令时返回稳定 `MacroPending`，当前位置不前进，并可通过 `pending_macro()` 读取原调用。`RuntimeExecutionContext::execute_macro_with_includes()` 复用现有 Definition 查询、参数准备、`@args`、Widget／Native、before／after 与 HIR include，返回隔离输出及 include 消耗量；因此被包含 Passage 的输出仍属于当前 Macro，并会进入 after。Engine 成功后用 `complete_macro()` 合并输出并只推进一次。`Continue` 恢复 VM，`StopPassage` 回到既有 Story 请求确认；预算超限或泄漏的循环内部控制信号会使整笔事务回滚。Engine continuation 与 Host Resume 已在上述统一驱动入口闭合。

同步 MIR 导航链现已由 Engine 提交：State 修改、goto history 与跨 Passage 输出一起成功；VM 中途失败或 include 超出预算时，State 与 Story 一起恢复。Host 已接通 Widget 注册顺序、同步 StoryInit、MIR 启动、玩家 Interaction、Cancel、Resume、VM 续跑、导航生命周期、后续 Macro 分派、自动稳定驱动与 Halted 提交。`link` 容器正文的捕获、异步恢复、后续 Macro 与目标 Passage也已贯通；I18n 语言选择复用同一事务所有权。

VM 使用默认语言 `step()` 与统一目标语言 `step_with_runtime_language()`。`I18nRuntimeLanguage::select()` 根据默认语言、玩家首选语言和无序安装包建立可选 fallback 链；Engine 把选择复制进 `EngineMirProgress`，随异步 Macro continuation 与后续 Passage 一起延续，Host 启动和玩家导航显式转交同一选择。语言不属于 State。重新编译或模组切换有效构建后必须重新绑定目录，错误语言对象形成可回滚的 VM 错误。

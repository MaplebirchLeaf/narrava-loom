# Narrava Script Binding

> 状态：Core 脚本契约已完成，等待首个 Tauri Binding
>
> 更新日期：2026-08-22

## 1. 定位

Narrava Core 接受 `.ts` 与 `.js`，但不内嵌浏览器、Node.js 或固定 JavaScript 引擎。Core 负责脚本源码分流、Narrava Value、函数身份、State/Macro/Logger API 和 VM 调用边界；宿主 Binding 负责 TypeScript 转译、模块加载、真实函数对象、Promise 与平台安全策略。

```text
.ts / .js Source
→ ScriptBundle
→ Host ScriptBinding
→ 显式 Narrava API
→ Engine / State / Macro / Story / Logger / Event / Save / Resource / I18n
```

`ScriptCallable` 只包含 `id` 与调试名称。JavaScript Function 不进入 Value 图、IR、Save 或 Core 注册表；`ScriptFunctionHost` 根据句柄找到真实函数并转换输入输出。这样同一 Core 可嵌入 Tauri、Web、Godot 或原生程序。

## 2. 启动加载

`ScriptBundle::from_sources()` 只收集 TypeScript 与 JavaScript，保留 Source 顺序、相对路径、语言和源码文本。空 Bundle 合法。

Binding 实现 `ScriptBinding::load(bundle, context)`。`ScriptLoadContext` 可按宿主实际配置开放：

- `state()`：完整的受控 State API；
- `global_set()`：导入一个普通全局名称；
- `global_extend()`：批量导入，返回新增与覆盖数量；
- `global_function()`：导入一个已在 Binding 函数表登记的函数；
- `macro_api()`：可选的 Macro 增删查改和 Hook；
- `logger()`：可选的结构化 Logger；
- `events()`：可选的结构化事实总线；
- `resources()`：可选的基础游戏 Resource 目录；
- `i18n()`：可选的当前语言、默认语言与目录视图。

`Save` 是 Binding 在运行期提供的控制器对象，不导入 `State.variables`。它把
`export`／`import` 转为 Core 请求，由 Host 完成真实 I/O；对应声明位于
`bindings/typescript/narrava.d.ts`。

ECMAScript `export/import` 只组织脚本模块，不会自动进入 Twee。脚本必须通过 State API 显式导入：

```ts
State.global.set("gameTitle", "Forest")
State.global.extend({ difficulty: 2, author: "Maple" })
State.global.set("sum", sum)
```

上例是 Binding 对外呈现的脚本形态；Rust Core 对应 `global_set`、`global_extend` 与 `global_function`，不规定 JavaScript 包装对象如何实现。

## 3. State 与 Expression

脚本和 Twee 使用同一份 Rust `State`。Tauri Boa Binding 的 `State.*` 是原生 Host
operation：每次 get/has/set/del/setup 都直接访问当前 Engine 调用所借用的 Rust State，
不在 JavaScript 中保存命名空间镜像，也不在函数或 Macro 完成后执行全 State JSON 回灌。
单个跨语言值仍需要做边界转换；这与复制整个 State 是两件事。

命名空间对应关系：

| Twee / scripts 概念 | Core 所有者 |
| --- | --- |
| 普通导入名 | `State.global` |
| `$name` | `State.variables` |
| `_name` | `State.temporary` |
| `setup` | `State.setup` |
| `@name`、`@args` | 当前 Macro Local，不属于 State |

`ScriptRuntimeContext` 组合借用 State 与 `ScriptFunctionHost`，因此 VM 无需知道 JavaScript 引擎。导入的 `ScriptCallable`：

- `typeof` 返回 `function`；
- 可由普通调用表达式执行，例如 `<<print sum(2, 3)>>`；
- 参数与返回值均为 Narrava `Value`；
- 调用可以通过同一上下文修改 State；
- 没有 Binding 的只读求值会明确返回 `MissingWriteContext`；
- Binding 调用失败映射为 `expression.script_call_failed`。

普通脚本函数调用当前是同步 Expression 能力。Promise 不允许伪装成普通 Value；异步工作应注册为 `MacroExecutionKind::Async`，走已有 suspension/resume/cancel 事务链。

## 4. 文本与动态 Macro

脚本函数返回字符串 `"<<notice>>"` 时，它仍是普通文本：

- `<<print abc()>>` 输出该字符串，不进行第二次 Macro 解析；
- `<<run abc()>>` 只执行表达式及其副作用，不显示返回值；
- 需要解析动态 Twee 时，Binding 应调用 Macro/Compiler 明确提供的解析入口；不能让任意字符串在输出阶段自动变成代码。

这一规则避免翻译文本、玩家输入或 Mod 数据意外获得执行权限。动态解析的权限、诊断和预算必须由显式 API 管理。

## 5. Macro API

`ScriptMacroApi` 复用 Core 的 `MacroDefinitions` 与 `MacroLifecycleSubscriptions`，提供：

- `add(name, definition)`：新增或替换，并返回旧定义；
- `update(name, definition)`：只更新已有定义；
- `del(name)`、`get(name)`、`has(name)`；
- `before(name, hook)`、`after(name, hook)`、`off(id)`。

定义明确包含三个维度：

- `MacroBodyKind::Inline` 或 `Container`；
- `MacroArgumentKind::Raw` 或 `ArgumentList`；
- `MacroExecutionKind::Sync` 或 `Async`。

Handler 与 Hook 都保存 `ScriptCallable`，真实函数仍归 Binding。before 可修改当前调用帧的 `@args`；after 接收并替换该 Macro 的隔离语义输出。`if`、`set`、`for`、`while` 等编译器固有逻辑不允许注册 Hook。Async Macro 使用现有不透明 Pending 句柄，Binding 把 Promise 映射为 resume/cancel，不把 Promise 塞进 Core。

## 6. Logger 与错误

加载期可选注入 `Logger`，脚本写入普通 `LogEvent` 或附带 `Diagnostic` 的事件。Logger 只保存结构化记录，不决定控制台、Tauri 菜单或游戏内日志面板如何显示。

边界错误保持稳定分类：源码编译与模块错误归 Binding；Expression 调用错误归 `EvalError`；Macro 定义、正文形态、同步/异步违规与 Hook 错误归 Macro Diagnostic。Binding 可以保留更详细的引擎堆栈，但不能让平台异常对象穿过 Core 公共类型。

## 7. 完成边界与后续

Core Script 内容现已闭合：源码分流、加载契约、批量 State 导入、不可保存函数身份、Expression 调用、Macro CRUD、生命周期 Hook、同步/异步所有权，以及 Engine、Story、Logger、Event、Save、Resource、I18n 边界均有稳定类型和测试。`ScriptStoryApi` 直接复用 Core Story 查询语义；Engine 和 Save 的平台动作通过窄 Host trait 实现。

作者侧使用 `Engine.restart()` 原子重置 State、Story 历史并重新执行启动流程；`StoryRestart` 不作为方法名或特殊 Passage 名。

游戏作者侧的对应声明位于 `bindings/typescript/narrava.d.ts`，公开 `Engine`、`State`、`Macro`、`Story`、`Logger`、`Event`、`Save`、`Resource` 与 `I18n`。`ModLoader`、`ModUtils` 和 Renderer/Audio 不属于 Core，不在此声明中。

首个 Tauri Binding 已使用 Boa 执行 ECMAScript，以 Oxc 去除 TypeScript 类型，并把真实
Function、Promise Macro、Host delay、State、Save、Resource、I18n 与事件桥接到 Runtime Worker。
这些实现位于 `hosts/narrava-loom-tauri`，不改变 Core 的宿主中立接口。后续平台能力必须继续
沿用同一窄 Binding 边界，不能让 DOM、Tauri 对象或 JavaScript 引擎类型进入 Core。

## 8. 当前示例

`examples/contents/scripts/main.ts` 是不含 Rust 的作者侧输入。Core 测试继续使用最小假 Binding
验证宿主中立契约，Tauri 集成测试则真实转译并执行同一类 TypeScript、函数调用、Macro 和异步恢复。

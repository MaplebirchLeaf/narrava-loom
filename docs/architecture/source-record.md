# Narrava Loom 源码记录

> 状态：随源码持续同步
>
> 更新日期：2026-08-22

本文只记录已经存在的源码与边界，不保存逐轮开发历史。

## Crate 入口

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | 宿主无关 Core 的 library 入口 |
| `src/main.rs` | 使用 `narrava_loom_core` library 的最小 Rust CLI Host |
| `src/config.rs` | 读取并验证 `config.toml` |
| `src/source.rs` | 发现、规范化并读取 Core Source |

CLI 不是 Core 的所有者。Compiler、Runtime、Engine 与状态模块都由 library 提供，其他 Rust Host 可以不经过 CLI 直接嵌入它们。

## 编译与执行模块

| 模块 | 职责 |
|---|---|
| `twee/` | Twee 词法、AST 解析、Fragment 解析与 Story 聚合 |
| `hir/` | Twee AST 到 HIR 的 lowering 与 Widget 静态校验 |
| `expression/` | Expression AST、Parser、Evaluator、Value 与 Prototype |
| `macro_runtime/` | Macro 定义、参数、调用帧、异步暂停与动态 Fragment 入口 |
| `mir.rs` | HIR 到显式控制流的中层指令、值／迭代槽、Passage 身份与执行位置 |
| `lir.rs` | MIR 到 VM 程序的 Passage 索引、重复名称与指令地址验证 |
| `bytecode.rs` | LIR 到可序列化拥有型 Bytecode；含格式校验、入口表、操作数与常量目录 |
| `runtime/` | HIR 节点执行、逻辑控制、Widget、include 与执行链身份 |
| `vm.rs` | Bytecode 位置、值／迭代状态、Passage 调用栈和累计 Surface 的单步执行帧 |
| `engine/` | 导航事务、Passage 生命周期、启动、新游戏及 MIR VM 事务适配 |

`State`、`Story` 与 Macro Definitions 分别保存各自领域的数据。Runtime 只借用这些能力；Engine 负责跨领域事务和回滚。VM 只接收 Bytecode，不反向拥有 State、Story 或平台对象；文件容器不进入运行时所有权。

`hir/lowering.rs` 只编排 Passage 与 Macro 分派。子模块按职责分为：`assignment.rs` 管赋值与动作 Expression，`control.rs` 管 `if`、`switch` 与 `for`，`fragment.rs` 管动态 Fragment 与 `print`，`source_map.rs` 管 Diagnostic 位置映射，`syntax.rs` 管 Macro 参数顶层扫描，`widget.rs` 管 Widget Definition 与 Story 级校验。迁移后不保留同职责副本；这些都是 HIR 内部模块，不增加编译阶段或对外 API。

## 支撑模块

| 文件 | 职责 |
|---|---|
| `src/state.rs` | global、setup、variables、temporary 与检查点 |
| `src/story.rs` | Passage 查询、导航时间线、history 与请求队列 |
| `src/surface.rs` | 当前最小宿主无关语义输出 |
| `src/host.rs` | Host 输入、Core 更新、最小启动／导航入口及统一 Diagnostic 边界 |
| `src/diagnostic.rs` | 稳定 Diagnostic 与源码定位 |
| `src/logger.rs` | 平台无关结构化日志 |
| `src/interpolation.rs` | 显式 Macro 参数的 `${expression}` 边界扫描 |
| `src/script.rs` | `.ts/.js` Script Bundle、加载上下文与宿主 Binding 契约 |
| `src/save/` | Save JSON、Value 引用图与 State／Story 原子恢复 |
| `hosts/narrava-loom-tauri/` | 共享 Tauri Worker、DTO 与 Renderer；桌面入口可运行，移动平台工程尚未初始化 |
| `bindings/typescript/narrava.d.ts` | 游戏 scripts 使用的宿主无关 TypeScript API 声明；不作为 Core Source 编译 |

## 当前有效边界

### Source

- Core Source 只识别 `.twee`、`.ts` 与 `.js`。
- `.css` 不是 Core Source；具体 Web／Tauri Host 可自行管理样式。
- `.ts/.js` 不进入 Narrative IR，而是形成有序 `ScriptBundle`，由宿主可选 `ScriptBinding` 加载。
- SourcePath 使用相对路径和 `/`，拒绝绝对路径、`..` 与反斜杠。
- `SourceList::discover()` 递归扫描 `contents/`，不跟随符号链接，并按保存路径稳定排序。

### Twee、Expression 与 Macro

- PassageName 区分大小写；跨 Source 重名会产生 Diagnostic。
- Twee 正文支持字面 Text、Macro 与 `/% %/` 注释；`${expression}` 在普通正文中没有特殊含义。
- Twee 正文是字面 Text；动态求值使用显式 Macro。
- Expression 负责值、运算符、成员访问、调用和内置原型语义。
- `$`、`_` 分别进入 State.variables、State.temporary；`@` 与 `@args` 属于 Macro 调用帧。
- Macro 拥有动态 Twee Fragment 的解析与执行入口；普通字符串不会自动重新解析为 Macro。
- `[widget]` Passage 只注册 Widget 定义，不产生可见文本；Twee Widget 不允许重名覆盖。

### Runtime、Engine 与 Story

- `BodyExecution` 同时返回控制信号和有序语义输出。
- include 在原节点位置执行，不创建导航 history；goto 由 Engine 请求并确认。
- Passage 生命周期为 Init、Start、Render、Display、End。
- `StoryInit` 在首次启动和新游戏前执行逻辑初始化，不建立 history，也不向 Host 输出正文。
- Engine 事务同时检查并恢复 State 与 Story，失败不会留下半提交导航。
- Story history 使用稳定 ID 和位置游标，支持 back、forward 与分支截断。
- 普通 Passage 没有作者导航动作时，Engine 可以追加 `SafeReturn` 语义。
- `[exit]` Passage 执行逻辑但跳过 Render／Display，并排除在安全返回目标之外。

### Surface 与 Host

- Core 产生宿主无关语义节点：Text、StyledText（`TextStyle + TextColor`、可选 `delay`）、Image、Region、Container、Component、Replace、Action、状态绑定 Input、Navigation 与 SafeReturn。
- Native 正文整体产生字面 Text；`$name` 与 `${expression}` 不自动求值，动态 Text 使用显式 `print` Macro。
- 旧的 Twee AST／HIR Interpolation 正文通道已移除；`${...}` 边界扫描器仅保留给 Interaction 等显式 Macro 参数。
- Navigation 与 SafeReturn 携带 `InteractionId`；Host 只能激活上一份 Core 输出中存在的动作。
- Renderer、输入设备、布局、资源解码和平台事件循环属于 Host。
- Resource 的逻辑身份属于 Core，具体协议 URL、缓存与解码对象属于 Host。

### I18n 与 Mod

- `I18nCatalog::from_hir()` 已把连续可见 Text 与显式 `print` 整理为带受控 placeholder 的消息，并可生成可序列化 `I18nTemplate`；翻译仍发生在 Host 表现之前。
- I18n 译文输入已校验语言标签形状、未知消息、placeholder 完整性和动态字典引用；`.nlang` 内使用 `manifest.json`、`translations.nmsg` 与 `dictionary.json` 分离配置、消息和动态字典。
- I18n 已能用已验证译文、Runtime placeholder 值与动态字典解析最终文本；缺失译文明确回退默认语言，校验与解析共用同一模板语法。
- `MirStory` 已持有同源 I18n 目录，普通 Passage 的 Text／Print MIR 指令已附加稳定消息 ID；表达式 Print 同时记录对应 placeholder。
- VM 已将同一 I18n ID 的连续 Text／Print 聚合为一个 Surface Text，并保持 `silently`／静默 include 的输出抑制；目标语言 fallback 会穿过 Engine 暂停、恢复与导航事务。
- 发布结构使用 `languages/<locale>.nlang`。
- Mod 修改 Core 能理解的 Source、IR、资源身份与语义配置，不依赖某一种 Host Renderer。
- I18n 文本闭环、Save 文档／Host 请求边界与基础游戏 Resource 已实现；模组 Story／Resource 组合和玩家模组仍未实现，不能把语言切片描述成完整模组系统。
- 有效构建顺序已确定为先应用 I18n，再按显式顺序应用 `.nmod` 修改；提前读取 manifest／依赖不代表提前修改内容。

## 测试约定

- 所有 Rust 单元测试集中在 `src/tests/`，由 `src/lib.rs` 挂载。
- Expression Evaluator 测试位于 `src/tests/expression_evaluator/`：共享测试 Context 留在 `mod.rs`，行为按职责拆成七组。
- Runtime Macro 测试位于 `src/tests/macro_runtime/`：共享假实现与构造器留在 `mod.rs`，行为按职责拆成九组。
- 测试需要访问的内部项保持最小 `pub(crate)`，不因此扩大 crate 外 API。
- 公共函数和不直观的边界写简短职责注释；不为显而易见的语句重复注释。
- 大文件按真实职责拆分，拆分不改变行为或公共 API。
- 完整约定见 [/docs/development/code-style.md](/docs/development/code-style.md)。

## 可运行示例

- [/docs/author/guide.md](/docs/author/guide.md) 是游戏作者入口，按可操作顺序解释唯一示例，并明确区分已实现能力与后续能力；
- `cargo run -p narrava-loom-core -- examples` 检查唯一示例游戏的配置、Source 与 Start Passage；
- `examples/` 只保存作者侧配置、Twee、Script 与语言数据，不要求游戏作者编写 Rust；Host、Script Binding、I18n、ModLoader、Save 与 Logger／Diagnostic 的组合行为由工作区测试覆盖；
- `cargo run -p narrava-loom-tauri -- examples` 启动 Tauri Host 的桌面开发入口；
- 示例只展示已经实现的能力；Tauri Script／Save Binding 与 Renderer 使用真实实现，完整 ModLoader 不使用占位实现伪装完成。

验证命令不在架构快照中记录易过期的通过数量，统一见
[仓库命令](../development/commands.md)与[项目状态](../development/status.md)。

## Continuation 实现

异步 Macro、Interaction、Resume／Cancel、事务回滚和 VM continuation 的当前结构以源码与
[运行时架构](runtime.md)为准。本页只维护模块职责，不复制逐函数完成清单。

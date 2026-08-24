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
| `vm.rs` | Bytecode 位置、值／迭代状态、Passage 调用栈和累计 Presentation 的单步执行帧 |
| `engine/` | 导航事务、Passage 生命周期、启动、新游戏及 MIR VM 事务适配 |

`State`、`Story` 与 Macro Definitions 分别保存各自领域的数据。Runtime 只借用这些能力；Engine 负责跨领域事务和回滚。VM 只接收 Bytecode，不反向拥有 State、Story 或平台对象；文件容器不进入运行时所有权。

`hir/lowering.rs` 只编排 Passage 与 Macro 分派。子模块按职责分为：`assignment.rs` 管赋值与动作 Expression，`control.rs` 管 `if`、`switch` 与 `for`，`fragment.rs` 管动态 Fragment 与 `print`，`source_map.rs` 管 Diagnostic 位置映射，`syntax.rs` 管 Macro 参数顶层扫描，`widget.rs` 管 Widget Definition 与 Story 级校验。迁移后不保留同职责副本；这些都是 HIR 内部模块，不增加编译阶段或对外 API。

## 支撑模块

| 文件 | 职责 |
|---|---|
| `src/state.rs` | global、setup、variables、temporary 与检查点 |
| `src/story.rs` | Passage 查询、导航时间线、history 与请求队列 |
| `src/presentation.rs` | 当前最小宿主无关语义输出 |
| `src/host.rs` | Host 输入、Core 更新、最小启动／导航入口及统一 Diagnostic 边界 |
| `src/diagnostic.rs` | 稳定 Diagnostic 与源码定位 |
| `src/logger.rs` | 平台无关结构化日志 |
| `src/interpolation.rs` | 显式 Macro 参数的 `${expression}` 边界扫描 |
| `src/script.rs` | `.ts/.js` Script Bundle、加载上下文与宿主 Binding 契约 |
| `src/save/` | Save JSON、Value 引用图与 State／Story 原子恢复 |
| `hosts/narrava-loom-tauri/` | Tauri command DTO、常驻 Runtime Worker、桌面入口与 WebView Host |
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
- Native Text 没有 HTML 或 Markup 特权。
- Core 中没有 `Engine.View`、DOM、`div` 创建逻辑或旧 Renderer 类型；形似 HTML 的 Twee 内容只会成为普通 Text。
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

### Presentation 与 Host

- Core 当前只产生 Text、Navigation 与 SafeReturn 三种最小语义节点。
- Native 正文整体产生字面 Text；`$name` 与 `${expression}` 不自动求值，动态 Text 留给显式 `print` Macro。Core 不产生 HTML、DOM 或平台控件。
- 旧的 Twee AST／HIR Interpolation 正文通道已移除；`${...}` 边界扫描器仅保留给 Interaction 等显式 Macro 参数。
- Navigation 与 SafeReturn 携带 `InteractionId`；Host 只能激活上一份 Core 输出中存在的动作。
- Renderer、输入设备、布局、资源解码和平台事件循环属于 Host。
- Resource 的逻辑身份属于 Core，具体协议 URL、缓存与解码对象属于 Host。

### I18n 与 Mod

- `I18nCatalog::from_hir()` 已把连续可见 Text 与显式 `print` 整理为带受控 placeholder 的消息，并可生成可序列化 `I18nTemplate`；翻译仍发生在 Host 表现之前。
- I18n 译文输入已校验语言标签形状、未知消息、placeholder 完整性和动态字典引用；`.nlang` 内使用 `manifest.json`、`translations.nmsg` 与 `dictionary.json` 分离配置、消息和动态字典。
- I18n 已能用已验证译文、Runtime placeholder 值与动态字典解析最终文本；缺失译文明确回退默认语言，校验与解析共用同一模板语法。
- `MirStory` 已持有同源 I18n 目录，普通 Passage 的 Text／Print MIR 指令已附加稳定消息 ID；表达式 Print 同时记录对应 placeholder。
- VM 已将同一 I18n ID 的连续 Text／Print 聚合为一个 Presentation Text，并保持 `silently`／静默 include 的输出抑制；目标语言 fallback 会穿过 Engine 暂停、恢复与导航事务。
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
- `cargo run -p narrava-loom-tauri -- examples` 启动最小 Tauri 桌面 Host；
- 示例只展示已经实现的能力；Tauri Script／Save Binding 与 Renderer 使用真实实现，完整 ModLoader 不使用占位实现伪装完成。

## 当前验证

```text
cargo clippy --workspace --all-targets -- -D warnings   通过
cargo test --workspace --all-targets                    Core 641 个、Tauri Host 7 个通过
cargo test --manifest-path crates/narrava-loom-modloader/Cargo.toml --offline
                                                        独立附属 crate 边界通过（当前 0 项实现测试）
cargo run --quiet -p narrava-loom-core -- examples      CLI Host 成功建立 Story
git diff --check                                        通过
```

## 已闭合的 continuation 边界

- Native/scripts Async 首次调用已能立即完成或返回 suspension；`Engine::begin_mir_chain()` 已接通 Passage 进入、Init／Start 和首次 MIR 事务，Pending 可包装为完整 continuation；
- Async Native 恢复仅在最终完成时执行 after，再次 Pending 保留名称并替换平台句柄，失败会返还已清理当前帧的外层作用域；
- Host Resume／Cancel 输入只携带稳定执行令牌；`HostPendingExecutions` 已提供不覆盖、单次取走的 continuation 所有权容器；
- Host Cancel 已从 pending 容器原子取走 continuation 并执行 Engine 回滚，只向 Binding 返还平台 Pending 所有权；
- Host Resume 已处理原子取出、再次 Pending 归还、失败回滚与完成事务的不透明包装；
- `HostApi::continue_resumed()` 已把完成事务驱动到四类稳定边界，执行失败自动回滚；
- `HostApi::commit_halted()` 已将 Halted 边界经 Render／Display 提交为 HostUpdate，非 Halted 边界不会丢失；
- `HostApi::continue_navigation()` 已将导航边界经 End、确认、Init／Start 继续为下一稳定边界；
- `HostApi::dispatch_macro()` 已处理后续 Macro 的 Complete／Pending、容器归还、非法 suspension 句柄返还与失败回滚；
- `HostApi::drive_stable()` 已自动串联后续 Macro、导航和 Halted 提交，只向 Binding 返回 Ready 更新或 Pending 令牌；
- `HostApi::resume_and_drive()` 已把当前 Handler 恢复与后续稳定驱动收束为单次 Binding 调用；
- `HostApi::start_mir()` 与 `advance_mir()` 已让首次启动和玩家 Interaction 直接进入 MIR/VM，并支持首次 Macro Pending；
- `start_mir()` 已在 `Start` 前执行可选 `StoryInit`；两者共享启动前的 State／Story 检查点，取消异步 Start 会连同初始化修改一起回滚；
- `[widget]` Passage 先由 Macro Definitions 收集，MIR 仅为其保留空执行体；Host 集成测试已验证 StoryInit 能调用已注册 Widget，取消启动仍会回滚 Widget 的 State 修改；
- Core `link()` 已把准备完成的 `[[label|target]]` 转换为 Navigation 语义；`link_with_body()` 会把容器正文、目标与捕获值原子登记到 `MacroInteractions`；
- Host 集成测试已贯通 `Start link → Navigation → 玩家激活 → Forest Text + SafeReturn`，并验证重复 `start_mir()` 在进入 VM 前被拒绝；
- Macro 恢复回调已显式接收 suspension 的平台句柄，再次 Pending 时以新句柄替换，不能依赖外部隐式关联；
- Interaction Macro 的同步创建、激活与异步延迟正文 continuation 已闭合；Interaction label 作为运行时 Macro 参数，不伪造编译期正文 I18n 身份；
- Macro Local 已能把 `capture` 明确列出的可见 `@` 绑定保存为独立所有权值，并恢复为不携带原 `@args` 的隔离帧；
- MIR 已把词法 Capture 名称附到内部动态 Macro 指令，VM 在 MacroPending 位置可读取，不需要维护可能被跳转绕过的捕获栈；
- Engine 已在动态 Macro 分派时建立捕获 Value，并通过单一 `EngineMirMacroInvocation` 交给 Macro 控制器；Host 不解释或保存这些局部绑定；
- Widget 递归 Runtime 已执行 `HirCapture` 词法域，并通过 `MacroInvocation.captures` 把选定 Value 交给嵌套 Native／scripts Macro；未列出的局部变量与原 `@args` 不会泄漏；
- `MacroInteractions` 已按 `InteractionId` 独占保存延迟正文、目标和捕获值，提供显式增删查改、一次性 `take` 与批量清理；重复新增不会静默覆盖；
- `HostApi::take_macro_interaction()` 已按 Presentation ID、动作存在性与目标一致性完成验证，全部通过后才一次性取走正文；失败不会消费动作；
- `HostApi::advance_macro_interaction_mir()` 已在同一 Engine 检查点中执行同步延迟正文并进入目标 Passage；正文失败会恢复 State、Story 与 Interaction；
- `MirMacroBody` 已把容器 Macro 的延迟 HIR 正文降低为独立 MIR 可执行单元；它复用 Passage 指令语义但不伪装成 Story Passage，为异步恢复位置提供编译边界；
- `MirExecutionFrame::new_macro()` 与 `step_macro()` 已能执行独立正文，在动态 Macro 位置暂停，并由 `complete_macro_body()` 精确推进后续指令；独立正文暂不允许 `include`；
- `RuntimeMacroBodyContinuation` 已组合独立正文 frame 与 `MacroSuspension`，校验执行身份和暂停位置；完成时推进正文，再次 Pending 时保留位置并替换平台句柄；
- `EngineMacroInteractionContinuation` 已共同持有 Runtime continuation、编译正文、原 Interaction 与 State／Story 检查点；取消会回滚领域状态并返还平台句柄和动作所有权；
- `EngineMacroInteractionContinuation::resume()` 已重新附着 Story 请求；再次 Pending 保留完整事务，完成则交出已推进 Macro 指令的 `EngineMacroInteractionResumed`，恢复失败会回滚并返还 Interaction；
- `EngineMacroInteractionResumed::continue_vm()` 已通过 State 与 Macro Local 联合上下文驱动后续正文，稳定返回 Halted／MacroPending；导航、include、VM 或控制信号错误会整体回滚；
- `EngineMacroInteractionResumed::dispatch_macro()` 已分派正文中的后续动态 Macro；Complete 推进原指令，Pending 建立新的 Engine continuation，回调／suspension／VM 错误统一回滚；
- `EngineMacroInteractionTransaction` 已固定保存 params、执行预算、请求与检查点；`Engine::begin_macro_interaction_target()` 会在正文 Halt 后用同一执行身份和检查点启动目标 MIR Passage；
- Host 已用 `HostMacroInteractionPending` 将 InteractionId 与 Engine continuation 一起保存；Resume 再次 Pending 会原子归还，Cancel 会回滚并把动作放回原 ID；
- `HostApi::drive_macro_interaction()` 已把异步 Interaction 恢复后的正文续跑、后续 Macro、目标 Passage 与普通稳定驱动接成单一 Host 边界；
- Tauri Host 已接通常驻 Runtime Worker、Start／Interaction、serde IPC、桌面入口与最小 WebView；
- 当前 WebView 只映射 Text／Navigation／SafeReturn，具体 Renderer 仍属于 Tauri Host，不进入 Core；
- 下一切片接入 Script Host 的最小生命周期，再处理 Save Binding。

# Narrava Host 与 Presentation 边界

> 状态：Presentation V2 基础协议与 Tauri keyed Renderer 已实现
>
> 更新日期：2026-08-24

## 目标

Narrava Core 是可嵌入的叙事 Runtime/VM。它决定游戏状态、控制流和语义事件，不决定画面、布局或平台对象。同一份已构建游戏应能由 Rust、Godot、Tauri、Web、TUI、Python、Java 等不同宿主驱动。

## 五层职责

| 层 | 拥有 | 不拥有 |
|---|---|---|
| Core | Compiler、HIR/MIR/VM、Expression、Macro、State、Story、Engine、Save、I18n、Mod、Resource 身份、Diagnostic/Logger/Event | DOM、CSS、Godot Node、终端控件和平台 Renderer |
| Host API | Core 的启动、输入、推进、存取状态和语义输出契约 | 布局、绘制、平台事件循环 |
| Presentation Protocol | 平台无关的有序语义和稳定身份 | HTML 标签、CSS selector、具体控件类 |
| Binding | Rust ABI 与目标语言/引擎之间的类型、生命周期和错误转换 | 游戏规则、表现策略 |
| Host Renderer | 把语义映射为当前平台画面和交互，并把玩家动作送回 Host API | Core 状态真相和 Story 控制流 |

依赖方向保持单向：

```text
Game Source
→ Compiler
→ Core Runtime / VM
→ Presentation Protocol
→ Binding / Host Adapter
→ Host Renderer

Player Input
→ Host Renderer
→ Binding / Host Adapter
→ Host API
→ Engine / Story / State
```

Core 不反向调用 DOM、Godot、WebView、TUI 或其他平台对象。Binding 不重新解释 Story，也不保存第二份 Core 状态。

## Presentation 所有权

Presentation Protocol 只表达 Narrava 必须理解的跨宿主语义。当前 `presentation` 模块提供：

- 普通 Text，以及可组合的 `TextStyle + TextTone`（含可选 `delay` 毫秒延迟浮现）；
- Resource 逻辑路径 Image、Header/Main/Footer/Bar/BarStowed/Dialog Region；
- Navigation、SafeReturn 和不触发 Story 导航的 Dismiss Action；
- 状态绑定 Input（checkbox／radiobutton／textbox），receiver 与允许值由 Core 保留；
- 版本化 Component capability、纯数据 properties 与必须存在的 fallback；
- `PresentationKey` 与普通 Container，供 Host 在更新间复用或替换同一语义节点。

Twee 用 `<<slot "name">>...<</slot>>` 建立普通稳定 Key 容器，再用
`<<replace "name">>...<</replace>>` 替换它。`silently` 会丢弃整块 Presentation，因此其中建立的
slot 不可见也不可替换；需要空目标时直接写 `<<slot "name">><</slot>>`。

文本样式共 8 个：emphasis、strong、code、quote、marked、small、inserted、deleted；
语气是 0..=63 的色阶（对齐二进制边界：灰阶 0-7（白`1`→亮灰`2`→浅灰`3`→灰`4`→深灰`5`→暗灰`6`→黑`7`），光谱 8-63（红`8`→橙`16`→黄`24`→绿`32`→蓝`40`→紫`48`→深紫`56`→`63`，每色相 8 级））。StyledText 可携带
`delay`（毫秒，0..=86400000）：渲染器在此之前保持文本隐藏、到时浮现，以及结构性
`heading`（1 或 2）：表达文档层级（如弹窗页签的页面标题），不属于字形样式，Host 据此划分页面。
这些名称表达跨 Host 的内容意图，不暴露 HTML 标签、CSS class、selector 或具体颜色。
游戏专用视觉效果应由作者主题映射到合适语义，或声明专用 Component，不能成为 Core 的通用颜色 API。

Navigation 由 Core 识别，因此可以参与 Story、SafeReturn、I18n、Save、测试和 Mod 补丁。Host 可以把同一动作表现为按钮、文字链接、3D 物体或终端选项。Host 返回的输入必须引用 Core 提供的稳定动作身份或受验证目标，不能把任意平台回调直接注入 VM。

## 文本、I18n 与 Mod

Twee 正文进入字面 Text，动态 Text 由 `print` 等显式 Macro 产生；Host 决定字体、布局和
可访问性。I18n 在 Compiler/IR 中保存文本身份与 placeholder，并在 Host 渲染前完成翻译。

Mod 继续修改 Core 可理解的 Source、AST/IR、资源身份和语义配置。模组不得为了修改核心叙事而依赖某一种 Host；平台专用扩展必须单独声明能力，缺少能力时不能破坏基础 Story。

## Scripts 边界

`.ts/.js` 的 Core 边界已经闭合为可选 `ScriptBinding` capability：Core 提供 ScriptBundle、受控 API、函数句柄与 VM 调用协议，具体宿主决定是否提供 ECMAScript Runtime。没有 Script capability 的宿主仍可运行不含脚本的游戏；不同宿主不必使用同一种 JavaScript Runtime。

## 首个 Host：Tauri

首个真实 Host 选择 Tauri。Rust 后端把 Presentation 转为带 key 的 DTO；WebView 按 key
协调现有 DOM，不再在每次更新时清空 Passage。Renderer 将语义文本映射为原生 `em`、`strong`、
`code`、`q`、`mark`、`ins`、`del`、`small` 等元素，将 Region 路由到稳定插槽，将 Image 映射为
`figure/img/figcaption`。当前原生支持 `meter@1` Component；未知 capability/version 必须渲染
fallback。WebView 的 HTML、CSS 和 DOM 实现始终只属于 Tauri Host。

Godot 是第二个 Host，用于验证同一套语义能否映射到 Control、2D 或 3D，而不依赖 Web 布局。Tauri 验证完成前不提前实现 Godot Binding。

## Godot Host 方向

Godot 不复用 Tauri WebView，也不把 GDScript、Scene 或 Node 类型引入 Core。建议建立独立
`hosts/narrava-godot/` Binding：

```text
Narrava Core
→ HostUpdate / PresentationNode
→ Rust Godot Binding
→ NarravaRuntime Godot 节点
→ 游戏作者的 GDScript、Control、2D 或 3D 场景

Godot signal / 玩家输入
→ InteractionId
→ Rust Godot Binding
→ HostInput
→ Narrava Core
```

Binding 只负责 Rust／Godot 类型转换、对象生命周期和 Diagnostic 转换。一个
`NarravaRuntime` 节点可以提供 `start_game()`、`activate(interaction_id)` 和语义更新 signal；
游戏作者决定把 Navigation 映射为 `Button`、场景物体或其他交互。Godot Host 与 Tauri Host
并列依赖 Core，二者不得相互依赖，也不能各自保存第二份 State 或 Story 真相。

## 当前 Host 契约

1. 移除 CSS SourceKind、Style Bundle 和 Style Host Adapter 规划；CSS 只可存在于具体 Web/Tauri Host 项目。
2. 移除 Native Presentation 的 Markup/HTML 特权；普通正文保持字面 Text，显式动态 Macro 才产生求值后的语义 Text。
3. 将语义输出集中在 `presentation`，不在 Core 建立 Renderer/Adapter 对象。
4. 已建立 `src/lib.rs`：Core 模块由 library 拥有，`main.rs` 只作为最小 Rust CLI Host 使用该 crate。
5. 最小 Rust Host API 已落地：`HostApi::start()` 固定进入 `Start`；`HostApi::advance()` 消费一个 `HostInput` 和上一份 `HostUpdate`，先验证 `InteractionId` 再推进 Engine 事务。
6. Host API 与执行闭包统一使用 `Diagnostic` 作为失败边界。Host 将 Engine／Story 结构错误转换为稳定错误码，Runtime 已形成的 Diagnostic 原样保留；Binding 不依赖 Rust 内部事务枚举。
7. `HostApi::state()` 返回只读 `HostStateView`，分别读取 global、setup、`$variables` 与 `_temporary`。Binding 若需跨 FFI 或异步边界保存数据，必须显式转换，不能持有 Core 引用。
8. `HostInput::Resume` 与 `HostInput::Cancel` 只携带 `HostExecutionToken`。令牌来自 Runtime 执行身份，不包含 VM frame、State 检查点、Macro 局部域或平台句柄，适合跨 FFI 传输。
9. `MacroSuspension` 与 `EngineMirContinuation` 仍由 Core／Binding 的待处理执行容器持有，不能序列化到 Host 前端。同步 `HostApi::advance()` 明确拒绝 Resume／Cancel，避免在没有对应 continuation 时伪造恢复。
10. `HostPendingExecutions<Pending>` 已按 Token 独占保存 Binding 后端的待处理值。重复 Token 不覆盖旧执行并返还新值；`take()` 先原子移除再允许恢复或取消，同一 continuation 不能被消费两次。
11. `HostApi::cancel_pending()` 已将 Cancel 接到 Engine 回滚。成功只返还平台 Pending 所有权供 Binding 清理，不暴露 VM frame 或 Macro 局部域；未知 Token 不修改 State／Story，Story 回滚失败仍尽可能返还平台值。
12. `HostApi::resume_pending()` 已原子取出 continuation 并恢复当前 Handler。再次 Pending 自动归还同一 Token；Story 不匹配会保存原 continuation；恢复失败会回滚；完成后只返回不透明 `HostResumed`，其公开视图限于执行令牌和 Runtime 位置。
13. `HostApi::continue_resumed()` 已继续驱动完成事务到 Halted、NavigationPending、MacroPending 或 PassageStopped，并以 `HostStableBoundary` 暴露类别。内部 Engine 事务仍封装；VM、Story、预算或控制信号失败会回滚。
14. `HostApi::commit_halted()` 已只接受 Halted 边界，执行 Engine 的 SafeReturn、Render／Display 和事务提交，最终收束为 `HostUpdate`。其他边界会原样返还不透明 `HostStable`；提交失败沿用 Engine 回滚并转换为稳定 Diagnostic。
15. `HostApi::continue_navigation()` 已消费 NavigationPending／PassageStopped，执行当前 End、确认 goto、清理 temporary、执行目标 Init／Start，并返回新的 `HostStable`。非导航边界原样返还；导航、生命周期或后续 VM 失败按 Engine 事务回滚。
16. `HostApi::dispatch_macro()` 已消费后续 MacroPending。Complete 返回新的 `HostStable`；Pending 以同一 Token 存入容器；非 Macro 边界原样返还；非法 suspension 回滚并返还平台 Pending，其他分派或 VM 失败回滚。
17. `HostApi::drive_stable()` 已把后续 Macro、Passage 导航和 Halted 提交收束为一个 Binding 入口。它持续推进到 `HostDriveResult::Ready(HostUpdate)` 或 `Pending { execution }`；失败统一返回 Diagnostic，并在非法异步结果中返还平台 Pending 所有权。分步 API 保留为 Core 的明确边界，不要求各语言 Binding 自行编排 VM 状态。
18. `HostApi::resume_and_drive()` 已把当前 Handler 恢复、VM 续跑、后续 Macro、导航和提交合并为一次调用。Binding 不再手动串联三个内部阶段。
19. `HostApi::start_mir()` 固定从 `Start` 启动真实 MIR/VM；`advance_mir()` 只从上一份 Presentation 解析目标。两者均直接返回 Ready 或 Pending，首次异步 Macro 也由 Host 容器接管。
20. 作者 `link` 的最小游戏链已经贯通：MIR Macro 分派生成 Navigation，Host 回送 InteractionId 后进入目标 Passage，并取得目标正文与 SafeReturn。`start_mir()` 同时拒绝活动 Story 的重复启动，不会覆盖现有 continuation。
21. `start_mir()` 会先通过独立初始化回调执行可选 `StoryInit`，再进入 `Start` MIR。初始化与 Start 共用启动前检查点；若 Start 暂停后被取消，State 与 Story 会一起恢复到 StoryInit 之前。StoryInit 当前保持同步，不把平台异步任务塞进初始化阶段。
22. Widget Definitions 不由 Host 保存。启动方先由 Macro 的 `register_story_widgets()` 建立 Definitions，再把同一份 Definitions 用于 StoryInit 与后续 MIR Macro 分派。Host 集成测试已验证 Widget 注册、StoryInit 调用、异步 Start 和取消回滚的完整顺序。
23. 动态 Macro 分派使用 `EngineMirMacroInvocation`，把调用、Runtime 执行身份和 `CapturedMacroLocals<Value>` 作为一个明确输入交给 Macro 控制器。Host 不读取捕获内容，也不把它序列化到平台前端。
24. `MacroInteractions` 以 `InteractionId` 保存延迟正文、导航目标和捕获值。Presentation 仍只公开 Navigation 语义；Host 激活时回送 ID，Core 通过一次性 `take` 取得动作。旧 Presentation 对应的动作可由 Macro 控制器统一清理，不需要 Host 持有 HIR 引用。
25. 容器 `link` 通过 `link_with_body()` 原子建立 Navigation 与 `MacroInteraction`。重复 ID 不覆盖已有动作；Host 激活时由 Core 校验并一次性取出正文，在同一事务内执行后再进入目标 Passage。
26. `HostApi::take_macro_interaction()` 先验证 ID 出现在上一份 Presentation、动作确实存在且双方目标一致，最后才一次性取走动作。任一验证失败都不消费正文；Binding 不能自行拼装目标绕过 Core。
27. `HostApi::advance_macro_interaction_mir()` 恢复捕获作用域，在导航前执行同步容器正文，并把正文修改与目标 Passage 放入同一 State／Story 检查点。正文失败时 Interaction 也会恢复；正文自身的 `exit` 在该作用域消费，冲突的 `goto`、遗留 include 与循环控制信号会被拒绝。
28. 异步延迟正文同时保存平台句柄和正文指令位置。`MirMacroBody` 提供独立 lowering 边界；独立正文不执行 `include`，避免伪造 Passage 调用栈。
29. `RuntimeMacroBodyContinuation` 将 frame 与 `MacroSuspension` 绑定到同一执行身份；Engine continuation 再组合 State／Story 检查点、目标与 Interaction 恢复数据。
30. Engine Resume 重新附着保存的 Story 请求。再次 Pending 保留新句柄与完整事务；Handler／VM 恢复失败会回滚领域状态并返还原 Interaction。
31. `EngineMacroInteractionResumed` 继续正文并分派后续动态 Macro。正文 Halt 后进入目标 Passage；正文导航、遗留 include、VM 错误与越域控制信号都会触发回滚。
32. `EngineMacroInteractionTransaction` 固定保存点击时 params、执行预算、Story 请求和检查点；Host 恢复时不能替换它们。
33. 后续 Macro 的 Complete 合并输出并推进，Pending 建立新的正文 continuation；失败恢复点击前检查点。
34. Tauri ECMAScript 提供 `Host.delay(ms)`。它把 Promise 转为 Core Pending，Rust Worker 到期后用同一 Token Resume；无 Host 操作来源的未决 Promise 会报错。
35. 正文 Halt 后以原 Runtime 身份和同一检查点启动目标 MIR Passage，再由 Host 统一驱动到稳定边界。
36. `HostMacroInteractionPending` 将原 `InteractionId` 与 Engine continuation 一起隐藏在 Host 容器中。`resume_macro_interaction_pending()` 再次等待时原子归还新句柄；`cancel_macro_interaction_pending()` 恢复 State／Story、把动作放回同一 ID，并把平台句柄返还 Binding。
37. `HostApi::drive_macro_interaction()` 已自动续跑延迟正文、分派后续 Macro，并在正文 Halt 后进入目标 Passage，再复用普通 Host 稳定驱动。Binding 最终仍只处理 Ready 或 Pending；两个待处理容器与动作所有者由 `HostMacroInteractionDriveContext` 明确组合，不进入前端协议。

Host continuation、Widget 注册顺序、同步 StoryInit、`Host.delay()`，以及同步／异步 `link`
容器正文的基础链已经闭合。I18n 文本与 fallback 已穿过 Host 事务。TUI Renderer 已验证
Region、稳定 Key 替换与 Component fallback 不依赖 DOM；终端输入循环会保留允许值，并把玩家
命令验证为 Host-neutral 操作。完整游戏目录的 ECMAScript／Save／I18n 驱动仍待与 Tauri 共用。
ModLoader 仍不在本体阶段。

每一步必须保持 Compiler、Runtime 和现有逻辑测试可运行，不借迁移重写 VM。

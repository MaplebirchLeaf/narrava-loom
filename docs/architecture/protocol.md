# Narrava Protocol

Native Host 的生命周期编排正在收敛为 `RuntimeCommand → RuntimeSession → RuntimeUpdate /
PendingOperation`；本页只描述展示协议，Session 的所有权与阶段边界见
[Runtime Session](runtime-session.md)。

> 状态：`narrava-loom-protocol` 是零 Core 依赖的纯数据协议；Core 转换属于 Script Runtime 适配层
>
> 更新日期：2026-08-29

## 目标

Narrava Core 是可嵌入的叙事 Runtime/VM。它决定游戏状态、控制流和语义事件，不决定画面、布局或平台对象。同一份已构建游戏应能由 Rust、Godot、Tauri、Web、TUI、Python、Java 等不同宿主驱动。

## 当前模块边界

Protocol 只有一个物理 crate。跨语言稳定契约不依赖 Core，Rust 语义转换是 Script Runtime 的内部适配：

```text
narrava-loom-core ────────────┐
                              ├─→ narrava-loom-script::protocol_adapter
narrava-loom-protocol ────────┘                    │
              │                                    └─→ RuntimeSession
              └───────────────────────────────────────→ hosts/*（纯数据命令与更新）
```

独立 ModLoader 只依赖 Core 的游戏身份、源码、资源与模组契约，不消费 Surface Protocol。

Core 内部的 `semantic::SemanticOutput` 继续使用 `TextValue`，保持 Expression、State 与 Save
共享的 UTF-16 语义。跨 Runtime/Host 边界时，Script Runtime 的 `protocol_adapter` 才把文本转换为
拥有型 UTF-8 `String` DTO；孤立代理项会在边界处形成错误，不能泄漏到 IPC。脚本 `Surface`
builder 也由该适配层验证并转回 Core 语义输出。纯 Protocol crate 不认识 `TextValue`、Surface
builder 或 Core conversion，因此依赖保持单向。

## 五层职责

| 层 | 拥有 | 不拥有 |
|---|---|---|
| Core | Compiler、HIR/MIR/VM、Expression、Macro、State、Story、Engine、Save、I18n、Mod、Resource 身份、Diagnostic/Logger/Event | DOM、CSS、Godot Node、终端控件和平台 Renderer |
| Runtime Session | 单局启动、输入、推进、挂起、存取状态和语义输出编排 | 布局、绘制、平台事件循环 |
| Protocol | 平台无关的拥有型命令、更新、节点和稳定身份 | Core、HTML 标签、CSS selector、具体控件类 |
| Adapter / Binding | Core、Protocol 与目标语言之间的类型、生命周期和错误转换 | 游戏规则、表现策略 |
| Host Renderer | 把语义映射为当前平台画面和交互，并把玩家动作送回 RuntimeSession | Core 状态真相和 Story 控制流 |

依赖方向保持单向：

```text
Game Source
→ Compiler
→ Core Runtime / VM
→ Runtime Session
→ Protocol DTO
→ Host Renderer

Player Input
→ Host Renderer
→ Protocol Command
→ Runtime Session
→ Engine / Story / State
```

Core 不反向调用 DOM、Godot、WebView、TUI 或其他平台对象。Binding 不重新解释 Story，也不保存第二份 Core 状态。

## Surface 所有权

Surface 语义由 Core 定义，Runtime 将其转换为 `narrava-loom-protocol`
中的拥有型 `HostNodeDto`。当前节点包括：

- Text、HardBreak，以及可组合的 `TextStyle + TextColor`；
- Resource 逻辑路径 Image，以及开放的 `RegionId`；
- Navigation、SafeReturn 和不触发 Story 导航的 Dismiss Action；
- 状态绑定 Input（checkbox／radiobutton／textbox），receiver 与允许值由 Core 保留；
- 版本化 Component capability、纯数据 properties 与必须存在的 fallback；
- `SurfaceKey` 与普通 Container，供 Host 在更新间复用或替换同一语义节点。

Twee 用 `<<slot "name">>...<</slot>>` 建立普通稳定 Key 容器，再用
`<<replace "name">>...<</replace>>` 替换它。`silently` 会丢弃整块 Surface，因此其中建立的
slot 不可见也不可替换；需要空目标时直接写 `<<slot "name">><</slot>>`。

文本样式共 8 个：emphasis、strong、code、quote、marked、small、inserted、deleted；
`TextColor` 是 0..=63 的标准调色板索引：0-7 为默认色与灰阶，8-63 为红到深紫的色谱。
Protocol 不携带 CSS RGB 或 ANSI 值。StyledText 可携带 `delay`（毫秒，0..=86400000）：
到时前内容不可见。该字段不承诺动画；Host 可以直接显示，也可以自行选择 fade、opacity、
easing 或 tween。结构性
`heading`（1 或 2）：表达文档层级（如弹窗页签的页面标题），不属于字形样式，Host 据此划分页面。
这些名称表达跨 Host 的内容意图，不暴露 HTML 标签、CSS class、selector 或具体颜色。
游戏专用视觉效果应由作者主题映射到合适语义，或声明专用 Component，不能成为 Core 的通用颜色 API。

Navigation 由 Core 识别，因此可以参与 Story、SafeReturn、I18n、Save、测试和 Mod 补丁。Host 可以把同一动作表现为按钮、文字链接、3D 物体或终端选项。Host 返回的输入必须引用 Core 提供的稳定动作身份或受验证目标，不能把任意平台回调直接注入 VM。

## 文本、I18n 与 Mod

Twee 正文进入字面 Text，动态 Text 由 `print` 等显式 Macro 产生；Host 决定字体、布局和
可访问性。I18n 在 Compiler/IR 中保存文本身份与 placeholder，并在 Host 渲染前完成翻译。

`<br>` 是 Narrava Twee 保留语法，在 HIR lowering 时成为 `HardBreak`。它分隔 I18n 消息，
不进入 NMSG，也不允许译者增加、删除或移动。其他 HTML 标签没有特殊含义。

标准 RegionId 为 `main`、`header`、`footer`、`bar`、`bar-stowed`、`dialog`；非空自定义名称
同样有效。未知区域在 Tauri 回退正文，在 TUI 保留进自定义区域表，不能静默丢弃。

Mod 继续修改 Core 可理解的 Source、AST/IR、资源身份和语义配置。模组不得为了修改核心叙事而依赖某一种 Host；平台专用扩展必须单独声明能力，缺少能力时不能破坏基础 Story。

## Scripts 边界

`.ts/.js` 的 Core 边界已经闭合为可选 `ScriptBinding` capability：Core 提供 ScriptBundle、受控 API、函数句柄与 VM 调用协议，具体宿主决定是否提供 ECMAScript Runtime。没有 Script capability 的宿主仍可运行不含脚本的游戏；不同宿主不必使用同一种 JavaScript Runtime。

## 首个 Host：Tauri

首个真实 Host 选择 Tauri。Rust 后端把 Surface 转为带 key 的 DTO；WebView 按 key
协调现有 DOM，不再在每次更新时清空 Passage。Renderer 将语义文本映射为原生 `em`、`strong`、
`code`、`q`、`mark`、`ins`、`del`、`small` 等元素，将 Region 路由到稳定插槽，将 Image 映射为
`figure/img/figcaption`。当前原生支持 `meter@1` Component；未知 capability/version 必须渲染
fallback。WebView 的 HTML、CSS 和 DOM 实现始终只属于 Tauri Host。

## Host 契约

- Core 只产生 `semantic::SemanticOutput`；Script Runtime adapter 将其转换为拥有型 Protocol DTO，并以不透明 `InteractionId` 接收玩家动作；Renderer 不进入 Core。
- Host 只发送 `RuntimeCommand` 并接收 `RuntimeUpdate`；Core 的 `HostApi` 与 execution token 不越过 RuntimeSession。
- Resume／Cancel 只回传 Protocol 的不透明 operation ID。Continuation、检查点和作用域留在 RuntimeSession。
- State 与 Story 在同一事务中提交或回滚；容器 Macro 正文与目标 Passage 也属于同一事务。
- Tauri、TUI 与其他 Host 并列依赖 Core，彼此不得依赖，也不得保存第二份 State／Story。

当前完成度只在[项目状态](../development/status.md)维护，公开 API 以 Rust 文档和
[作者 API 速查](../reference/api-and-syntax.md)为准。

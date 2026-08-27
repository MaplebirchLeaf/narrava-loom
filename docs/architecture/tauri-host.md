# Narrava Tauri Host

> 状态：共享 Host 实现已建立；桌面可运行和发行，Android/iOS 工程尚未初始化
>
> 更新日期：2026-08-24

## 桌面与移动端当前状态

`narrava-loom-tauri` 是一个共享 Host crate，不是两个独立 Host。Rust Worker、DTO、Renderer、
默认主题和游戏 API 由桌面与移动端共同使用；区别只出现在 Tauri 平台入口、系统能力和打包层。

| 能力 | 桌面 | Android/iOS |
|---|---|---|
| Rust Worker、Core、DTO | 共用，已测试 | 共用，已测试 |
| Renderer 与响应式／触屏 CSS | 共用，已测试 | 共用，尚需真机验收 |
| 平台入口 | `src/main.rs`，可运行 | `run_mobile()` 已标注 mobile entry point |
| 平台工程与打包 | Linux/Windows/macOS 发行已接通 | 尚未初始化，无仓库级构建命令 |
| 完成状态 | 可开发、测试和生成可移动目录 | 不能宣称已交付 |

因此，`cargo run -p narrava-loom-tauri -- examples` 始终表示桌面开发入口。移动端后续要先初始化
Tauri 2 Android/iOS 工程，再处理权限、签名、资源、生命周期和真机 WebView；桌面测试不能替代
这些工作。

## 定位与数据流

Tauri 是首个平台 Host，不是 Narrava Core 的表现模型。Rust 后端直接依赖
`narrava-loom-core`，WebView 使用平台自己的页面能力显示宿主无关语义；Core 不产生 DOM 或
CSS。默认 CSS 由 Host 提供，游戏作者可以完全不写 CSS。

```text
Narrava Core Bytecode VM
→ HostUpdate
→ Tauri Host DTO
→ Tauri command
→ WebView Renderer
```

## 当前实现

独立 workspace crate 位于 `hosts/narrava-loom-tauri/`，Tauri 依赖不会进入 Core 或
ModLoader。`TauriHost::spawn(relative_game_path)` 建立专用 Runtime Worker：

- Worker 在线程栈中长期拥有 Source、AST、HIR、MIR、LIR、Bytecode、State、Story 与上一份输出；
- Tauri managed state 只保存可克隆的命令发送端，不持有自引用 IR；
- `start_game` command 启动固定 `Start` Passage；
- `activate` command 只接受上一份 Surface 中的 Interaction ID；
- Rust DTO 以 serde tagged enum 输出 Text、Navigation 与 SafeReturn；
- 错误统一输出稳定 `code + message`，不把 Rust 错误对象交给 WebView。

游戏 Worker 中可用的 TypeScript 契约位于 `bindings/typescript/narrava.d.ts`。

该 crate 包含桌面二进制入口、mobile entry point、`tauri.conf.json`、默认 CSS 和无框架
Renderer。页面只消费
`HostUpdateDto`：Text 成为文本节点，Navigation／SafeReturn 成为原生按钮，点击后把
Interaction ID 交回 `activate`。DOM 与 CSS 因此留在 Tauri Host，不进入 Core 或游戏 Source。

## Core 与 Host 的表现边界

Core 输出文本、标准颜色、图片、区域、动作、状态绑定输入、导航和带 fallback 的版本化
Component。Tauri 再决定 DOM、CSS、Dialog 布局和更新算法。

`replace "header"` 是 Core 固定的 Region／Key 替换语义。WebView 把固定区域映射到稳定 DOM
插槽，TUI 把它映射到自己的终端区域；Core 不保存 CSS selector。Radio 的互斥组同样是跨 Host
的输入语义：Core 提供 opaque group ID，WebView 映射为 HTML `name`，TUI 可映射为自己的
RadioGroup。

普通 Key 由 `slot` 容器显式建立。Tauri 把 slot 映射为无额外视觉样式的稳定 DOM 容器；空 slot
也会保留目标节点，随后由 `replace` 更新其子节点。

Runtime Worker 使用 Boa 执行游戏 `.js`，并先通过 Oxc 去除 `.ts` 类型；游戏脚本不会进入
WebView。默认 locale 在加载时注入脚本环境；Resource 只注入原生只读 adapter，启动配置不再
携带完整 bytes/text。Core Expression 可调用脚本函数，自定义同步 Macro 与仅依赖 ECMAScript
job queue 的 Promise Macro可直接完成。

Renderer 提供 `passage-header`、`passage-main`、`passage-footer`、`bar`、`dialog` 五个稳定插槽。
游戏根目录可选的 `styles/**/*.css` 按逻辑路径排序并在默认样式后加载；CSS 中的
`resource("path")` 由 Host 转换为受 CSP 限制的 `narrava-resource://` URL。WebView 启动时只
取得 Resource 元数据，真正字节由自定义协议按单个路径读取，不再经过 JSON/IPC/Blob。

即使游戏完全没有 CSS，Host 也会提供可直接交付的阅读界面：左侧固定 `bar`、可折叠控制栏、
最大宽度受限的正文画布、Header／Main／Footer 分区以及原生模态 Dialog。窄屏默认收起侧栏，
展开时会阻止误触正文。`styles/**/*.css` 是主题覆盖层，不是获得基础布局的必填文件；游戏作者
可以只改颜色和字体，也可以完全不提供它。

需要自定义主题时，稳定骨架如下：

```text
nv-story
├─ nv-ui-bar#ui-bar
│  ├─ nav#ui-bar-tray
│  └─ div#ui-bar-body
│     └─ div#bar-surface
├─ nv-passage[data-passage]
│  ├─ header.passage-header
│  ├─ main.passage-main
│  └─ footer.passage-footer
│     ├─ div#passage-footer-surface
│     └─ span#status
└─ dialog#nv-dialog
```

游戏标题只进入系统窗口标题，不由 Host 硬编码到侧栏。展开内容来自 `:: Bar`，收起后的窄栏
摘要来自 `:: BarStowed`；回退、前进和展开／收起仍属于 Host 外壳。Runtime 连接与执行状态显示
在 Passage Footer，不占用作者侧栏。

## 游戏作者配置

标题、图标和窗口选项来自游戏目录中的同一份 `config.toml`，不再写死在
Host 的 `tauri.conf.json`。平台扩展归 Tauri Host 解析，Core 仍只读取 `[game]`：

```toml
[host.tauri]
title = "翡翠森林"
# 可选：icon = "host/tauri/icon.png"

[host.tauri.window]
width = 960
height = 640
min_width = 480
min_height = 360
resizable = true
fullscreen = false
decorations = true
maximized = false
```

未填写 `[host.tauri.window]` 时，Host 默认以 `fullscreen = true`、
`decorations = false` 启动，不显示占用空间的系统标题栏。开发时若需要普通窗口和系统窗口按钮，
显式使用上面的 `fullscreen = false`、`decorations = true` 即可。

开发时可在 `[host.tauri]` 设置 `developer = true`。此时按 F12 切换 WebView DevTools；
默认和发布配置应保持 `false`。关闭时 Rust command 同样拒绝访问，而非仅隐藏前端入口。
DevTools 只属于开发 WebView，不进入游戏 Worker API；调试 Worker State 不通过浏览器控制台暴露。

`title` 未填写时继承 `game.name`。`icon` 是游戏级可选项，仓库 Host 不要求自带 `icon.png`；
填写时只接受游戏目录内的普通相对 PNG／ICO 路径；
不能使用绝对路径、`..` 或反斜杠。当前 Host 不创建原生菜单栏；完整菜单需要多菜单、
多条目与动作分派契约，等实际 Host 动作稳定后再单独设计。

桌面开发从 workspace 根目录运行：

```text
cargo run -p narrava-loom-tauri -- examples
```

省略最后一个参数时使用 `examples/`。应用入口拒绝绝对游戏路径。其他测试和发行命令见
[仓库命令](../development/commands.md)。

## 平台管理能力

- `Host.delay()` 经 Core Pending／Resume 边界恢复，不阻塞 WebView 或 Runtime Worker；
- `Save.export/import` 把命名槽位写入或读自 `save/<target>.nsave`；
- Host 提供语言包选择和有界结构化诊断能力，但不生成“存档／语言／日志”固定页面；
- 游戏内管理弹窗的标题、页签、控件、文案与布局全部由游戏作者通过 Twee、脚本和
  Surface 定义；Host 只拥有原生 Dialog 外壳与经过验证的平台动作；
- Mod 管理界面属于 `narrava-loom-modloader`，不进入本 Host 的 Core 完成条件。

默认主题包含窄屏、safe-area 与 coarse pointer 规则。这只说明共享页面具备移动布局基础，
不代表 Android WebView 或 iOS WKWebView 已完成平台验收。

具体的自动测试、WebView 静态检查、真实窗口验收与发行目录回归步骤见
[Tauri Host 开发与测试](../development/testing-tauri.md)。非 Web Host 的对照验证见
[TUI Host 开发与测试](../development/testing-tui.md)。

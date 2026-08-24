# Narrava Loom 游戏作者示例

第一次使用请先阅读
[超级无敌菜鸟游戏制作手册](../docs/author/guide.md)，再把本目录复制为
自己的游戏。手册包含从安装、Twee、TypeScript、Resource、CSS 到 Tauri 启动与排错的完整步骤。

`examples/` 是纯游戏项目，不包含 Rust 源码。游戏作者只需要维护配置、Twee、可选的
TypeScript／JavaScript 和语言数据：

```text
examples/
├── config.toml
├── contents/
│   ├── story/main.twee
│   ├── story/widgets.twee
│   └── scripts/main.ts
├── resources/data/guide.txt
└── languages/en/
    ├── manifest.json
    ├── translations.nmsg
    └── dictionary.json
```

`story/widgets.twee` 定义 `crossFileCard`，`story/main.twee` 调用它，作为
“一个 Twee 文件定义、另一个 Twee 文件使用”的可编译回归示例。

正常游玩路径还提供两个专用 UI 验收页：

- “查看语义渲染”展示 Header、Bar、Footer、不同文字语义、Meter、Resource 图片和未知组件 fallback；
- “查看弹窗与按钮”展示 Dialog，以及默认、主要、次要、危险四种按钮角色。

因此不需要修改脚本或从 F12 手工调用 Macro，就能从大厅逐项检查 Host Renderer。

## 当前尚未支持的表单 Macro

下列常用 Macro 目前尚未进入 Narrava Loom 的 Core／Host 交互协议，因此示例不会写出
“能编译但不能正确游玩”的假用法：

- 表单扩展族 `numberbox`、`textarea`、`listbox`、`cycle`。

`checkbox`、`radiobutton` 与 `textbox` 已走完 Core 语义节点、Host 值验证和 `$`／`_` State
写回链路，并在“查看状态绑定表单”中可直接操作。单选 Macro 的名称是 `radiobutton`，不是
`radiobox`。同页的 `button` 会先执行正文写入确认状态，再在同一事务中返回大厅。`@` receiver
要等持久 Widget 实例状态完成后再开放。

“查看内容替换”先用 `slot "status-panel"` 建立稳定内容槽，再由 `replace "status-panel"` 替换；
同页还把普通文本与 `print` 产生的 Presentation 写入 Passage Header。
Core 接受 Header、Main、Footer、Bar、Dialog 区域名或稳定 Presentation key，不接收 CSS selector。

检查并编译游戏内容：

```text
cargo run -p narrava-loom-core -- examples
```

用当前 Tauri Host 启动游戏：

```text
cargo run -p narrava-loom-tauri -- examples
```

这里的 `cargo run` 只是仓库开发阶段启动尚未发布工具的方式，不表示游戏作者需要编写
Rust。正式发行后应由 Narrava CLI 或桌面工具直接打开游戏目录。

当前 CLI 会完成 Config、Source、Resource、Twee、HIR、MIR、LIR 与 Bytecode 检查。Tauri Host
会在 Rust Worker 中真实转译并执行示例 TypeScript、驱动 Story，并由 WebView Renderer 显示
Presentation。`:: Bar` 通过 `<<barDemo>>` 展示天气、人物、状态、提示和 Host 工具，
`:: BarStowed` 通过 `<<barStowedDemo>>` 展示“雨／痛／!”等窄栏摘要；删除对应 Macro 后，
Host 不会自行向 Bar 注入内容。模组管理属于独立 ModLoader。

示例在 `[host.tauri]` 中开启了 `developer = true`，因此窗口内可按 F12 检查 Renderer，并在
控制台使用 `window.narrava.state()`、`window.narrava.set(...)` 和
`window.narrava.del(...)` 调试 State。
制作发行配置时应关闭该选项。

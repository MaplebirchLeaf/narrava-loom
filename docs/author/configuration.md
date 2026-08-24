# `config.toml` 与第一个 Passage

## 5. 配置 `config.toml`

最小配置：

```toml
[game]
id = "your-name.my-first-game"
name = "我的第一个游戏"
version = "0.1.0"
default_locale = "zh-CN"
```

逐项解释：

- `id`：机器使用的永久身份。不能为空、不能包含空白。发布存档或语言包后不要随意修改。
- `name`：玩家看到的游戏名，也会成为默认窗口标题。
- `version`：语义化版本，例如 `0.1.0`、`1.0.0`、`1.2.3-beta.1`。
- `default_locale`：原文语言，使用合法语言标签，如 `zh-CN`、`en`、`ja`。

桌面窗口可选配置：

```toml
[host.tauri]
title = "我的第一个游戏"
# 可选：icon = "host/tauri/icon.png"
developer = true

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

规则：

- 省略整个 `[host.tauri]` 也能启动；
- 省略 `title` 时使用 `game.name`；
- `icon` 完全可选；不需要图标时不要创建 `icon.png`；填写时只接受游戏目录内的相对 `.png` 或 `.ico`；
- `developer` 默认为 `false`；开发时设为 `true`，发布时应关闭；
- `icon` 不能使用绝对路径、`..` 或反斜杠；
- 最小宽高不能大于初始宽高；
- 当前 Host 没有游戏作者配置的原生菜单栏。
- 省略窗口配置时默认全屏且无系统标题栏；调试窗口模式可显式设置
  `fullscreen = false` 与 `decorations = true`。

`game.name`（或显式的 `host.tauri.title`）用于系统窗口标题，不会被 Host 硬编码到侧栏中。
侧栏展开内容由 `:: Bar` 决定，收起后的窄栏内容由 `:: BarStowed` 决定。游戏不写 `styles/`
也会自动获得侧栏、正文、Header／Footer 与弹窗布局；自定义 CSS 只用于主题覆盖。

开发者模式开启后，在游戏窗口按 F12 可以开关 WebView DevTools。控制台通过
`window.narrava` 提供：

```js
await window.narrava.state()
await window.narrava.set("variables", "coins", 999)
await window.narrava.set("temporary", "debugName", "Maple")
await window.narrava.del("variables", "coins")
```

namespace 只能是 `global`、`variables` 或 `temporary`。`state()` 对标量显示实际值；对数组、
对象和函数只显示安全摘要，避免循环引用卡死 DevTools。修改值后通常要进行一次游戏 Interaction
才能看到新状态影响后续正文；它不会倒带并重画已经提交的 Passage。

还可以使用：

```js
window.narrava.current()       // 最近一次 Presentation DTO 的副本
await window.narrava.assets()  // CSS 和 Resource 清单
await window.narrava.activate("交互ID")
await window.narrava.devtools()
window.narrava.help()          // 显示所有调试方法说明
```

这些能力只用于 F12 调试，不是游戏作者脚本 API。`current()` 和 `assets()` 返回副本，修改它们
不会修改 Engine；`activate()` 会真实推进游戏，应当像点击按钮一样谨慎使用。

开发者模式会开放检查页面和修改运行状态的能力，发行给玩家前请改回：

```toml
[host.tauri]
developer = false
```

## 6. 写第一个能运行的故事

把 `contents/story/main.twee` 改成：

```twee
:: Start
你好，世界！
```

`::` 表示一个 Passage 的开始。`Start` 是固定入口，大小写必须完全一致。下面这些都不等于
`Start`：

```text
start
START
 Start
```

检查：

```bash
cargo run -p narrava-loom-core -- my-game
```

打开：

```bash
cargo run -p narrava-loom-tauri -- my-game
```

游戏路径当前必须是相对于仓库根目录的普通相对路径。不要传 `/home/.../my-game`，也不要传
`../my-game`。

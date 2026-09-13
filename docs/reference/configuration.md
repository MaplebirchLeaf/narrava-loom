# 项目配置

配置文件位于游戏根目录 `config.toml`。Core 读取 `[game]` 与 `[engine]`，
Tauri Host 读取自己的 `[host.tauri]` 扩展。

## 游戏身份

```toml
[game]
id = "tutorial.first-game"
name = "门后的故事"
version = "0.1.0"
default_locale = "zh-CN"
```

| 字段 | 要求 |
| --- | --- |
| `id` | 必填，非空且无空白，区分大小写 |
| `name` | 必填，去除空白后非空，用作默认窗口标题 |
| `version` | 必填，语义化版本，例如 `0.1.0` |
| `default_locale` | 必填，合法语言标签，例如 `zh-CN`、`en` |

游戏版本标识内容，与仓库的引擎版本独立。存档精确匹配游戏 ID 与版本；语言包使用
相同 ID 和版本范围。发布内容后修改身份需要考虑已有存档和语言包。

## Engine

```toml
[engine]
seed = 42
```

`seed` 可省略，由 Host 在作者脚本装载前生成；配置接受非负 TOML 整数。
`[engine]` 拒绝未知字段。Engine 持有根种子及游戏随机进度，行为见[游戏流程](../author/flow.md#根种子)。

## Tauri 窗口

```toml
[host.tauri]
title = "门后的故事"
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

整个 `[host.tauri]` 可省略；段内未知字段会报错。

| 字段 | 默认值与要求 |
| --- | --- |
| `title` | 继承 `game.name`；去除空白后非空 |
| `icon` | 无；可选游戏内相对 `.png` / `.ico` 路径，禁止绝对路径、`..` 和反斜杠 |
| `developer` | `false`；启用 F10 游戏控制台与 F12 WebView DevTools |
| `window.width` / `height` | `1048` / `640`，正有限数值，逻辑像素 |
| `window.min_width` / `min_height` | `360` / `640`，正有限数值，不得大于初始尺寸 |
| `window.resizable` | `true` |
| `window.fullscreen` | `true` |
| `window.decorations` | `false` |
| `window.maximized` | `false` |

上例显式启用普通窗口模式；省略设置时以全屏、无系统标题栏启动。
发行前关闭 developer。游戏标题进入系统窗口，侧栏内容由 `Bar` / `BarStowed` Passage 定义。

## 游戏目录

```text
my-game/
├── config.toml
├── contents/
│   ├── story/*.twee
│   └── scripts/*.ts 或 *.js
├── resources/
│   ├── img/
│   ├── audio/
│   └── data/
├── styles/*.css
└── languages/<locale>/
```

仅配置与包含 `Start` 的 Twee 故事必需。`contents/` 递归发现 `.twee`、`.ts`、`.js`，
按平台无关的相对路径排序。文件路径不决定故事导航顺序。
资源读取规则见[资源与主题](../author/resources.md)，语言目录见[多语言](../author/i18n.md)。
`styles/` 是可选 Tauri 主题，游戏无 CSS 时使用 Host 默认样式。

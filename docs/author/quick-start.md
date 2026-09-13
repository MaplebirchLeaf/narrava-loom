# 快速入门

目标是运行示例，再建立一个能在两页之间选择的游戏。以下命令均从仓库根目录执行。

## 准备环境

安装支持 Rust 2024 Edition 的稳定 Rust 工具链、Git，以及
[Tauri 对应操作系统的前置依赖](https://v2.tauri.app/start/prerequisites/)。
Linux 的两个 Host 还需要 ALSA 开发文件，例如 Debian/Ubuntu 的 `libasound2-dev`。

```bash
rustc --version
cargo --version
cargo run --locked -p narrava-loom-core -- examples
cargo run --locked -p narrava-loom-tauri -- examples
```

检查命令成功后会输出“可执行 Story 已建立”；桌面命令打开“小镇的一天”。
要使用终端界面，运行 `cargo run --locked -p narrava-loom-tui -- examples`。
首次运行会编译依赖，之后复用缓存。

运行游戏不需要 Node.js、Bun 或手工转译 TypeScript。编辑器类型检查需要的工具见
[脚本](scripting.md#编辑器类型配置)。Android/iOS 的交付状态见[项目状态](../development/status.md)。

## 建立自己的项目

在仓库下创建以下两个文件：

```text
my-game/
├── config.toml
└── contents/
    └── story/
        └── main.twee
```

`config.toml`：

```toml
[game]
id = "tutorial.first-game"
name = "门后的故事"
version = "0.1.0"
default_locale = "zh-CN"
```

`contents/story/main.twee`：

```twee
:: Start
你好，旅行者。<br>
<<link [[打开门|Room]]>><</link>>

:: Room
门后是一间安静的房间。<br>
<<link [[返回|Start]]>><</link>>
```

`Start` 是区分大小写的固定入口；每个 Passage 名称在整个项目中唯一。
`link` 的文字和目标用 `|` 分隔，目标必须存在。

## 检查并运行

```bash
cargo run --locked -p narrava-loom-core -- my-game
cargo run --locked -p narrava-loom-tauri -- my-game
```

开发入口使用 `my-game` 这样的普通相对路径，不接受绝对路径或 `../`。
修改故事后先重新检查，再启动 Host。检查通过只说明编译成功，点击和脚本行为仍需实际运行。

也可以复制完整 `examples/` 后修改内容。复制时保留 `tsconfig.json`，并为自己的游戏设置
新的 `game.id`；已有存档按游戏 ID 和版本匹配。

下一步阅读[编写 Twee](writing-twee.md)。窗口设置和完整项目目录见
[配置参考](../reference/configuration.md)，出现错误时查[调试](debugging.md)。

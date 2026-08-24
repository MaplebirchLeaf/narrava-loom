# Narrava Loom 超级无敌菜鸟游戏制作手册

> 适用版本：仓库当前 `0.1.0` 开发版
> 更新日期：2026-08-23
> 读者：不会 Rust、不会编译器、第一次写互动叙事的人

这份手册从零开始。你不需要理解 Rust，也不需要修改 `src/`、`hosts/` 或 `crates/`。
开发版目前仍要借助仓库里的 Rust 工具启动游戏，但游戏项目本身只包含配置、Twee、可选
TypeScript/JavaScript、资源、语言包和可选 CSS。

如果你只记住一件事：复制 `examples/`，只改复制出来的目录，不要拿引擎源码当游戏源码。

## 快速目录

只想查“现在能写什么”时，直接打开
[/docs/reference/api-and-syntax.md](/docs/reference/api-and-syntax.md)；其中集中列出了所有当前
内置 Macro、Expression 函数/方法、操作符、Worker ECMAScript API 和基础事件。

- 第 1～5 节：安装、启动、创建目录和配置；
- 第 6～12 节：Twee、Passage、导航、变量、条件和 Macro；
- 第 13～21 节：TS/JS、State、Resource、CSS、Engine、Event、I18n、Save；
- 第 22～25 节：运行、构建、错误信息和逐项排错；
- 第 26～30 节：避坑、推荐流程、能力状态、完整最小游戏和延伸文档。

完全没经验时按编号从头做到尾。已经成功打开 `examples/` 时，可直接从第 4 节开始。

## 1. 五分钟成功路线

在仓库根目录打开终端。所谓“仓库根目录”，就是同时能看到 `Cargo.toml`、`README.md`、
`examples/`、`src/` 的目录。

先检查 Rust：

```bash
rustc --version
cargo --version
```

然后检查示例：

```bash
cargo run --locked -p narrava-loom-core -- examples
```

成功时，最后应看到类似：

```text
已读取 scripts/main.ts
已读取 story/main.twee
已读取 1 个 Resource
可执行 Story 已建立
```

最后打开桌面游戏：

```bash
cargo run --locked -p narrava-loom-tauri -- examples
```

第一次运行会编译依赖，等待时间明显长于之后运行，这是正常的。打开后点击文本中的选项即可
切换 Passage。

## 2. 你需要安装什么

### 2.1 必需

1. 支持 Rust 2024 Edition 的稳定 Rust 工具链；推荐通过 rustup 安装。
2. Git，用于取得和更新仓库。
3. Tauri 2 在你的操作系统上要求的 WebView 与系统编译依赖。

各 Linux 发行版、Windows 和 macOS 的系统包不同，不要直接使用其他系统的安装命令。请使用
[Tauri 官方前置要求](https://v2.tauri.app/start/prerequisites/)中与你的系统对应的一节。

### 2.2 写游戏时不必安装

- 不必会 Rust；
- 不必安装 Node.js、npm、Bun 或前端打包器；
- 不必手工把 TypeScript 编译成 JavaScript；
- 不必使用 CSS；
- 不必使用 ModLoader。

`narrava-loom-tauri` 会在 Rust Worker 中用 Oxc 去除 TypeScript 类型，再用 Boa 执行
ECMAScript。WebView 只负责显示，不执行游戏脚本。

## 3. 绝对不要混淆的三个东西

| 名称 | 是什么 | 游戏作者是否修改 |
|---|---|---:|
| `narrava-loom-core` | 编译、状态、故事、宏、存档数据模型等本体 | 否 |
| `narrava-loom-tauri` | 桌面窗口、ECMAScript Worker、Renderer、默认 CSS | 否 |
| 你的游戏目录 | `config.toml`、Twee、脚本、资源、翻译、可选 CSS | 是 |

`narrava-loom-modloader` 是另一个附属项目，不属于 Core，也不是制作普通游戏的前置条件。

## 4. 创建自己的游戏

最稳妥的方法是复制完整示例：

```bash
cp -R examples my-game
```

Windows PowerShell 可使用：

```powershell
Copy-Item -Recurse examples my-game
```

之后只编辑 `my-game/`。最小目录如下：

```text
my-game/
├── config.toml
└── contents/
    └── story/
        └── main.twee
```

完整可选结构如下：

```text
my-game/
├── config.toml
├── contents/
│   ├── story/
│   │   ├── main.twee
│   │   └── chapter-2.twee
│   └── scripts/
│       ├── main.ts
│       └── helpers.js
├── resources/
│   ├── images/
│   ├── audio/
│   └── data/
├── styles/
│   └── game.css
├── languages/
│   └── en/
│       ├── manifest.json
│       ├── translations.nmsg
│       └── dictionary.json
```

目录可以分更多层。Narrava 会稳定排序并读取 `contents/**/*.twee`、`contents/**/*.ts` 和
`contents/**/*.js`。CSS 不属于 Core Source，只由 Tauri Host 从 `styles/**/*.css` 读取。
游戏图标不是必需文件；确实需要时再在 `config.toml` 中配置其相对路径。

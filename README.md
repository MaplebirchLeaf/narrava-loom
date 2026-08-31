# Narrava Loom

Narrava Loom 是以 Rust 实现、与宿主平台无关的叙事编译与运行核心。它负责 Twee 编译、
Expression、Macro、State、Story、I18n、Save、VM 和事务化 Engine；画面、输入、文件选择与
平台对象由 Tauri、Godot 或其他 Host 负责。官方 Tauri Host 位于 `hosts/narrava-loom-tauri`：
桌面入口可运行和发行，移动端目前只完成共享入口与布局基础；
游戏交付物是可移动的 `NarravaGame/` 目录，不要求作者维护 Rust 源码。文档从
[文档总入口](docs/README.md) 开始。

当前版本为 `0.5.1`：可构建、可测试的开发基线。它用于继续完善引擎和 Host，尚不承诺
面向最终游戏作者的稳定兼容性。版本变化见 [CHANGELOG](CHANGELOG.md)。

## 快速开始

需要支持 Rust 2024 Edition 的稳定 Rust 工具链。

```text
cargo run --locked -p narrava-loom-core -- examples
```

该命令读取不含 Rust 源码的示例游戏，并完成 Source → Twee → HIR → MIR → LIR → Bytecode 编译。

游戏制作入口见
[Narrava 游戏作者指南](docs/author/guide.md)，可运行示例及其预期行为见
[examples/README.md](examples/README.md)。
完整语法/API 清单见
[作者 API 与语法速查](docs/reference/api-and-syntax.md)；`.twee` 的 VS Code
高亮扩展位于 [editors/vscode-narrava-loom](editors/vscode-narrava-loom)。

## 工作区

| 路径 | 职责 |
|---|---|
| `src/` | `narrava-loom-core` library 与最小 CLI Host |
| `crates/narrava-loom-modloader/` | Core 的可选 ModLoader 附属；只允许依赖 Core |
| `hosts/narrava-loom-tauri/` | Tauri Host 与最小前端 |
| `bindings/typescript/` | Script 与 Tauri 的 TypeScript 契约 |
| `editors/vscode-narrava-loom/` | `.twee` 的 VS Code 高亮与编辑扩展 |
| `examples/` | 唯一的完整无 Rust 示例游戏 |
| `docs/` | 当前有效架构、领域边界与作者文档 |
| `dist/` | 构建输出与交付物（`NarravaGame/`、`*.vsix`），不入库 |
| `scripts/` | 仓库级构建/打包脚本 |

核心依赖方向固定为：

```text
narrava-loom-core
       ↑
narrava-loom-modloader / Host / Binding
       ↑
Host Renderer
```

所有构建产物统一输出到 `dist/`（gitignore 忽略）：游戏发行目录为
`dist/NarravaGame/`，VS Code 扩展包为 `dist/vscode-narrava-loom/*.vsix`。
`target/` 只保留 cargo 缓存，`cargo clean` 不会影响交付物。

Core 不依赖 ModLoader、Tauri、DOM、CSS 或具体 Renderer。详细边界见
[架构纲要](docs/architecture/overview.md)、
[仓库布局](docs/development/repository-layout.md)和
[Host 与 Surface](docs/architecture/protocol.md)。

## 常用命令

| 命令 | 用途 |
|---|---|
| `cargo run --locked -p narrava-loom-core -- examples` | 检查示例游戏源码和完整编译管线 |
| `cargo run --locked -p narrava-loom-tauri -- examples` | 启动 Tauri 桌面 Host |
| `cargo run --locked -p narrava-loom-tui -- examples` | 用根目录示例游戏启动可操作的 TUI Host |
| `cargo test --workspace --all-targets --locked` | 运行 Rust workspace 测试 |
| `bun run check` | 检查 TypeScript、前端、格式和 VS Code 扩展 |

参数含义、窄范围命令、格式／Clippy／文档／发行命令统一见
[仓库命令](docs/development/commands.md)。

## 当前边界

已经闭合的基础链包括 Twee 编译、MIR/Bytecode VM、同步与异步 Macro continuation、
Host Surface、I18n fallback、Save 数据模型、Resource、Event，以及 Script Bundle 与脚本调用契约。

Core 已建立 `.nar` 的拥有型源码记录、游戏身份、格式版本、内容哈希、可直接反序列化的
拥有型 Bytecode、Script Bundle 与逐资源哈希边界。更广的平台能力只在出现稳定用例后扩展。
Tauri Host 已在 Rust Worker 内通过 Boa/Oxc 执行游戏 JS/TS，并提供默认 Renderer、可选作者
CSS，以及供作者脚本调用的存档、语言和诊断能力；具体管理界面仍由游戏作者定义。模组 patch、模组 Story/Resource
组合和玩家模组属于独立 `narrava-loom-modloader`，均不计入 Core 完成度。当前状态以各领域
文档为准；当前完成度和下一阶段边界统一记录在
[项目状态](docs/development/status.md)，不在多个计划文件中重复维护。

## 修改约定

源码和测试布局遵循
[Narrava 源码规范](docs/development/code-style.md)。提交前至少运行格式检查、
Clippy、全工作区测试和示例 CLI；工作区包含未提交成果时，不自动覆盖、暂存或提交它们。
Host 的分层测试步骤见 [TUI 开发测试](docs/development/testing-tui.md)与
[Tauri 开发测试](docs/development/testing-tauri.md)。

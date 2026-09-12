# Narrava Loom

Narrava Loom 是以 Rust 实现、与宿主平台无关的叙事编译与运行核心。Core 负责 Twee 编译、
表达式、状态、历史与事务；官方 Tauri 和 TUI Host 负责画面、输入、音频与平台 IO。
游戏作者编写 Twee 和 TypeScript/JavaScript，交付可移动的 `NarravaGame/` 目录，无需维护 Rust 源码。

项目处于开发阶段，尚不承诺长期 API 兼容性。版本与变更见 [CHANGELOG](CHANGELOG.md)，
完成度和待验收范围见[项目状态](docs/development/status.md)。

## 快速开始

安装支持 Rust 2024 Edition 的稳定 Rust 工具链，然后检查示例：

```bash
cargo run --locked -p narrava-loom-core -- examples
```

该命令完成 Source → Twee → HIR → MIR → LIR → Bytecode 编译。启动游戏使用：

```bash
cargo run --locked -p narrava-loom-tauri -- examples
# 或终端 Host
cargo run --locked -p narrava-loom-tui -- examples
```

系统依赖和安装步骤见[第一次运行](docs/author/getting-started.md)，
全部检查、构建与发行命令见[仓库命令](docs/development/commands.md)。

## 文档入口

- [游戏作者手册](docs/author/guide.md)：从写故事到打包。
- [API 与语法速查](docs/reference/api-and-syntax.md)：当前作者契约。
- [综合示例](examples/README.md)：可运行场景与预期行为。
- [Twee 编辑器扩展](editors/vscode-narrava-loom/README.md)：高亮、导航与诊断。
- [总体架构](docs/architecture/overview.md)：Core、Protocol、Script 与 Host 的边界。
- [仓库布局](docs/development/repository-layout.md)与[源码规范](docs/development/code-style.md)：源码、测试和文档归属。
- [文档总入口](docs/README.md)：各领域教程、设计与开发指南。

## 修改与验证

Core 不依赖 Tauri、DOM、CSS 或具体 Renderer。重构保持公开行为、错误与事务边界；
保留工作区中已有的未提交成果。完成前运行 Rust 全工作区门禁、`bun run check` 和示例编译，
具体命令见[仓库命令](docs/development/commands.md)。Host 验收见
[TUI 开发测试](docs/development/testing-tui.md)与[Tauri 开发测试](docs/development/testing-tauri.md)。

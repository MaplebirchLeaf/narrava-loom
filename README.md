# Narrava Loom

用 Twee 与 TypeScript/JavaScript 编写互动叙事，在桌面窗口或终端中运行。
Rust 核心管理故事、状态、随机序列与存档；Tauri 和 TUI 负责显示、输入、音频与文件读写。

## 运行示例

在仓库根目录执行：

```bash
cargo run --locked -p narrava-loom-core -- examples
cargo run --locked -p narrava-loom-tauri -- examples
```

第一条检查并编译示例，第二条打开桌面游戏。终端版将第二条的包名换成
`narrava-loom-tui`。首次运行需要 Rust 和系统依赖，见[快速入门](docs/author/quick-start.md)。

## 文档

| 我要做什么 | 入口 |
| --- | --- |
| 从零制作游戏 | [作者手册](docs/author/README.md) |
| 查语法、参数和配置 | [契约参考](docs/reference/README.md) |
| 理解引擎与宿主边界 | [架构](docs/architecture/README.md) |
| 修改、测试和发布仓库 | [开发指南](docs/development/README.md) |

[小镇示例](examples/README.md)包含可玩游戏和能力手册；
[Twee 编辑器扩展](editors/vscode-narrava-loom/README.md)提供高亮、导航和诊断。
全部主题见[文档目录](docs/README.md)。

项目处于开发阶段，API 尚未承诺长期兼容。[项目状态](docs/development/status.md)
记录已实现范围与待验收项，[CHANGELOG](CHANGELOG.md)记录历史版本变化。

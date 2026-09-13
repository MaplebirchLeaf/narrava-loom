# Narrava Loom 文档

按读者选择入口；每个主题的行为、内部设计与完成度分别维护。

## 我是游戏作者

- [游戏作者手册](author/guide.md)：从安装、写故事到打包的分册教程。
- [API 与语法速查](reference/api-and-syntax.md)：宏、内置函数、运算符和脚本 API 索引。

## 我在开发 Host 或 Core

- [总体架构](architecture/overview.md)：三个宿主目标、领域所有权与编译管线。
- [运行时](architecture/runtime.md)：Engine、State、Macro、Story、Surface、scripts 与 Resource/Event 契约。
- [Runtime Session](architecture/runtime-session.md)：Host-neutral 命令、更新与挂起操作边界。
- [Macro 执行](architecture/macro-runtime.md)、[Expression](architecture/expression.md)、[Twee 编译器](architecture/twee.md)、[I18n](architecture/i18n.md)、[Save 格式](architecture/save-format.md) 设计说明。
- [弹窗执行](architecture/dialog.md)、[音频生命周期](architecture/audio.md)。
- [Tauri Host](architecture/tauri-host.md)、[Host Surface](architecture/protocol.md)。
- [仓库布局](development/repository-layout.md)、[源码规范](development/code-style.md)、[诊断与日志](development/diagnostics-and-logger.md)、[公开 API 和依赖锁定](development/public-api-and-dependencies.md)。
- [仓库命令](development/commands.md)：游戏检查、桌面 Host、质量门禁、Bun 与发行命令。
- [TUI 开发测试](development/testing-tui.md)、[Tauri 开发测试](development/testing-tauri.md)、[项目状态](development/status.md)。

## 规划与文档规则

[项目状态](development/status.md)是当前完成度与实际缺口的唯一入口，未实现能力不得写成当前 API。

| 目录         | 读者与内容                         | 不应放入              |
| ------------ | ---------------------------------- | --------------------- |
| author       | 游戏作者的操作、行为规则与示例     | Rust 所有权和编码细节 |
| reference    | 当前可用语法与 API 索引            | 待实现方案和迭代日志  |
| architecture | Core/Host 的边界、数据流、取舍     | 重复作者教程          |
| development  | 仓库命令、规范、验收和有状态的规划 | 冒充已完成的未来能力  |

同一主题的作者手册与内部设计各自维护对应职责，并互相链接；速查页链接详细行为，不重复维护。
完整命令集中在[仓库命令](development/commands.md)，变更历史集中在根目录 CHANGELOG。

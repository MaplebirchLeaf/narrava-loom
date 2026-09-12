# Narrava Loom 文档

文档按读者和稳定性分层：`author/` 只写完成任务的操作步骤，`reference/` 只放已实现契约的速查，
`architecture/` 解释所有权、边界和取舍，`development/` 面向修改仓库的人。尚未实现的内容必须
明确标注“后续”，不得写成当前 API；命令的完整解释只在“仓库命令”维护。

## 我是游戏作者

- [游戏作者手册](author/guide.md)：从安装、写故事到打包的分册教程（唯一入口）。
- [API 与语法速查](reference/api-and-syntax.md)：宏、内置函数、运算符和脚本全局对象——唯一契约清单。
- [Macro](author/macro.md)：原生宏、参数、点击正文与内容复用。
- [Save](author/save.md)：保存范围、存读档和完成结果。
- [Event](author/event.md)：作者事件、拉取订阅、Engine Passage 事件与 Reaction 事件链。
- [Reaction](author/reaction.md)：三种触发源、结构化效果、事务与 Save 行为。

## 我在开发 Host 或 Core

- [总体架构](architecture/overview.md)：项目定位、核心原则与编译管线。
- [运行时](architecture/runtime.md)：Engine、State、Macro、Story、Surface、scripts 与 Resource/Event 契约。
- [Runtime Session](architecture/runtime-session.md)：Host-neutral 命令、更新与挂起操作边界。
- [Macro 执行](architecture/macro-runtime.md)、[Expression](architecture/expression.md)、[Twee 编译器](architecture/twee.md)、[I18n](architecture/i18n.md)、[Save 格式](architecture/save-format.md) 设计说明。
- [Tauri Host](architecture/tauri-host.md)、[Host Surface](architecture/protocol.md)、[源码记录](architecture/source-record.md)。
- [仓库布局](development/repository-layout.md)、[源码规范](development/code-style.md)、[诊断与日志](development/diagnostics-and-logger.md)、[公开 API 和依赖锁定](development/public-api-and-dependencies.md)。
- [仓库命令](development/commands.md)：游戏检查、桌面 Host、质量门禁、Bun 与发行命令。
- [TUI 开发测试](development/testing-tui.md)、[Tauri 开发测试](development/testing-tauri.md)、[项目状态](development/status.md)。

## 规划与文档规则

[项目状态](development/status.md)是当前完成度与优先级的唯一入口；
[弹窗与页面](author/dialog.md)说明已实现语法，[弹窗执行边界](development/dialog-design.md)记录内部设计。

| 目录 | 读者与内容 | 不应放入 |
| --- | --- | --- |
| author | 游戏作者的任务、示例、失败处理 | Rust 所有权和编码细节 |
| reference | 当前可用语法与精确契约 | 待实现方案和迭代日志 |
| architecture | Core/Host 的边界、数据流、取舍 | 重复作者教程 |
| development | 仓库命令、规范、验收和有状态的规划 | 冒充已完成的未来能力 |

同一主题可有作者手册与内部设计两篇，但各自只维护对应职责，并互相链接。
变更历史集中在根目录 CHANGELOG；页面不再追加逐次实施日志。

音频作者入口：[Audio 播放与停止](author/audio.md)；实现边界：[Audio effect](development/audio-design.md)。

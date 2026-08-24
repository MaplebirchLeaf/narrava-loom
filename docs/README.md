# Narrava Loom 文档

文档按读者和稳定性分层，不再把教程、API 清单、实现细节和未来设想混在同一目录。

## 我是游戏作者

- [超级新手手册](author/guide.md)：从安装、写故事到打包的分册教程。
- [API 与语法速查](reference/api-and-syntax.md)：宏、内置函数、运算符和脚本全局对象。
- [Twee 参考](reference/twee.md)、[Expression 参考](reference/expression.md)、[Macro 参考](reference/macro.md)。

## 我在开发 Host 或 Core

- [总体架构](architecture/overview.md)、[运行时](architecture/runtime.md)、[脚本边界](architecture/script-binding.md)。
- [Tauri Host](architecture/tauri-host.md) 和 [Host Presentation](architecture/host-presentation.md)。
- [仓库布局](development/repository-layout.md)、[源码规范](development/code-style.md)、[诊断与日志](development/diagnostics-and-logger.md)。
- [TUI 开发测试](development/testing-tui.md)和 [Tauri 开发测试](development/testing-tauri.md)。
- [公开 API 和依赖锁定政策](development/public-api-and-dependencies.md)。
- [项目状态](development/status.md)：已闭合能力、当前限制和下一阶段边界。

## 文档规则

- `author/` 只写完成任务的操作步骤。
- `reference/` 是可查的已实现契约，同一个 API 只在这里维护一份清单。
- `architecture/` 解释所有权、边界和取舍，不充当新手教程。
- `development/` 面向修改仓库的人。
- 尚未实现的内容必须明确标注“后续”，不得写成当前 API。

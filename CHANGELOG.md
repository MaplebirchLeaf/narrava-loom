# 变更记录

本文件只记录使用者能够观察到的版本变化，不复制提交日志。

## 0.1.0 - 2026-08-25

Narrava Loom 的首个开发基线。

### 已包含

- Host-neutral 的 Twee 编译、Expression、Macro、State、Story、Event、I18n、Save、Resource、VM 与 Engine；
- 拥有型 `.nar`、Script Bundle、TypeScript 声明和无 Rust 示例游戏；
- Tauri 桌面 Host、最小 TUI Presentation 适配器和 VS Code Twee 扩展；
- 作者控制的 `Bar`／`BarStowed` 侧栏、Footer Runtime 状态和存档／语言／日志 Host 工具；
- 可移动 `NarravaGame/` 发行目录与 GitHub Actions 构建流水线。

### 当前限制

- `narrava-loom-modloader` 只保留独立附属边界，尚未实现模组加载流程；
- TUI 是跨 Host 语义验证适配器，不是完整终端游戏壳；
- `0.1.x` 阶段公开 API 仍可能调整，不提供跨版本兼容承诺。

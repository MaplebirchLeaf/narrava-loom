## 0.3.0 - 2026-08-27

### 已包含

- 拆出独立 `narrava-loom-protocol` crate：跨 Host 的 Surface 传输协议（`HostErrorDto`、节点/更新 DTO 与脚本 bridge 的受验证转换），单向依赖 Core；
- Host 与 `narrava-loom-modloader` 统一改为同时依赖 `narrava-loom-protocol` 与 `narrava-loom-core`，依赖方向固定为 `host/modloader → protocol → core`；
- 纯内部重构：Core 语义（Surface 等）与 Host 传输层分离，作者侧 API 与既有 DTO 公开形态不变。

### 当前限制

- `narrava-loom-modloader` 只保留独立附属边界，尚未实现模组加载流程；
- TUI 是跨 Host 语义验证适配器，不是完整终端游戏壳；
- `0.3.x` 阶段公开 API 仍可能调整，不提供跨版本兼容承诺。

## 0.2.0 - 2026-08-27

### 已包含

- 64 级状态色阶（灰阶 0-7 ＋ 光谱 8-63，二进制对齐）与 8 个语义字形（emphasis／strong／code／quote／marked／small／inserted／deleted）；
- StyledText 新增可选 `delay` 延迟浮现与结构性 `heading`（1/2，弹窗页签等页面划分），WebView 与 TUI 同步支持；
- Twee `text` Macro 并入 `print`：`<<print value [tone] [style...]>>` 或对象形式 `{tone, styles, delay, heading}`，单参数仍输出纯文本；
- 脚本侧 `Presentation.text()` 支持 `heading`，Dialog 按结构性标题恢复页签切换；
- 综合示例扩至 19 个 Passage：控制流范本（switch／for／while＋break/continue／unset/include）、作者工具、Twee 内 Presentation 等；
- 修复 `if`／`switch` 默认分支与后续文本合并导致的 I18n placeholder 错位；
- 文档按读者收束：`reference/` 只保留契约速查，设计说明移入 `architecture/`，作者手册去编号并统一入口，清除遗留合并冲突标记。

### 当前限制

- `narrava-loom-modloader` 只保留独立附属边界，尚未实现模组加载流程；
- TUI 是跨 Host 语义验证适配器，不是完整终端游戏壳；
- `0.2.x` 阶段公开 API 仍可能调整，不提供跨版本兼容承诺。

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

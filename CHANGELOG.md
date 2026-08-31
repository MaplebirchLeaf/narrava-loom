# 变更记录

本文件只记录使用者能够观察到的版本变化，不复制提交日志。

## 0.5.2 - 2026-09-01

### 已包含

- 修正 Tauri 中 `row` panel 被空白正文或内部替换指令意外拆成竖排的问题；同行 panel 使用紧凑间距，后续普通正文从下一行继续；
- 整理 WebView Renderer 的函数职责与命名，并补充横排分组、忽略项和结束边界的静态回归测试。
- 调整 TUI 帧顺序为页眉、侧栏、正文、弹窗、页脚，并停止显示内部 Passage 标题；正文继续作为无外框的内容画布。
- 修正 TUI 将相邻普通文字、样式文字和标点擅自拆行的问题；只有显式换行和块级内容建立行边界。
- 按职责拆分 TUI 命令、终端循环与 Surface Renderer，`lib.rs` 只保留稳定导出。

## 0.5.1 - 2026-08-31

### 已包含

- Script Runtime 与 Protocol 收敛为 Host-neutral `RuntimeSession`，Tauri 与 TUI 共享事务、挂起操作、Save、I18n、Event 与 Reaction 调度；
- Bootstrap 按职责拆成 TypeScript 模块，由 Bun 在开发期生成单一脚本；发布后的 Boa Runtime 和游戏作者不依赖 Bun；
- 新增 Native Reaction 注册、索引、Event/State/lifecycle 触发、次数状态、Save 恢复、循环保护与事务回滚；
- 作者脚本可直接使用 `V`、`T`、`setup` 代理，以及 Event 与 Reaction 的 TypeScript 契约和 VS Code 导航声明；
- `slot` 新增可选 `panel` 与 `stack`／`row` 标准语义：Protocol 保留容器意图和上下／同行排列，TUI 映射为字符方框，Tauri 映射为主题面板；默认 `plain + stack`；
- TUI 为页眉、页脚和侧栏增加区域边框，Dialog 按标题页分别成框，操作按正文、页面和侧栏分组；空页眉／页脚仍不显示，`s` 命令在互斥的展开与收起侧栏内容之间切换；
- TUI 与 Tauri 的同行 panel 之间保留 Host 间距，panel 组后的普通正文从下一行继续；
- 修正 Reaction widget/include 的追加时机说明与根 README 中已失效的 TUI example 命令。

## 0.3.1 - 2026-08-27

### 已包含

- `.nar` 发布容器增加 `NAR1` 魔数头：Host 加载时校验并拒绝无头文件，杜绝任意 ZIP 伪装成游戏包；容器内部仍是确定性 ZIP（可标准解压查看），哈希校验链不变；
- TUI Host 补全：支持 `game.nar` 发行包加载（含魔数校验与哈希验证），并渲染 `Bar`／`BarStowed` 特殊区域（隔离 State/Story 视图）；`Source`/`SourceList` 支持 Clone；
- 文档补充 `.nar` 容器结构与魔数说明。

## 0.3.0 - 2026-08-27

### 已包含

- 拆出独立 `narrava-loom-protocol` crate：跨 Host 的 Surface 传输协议（`HostErrorDto`、节点/更新 DTO 与脚本 bridge 的受验证转换），单向依赖 Core；
- Host 与 `narrava-loom-modloader` 统一改为同时依赖 `narrava-loom-protocol` 与 `narrava-loom-core`，依赖方向固定为 `host/modloader → protocol → core`；
- 纯内部重构：Core 语义（Surface 等）与 Host 传输层分离，作者侧 API 与既有 DTO 公开形态不变。

## 0.2.0 - 2026-08-27

### 已包含

- 64 级状态色阶（灰阶 0-7 ＋ 光谱 8-63，二进制对齐）与 8 个语义字形（emphasis／strong／code／quote／marked／small／inserted／deleted）；
- StyledText 新增可选 `delay` 延迟浮现与结构性 `heading`（1/2，弹窗页签等页面划分），WebView 与 TUI 同步支持；
- Twee `text` Macro 并入 `print`：`<<print value [tone] [style...]>>` 或对象形式 `{tone, styles, delay, heading}`，单参数仍输出纯文本；
- 脚本侧 `Presentation.text()` 支持 `heading`，Dialog 按结构性标题恢复页签切换；
- 综合示例扩至 19 个 Passage：控制流范本（switch／for／while＋break/continue／unset/include）、作者工具、Twee 内 Presentation 等；
- 修复 `if`／`switch` 默认分支与后续文本合并导致的 I18n placeholder 错位；
- 文档按读者收束：`reference/` 只保留契约速查，设计说明移入 `architecture/`，作者手册去编号并统一入口，清除遗留合并冲突标记。

## 0.1.0 - 2026-08-25

Narrava Loom 的首个开发基线。

### 已包含

- Host-neutral 的 Twee 编译、Expression、Macro、State、Story、Event、I18n、Save、Resource、VM 与 Engine；
- 拥有型 `.nar`、Script Bundle、TypeScript 声明和无 Rust 示例游戏；
- Tauri 桌面 Host、最小 TUI Presentation 适配器和 VS Code Twee 扩展；
- 作者控制的 `Bar`／`BarStowed` 侧栏、Footer Runtime 状态和存档／语言／日志 Host 工具；
- 可移动 `NarravaGame/` 发行目录与 GitHub Actions 构建流水线。

# 项目状态

> 基线版本：0.2.0
>
> 更新日期：2026-08-27

本页只记录仓库级完成度和下一阶段边界。已实现 API 的精确定义以
[参考文档](../reference/api-and-syntax.md)为准，设计理由以[架构文档](../architecture/overview.md)为准。

## 已闭合的开发基线

- Source → Twee → HIR → MIR → LIR → 拥有型 Bytecode → VM 编译执行链；
- 事务化 Engine、State、Story、Expression、Macro、Event、Logger 和 Diagnostic；
- I18n、Save、Resource、Script Bundle 与 TypeScript 游戏作者契约；
- Host-neutral Surface，包括语义节点、交互、稳定 Key、`slot` 与 `replace`；
- 在 Rust Worker 中执行 ECMAScript 的 Tauri Host，以及只负责 Renderer 的 WebView；
- 由 `Bar`／`BarStowed` 控制的展开与收起侧栏，以及不污染真实 State／Story 的特殊区域渲染；
- 可操作的 TUI Surface 前端，包括输入值保留、命令验证与标准输入／输出循环；
- 无 Rust 综合示例、Twee VS Code 扩展、可移动发行目录和正式构建流水线。

## 当前限制

- `narrava-loom-modloader` 不属于 Core，当前仅保留独立 crate 边界；
- TUI 尚无完整游戏目录的脚本 Worker、存档／语言菜单和终端尺寸自适应布局；
- Tauri 自动测试不代替真实 WebView 的像素与交互验收；
- Android/iOS 平台工程、签名、打包与真机验收尚未接通；当前可发行目标是桌面端；
- `Renderer.Model`、`Renderer.Audio`、ModLoader 和 ModUtils 不在 0.2.0 范围内。

## 下一阶段

下一阶段只接受能够形成完整纵向用例的工作，优先级如下：

1. 用真实 Tauri 窗口完成发行目录的视觉、输入、Resource、I18n 与 Save 回归；
2. 抽取可供 Tauri 与 TUI 共用的 Native ECMAScript／Runtime 驱动，避免复制 Worker；
3. 根据实际游戏用例稳定 0.2.x 的公开 API 与诊断，不预先扩张平台能力；
4. Core 稳定后，再单独设计 `narrava-loom-modloader` 的最小 `.nmod` 纵向链。

## 发布门禁

运行[仓库命令](commands.md)中的 Rust 全工作区门禁、`bun run check`、示例编译和发行目录回归。

桌面视觉未实际验收时必须明确说明；移动端未完成平台工程与真机验收前不得列为发布目标。

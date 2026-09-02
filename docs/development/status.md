# 项目状态

> 基线版本：0.5.3
>
> 更新日期：2026-09-01

本页只记录仓库级完成度和下一阶段边界。已实现 API 的精确定义以
[参考文档](../reference/api-and-syntax.md)为准，设计理由以[架构文档](../architecture/overview.md)为准。

## 已闭合的开发基线

- Source → Twee → HIR → MIR → LIR → 拥有型 Bytecode → VM 编译执行链；
- 事务化 Engine、State、Story、Expression、Macro、Event、Logger 和 Diagnostic；
- I18n、Save、Resource、Script Bundle 与 TypeScript 游戏作者契约；
- Event、State 与 lifecycle Reaction，含结构化效果、事务回滚、goto continuation 与 Save 状态；
- 零 Core 依赖的拥有型 Protocol，以及 Script Runtime 内部的 Surface/Core 转换适配层；
- 带协议版本的 Protocol Session request/response、无生命周期跨语言 handle 与 Native driver；
- Save/语言共用 pending/resume，Host 只处理平台 IO，Import 与 Script 同步按 State/Story 事务提交；
- 从 canonical Script Contract 生成的标签和 Runtime DTO 类型，以及 Bootstrap 启动自检；
- Host-neutral Surface，包括语义节点、交互、稳定 Key、`slot` 与 `replace`；
- 在 Rust Worker 中执行 ECMAScript 的 Tauri Host，以及只负责 Renderer 的 WebView；
- 由 `Bar`／`BarStowed` 控制的展开与收起侧栏，以及不污染真实 State／Story 的特殊区域渲染；
- 可操作的 TUI Surface 前端，包括输入值保留、命令验证与标准输入／输出循环；
- 无 Rust 综合示例、Twee VS Code 扩展、可移动发行目录和正式构建流水线。

## 当前限制

- 模组加载尚未实现；
- TUI 已使用共享 RuntimeSession 执行完整游戏目录；仍无存档／语言菜单和终端尺寸自适应布局；
- Tauri 自动测试不代替真实 WebView 的像素与交互验收；
- Android/iOS 平台工程、签名、打包与真机验收尚未接通；当前可发行目标是桌面端；
- `Renderer.Model`、`Renderer.Audio` 和模组加载不在 0.5.3 范围内。

## 下一阶段

下一阶段只接受能够形成完整纵向用例的工作，优先级如下：

1. 用真实 Tauri 窗口完成发行目录的视觉、输入、Resource、I18n 与 Save 回归；
2. 用实际嵌入方反馈稳定 RuntimeSession、ScriptAdapter 与 Protocol 的公开边界；
3. 根据实际游戏用例稳定 0.5.x 的公开 API 与诊断，不预先扩张平台能力；
4. Core 稳定后，再从可验证的 `.nmod` 纵向用例开始设计模组加载。

## 发布门禁

运行[仓库命令](commands.md)中的 Rust 全工作区门禁、`bun run check`、示例编译和发行目录回归。

桌面视觉未实际验收时必须明确说明；移动端未完成平台工程与真机验收前不得列为发布目标。

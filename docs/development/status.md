# 项目状态

> 基线版本：0.7.0
>
> 更新日期：2026-09-08

本页只记录仓库级完成度和下一阶段边界。已实现 API 的精确定义以
[参考文档](../reference/api-and-syntax.md)为准，设计理由以[架构文档](../architecture/overview.md)为准。

## 已闭合的开发基线

- Source → Twee → HIR → MIR → LIR → 拥有型 Bytecode → VM 编译执行链；
- 事务化 Engine、State、Story、Expression、Macro、Event、Logger 和 Diagnostic；
- I18n、Save、Resource、Script Bundle 与 TypeScript 游戏作者契约；
- Event、State 与 lifecycle Reaction，含结构化效果、事务回滚、goto continuation 与 Save 状态；
- 零 Core 依赖的拥有型 Protocol，以及 Script builder 到 Core 语义、Core 到 DTO 的边界转换；
- 带协议版本的 Protocol Session request/response，以及 Host 直接驱动的 RuntimeSession；
- Save/语言共用 pending/resume，Host 只处理平台 IO，Import 复用命令事务，Script 直接使用活动 Rust State；
- 从 Protocol Rust 声明生成的 DTO 类型及从 Script Contract 生成的脚本名称，以及 Bootstrap 启动自检；
- Host-neutral Surface，包括语义节点、交互、稳定 Key、`slot` 与 `replace`；
- 在 Rust Worker 中执行 ECMAScript 的 Tauri Host，以及只负责 Renderer 的 WebView；
- 由 `Bar`／`BarStowed` 控制的展开与收起侧栏，以及不污染真实 State／Story 的特殊区域渲染；
- 可操作的 TUI Surface 前端，包括输入值保留、命令验证与标准输入／输出循环；
- 无 Rust 综合示例、Twee VS Code 扩展、可移动发行目录和正式构建流水线。

## 当前限制

- 模组加载尚未实现；
- TUI 已使用共享 RuntimeSession 执行完整游戏目录，并支持自适应全屏布局、方向键操作、快捷存读档与语言轮换；独立的存档／语言菜单尚未提供；
- Tauri 自动测试不代替真实 WebView 的像素与交互验收；
- Android/iOS 平台工程、签名、打包与真机验收尚未接通；当前可发行目标是桌面端；
- `Renderer.Model`、`Renderer.Audio` 和模组加载尚未实现。

## 当前工作与下一阶段

| 工作 | 状态 | 完成标准 |
| --- | --- | --- |
| 原生 meter 与跨 Host 表现 | 已实现 | 复用 meter@1；label/value/min/max；TUI 字符条、Tauri 图形条 |
| 原生 image 与跨 Host 表现 | 已实现，待真实窗口验收 | 位置参数 alt；Tauri 图片；TUI alt 方框；Resource 逻辑路径 |
| TUI 收起侧栏 | 已实现 | 相邻单元共享边框，无额外间隔 |
| 作者文档分工 | 已整理 | Macro/Save 用法进入 author，内部格式与执行设计留在 architecture |
| 显式弹窗宏与 page 子句 | 规则已确认，待实现 | 见[弹窗提案](dialog-design.md)，目前不能作为可用 API |
| 桌面发行目录回归 | 待真实窗口验收 | 图片、输入、I18n、Save、缩放与键盘操作 |

下一步补齐已确认的 dialog/page；Audio 先设计，不把方案记为已实现能力。Model、模组和新平台暂不展开。
后续工作必须给出可运行用例与验收标准，不能仅凭新增接口记为完成。

## 发布门禁

运行[仓库命令](commands.md)中的 Rust 全工作区门禁、`bun run check`、示例编译和发行目录回归。

桌面视觉未实际验收时必须明确说明；移动端未完成平台工程与真机验收前不得列为发布目标。

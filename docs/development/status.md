# 项目状态

> 基线版本：0.1.0
>
> 更新日期：2026-08-25

本页只记录仓库级完成度和下一阶段边界。已实现 API 的精确定义以
[参考文档](../reference/api-and-syntax.md)为准，设计理由以[架构文档](../architecture/overview.md)为准。

## 已闭合的开发基线

- Source → Twee → HIR → MIR → LIR → 拥有型 Bytecode → VM 编译执行链；
- 事务化 Engine、State、Story、Expression、Macro、Event、Logger 和 Diagnostic；
- I18n、Save、Resource、Script Bundle 与 TypeScript 游戏作者契约；
- Host-neutral Presentation，包括语义节点、交互、稳定 Key、`slot` 与 `replace`；
- 在 Rust Worker 中执行 ECMAScript 的 Tauri Host，以及只负责 Renderer 的 WebView；
- 由 `Bar`／`BarStowed` 控制的展开与收起侧栏，以及不污染真实 State／Story 的特殊区域渲染；
- 最小 TUI Presentation 适配器，用于证明 Core 不依赖 Web 技术；
- 无 Rust 综合示例、Twee VS Code 扩展、可移动发行目录和正式构建流水线。

## 当前限制

- `narrava-loom-modloader` 不属于 Core，当前仅保留独立 crate 边界；
- TUI 尚无输入事件循环、脚本 Worker、存档菜单和完整终端布局；
- Tauri 自动测试不代替真实 WebView 的像素与交互验收；
- `Renderer.Model`、`Renderer.Audio`、ModLoader 和 ModUtils 不在 0.1.0 范围内。

## 下一阶段

下一阶段只接受能够形成完整纵向用例的工作，优先级如下：

1. 用真实 Tauri 窗口完成发行目录的视觉、输入、Resource、I18n 与 Save 回归；
2. 为 TUI 增加最小输入驱动，验证同一 Interaction 在非 Web Host 中可以提交；
3. 根据实际游戏用例稳定 0.1.x 的公开 API 与诊断，不预先扩张平台能力；
4. Core 稳定后，再单独设计 `narrava-loom-modloader` 的最小 `.nmod` 纵向链。

## 发布门禁

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo doc --workspace --no-deps --locked
cargo run --locked -p narrava-loom-core -- examples
node --check hosts/narrava-loom-tauri/frontend/main.js
npm test --prefix editors/vscode-narrava-loom
```

桌面视觉未实际验收时，发布说明必须明确写出，不得用 Rust 自动测试替代视觉验证。

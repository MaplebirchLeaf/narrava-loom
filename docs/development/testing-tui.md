# TUI Host 开发与测试

## 当前定位

`hosts/narrava-loom-tui` 是 Core Presentation 的最小终端适配器，用来证明同一语义输出不依赖
DOM、CSS 或 Tauri。它目前是 library，不是完整的可执行终端游戏壳：没有键盘事件循环、终端
布局库、存档菜单或脚本 Runtime Worker。

当前适配器负责：

- 把 Header、Main、Footer、Bar、BarStowed、Dialog 映射为六组终端文本；
- 保留稳定 Presentation Key，并执行 Region／Key `replace`；
- 把文本样式降级为 Markdown 风格的终端表示；
- 把图片降级为包含替代文字和 Resource 路径的文本；
- 为未知 Component 显示语义 fallback；
- 把导航、按钮、复选框、单选框和文本框收集为只读 `TuiInteraction` 清单。

当前清单只保留显示标签、交互类型和可用的不透明 Interaction ID；它尚未执行输入、保存 Radio
group 或把值提交回 Host。不要把“节点可以降级显示”写成“终端表单已经可操作”。

## 快速验证

只测试 TUI crate：

```bash
cargo test --locked -p narrava-loom-tui
```

只运行区域与替换测试，并显示测试输出：

```bash
cargo test --locked -p narrava-loom-tui \
  tests::region_and_key_replacements_update_terminal_surfaces -- --exact --nocapture
```

检查 TUI 与 Core 的依赖边界：

```bash
cargo clippy --locked -p narrava-loom-tui --all-targets -- -D warnings
cargo tree -p narrava-loom-tui
```

依赖树应包含 `narrava-loom-core`，不应包含 Tauri、WebView、DOM 或 CSS 工具。

## 可视化检查

运行不依赖终端 UI 库的可视 Demo：

```bash
cargo run --locked -p narrava-loom-tui --example visual_demo
```

它会把同一份 `PresentationOutput` 打印为 Header、Main、Bar、Dialog 和 Actions。示例同时建立
`status` slot，再用 `replace` 替换，因此 Main 中应只出现“替换完成”，不出现“等待替换”。

这是一份确定性的语义可视化，不会进入 alternate screen，也不读取键盘。未来完整终端 Host 可以
在它之上接入 Ratatui 等布局层，但不能改变 `TuiFrame` 已验证的区域与交互含义。

## 修改 Renderer 时必须覆盖什么

每次新增或修改 Presentation 节点映射，至少检查：

1. 节点进入正确的终端区域，而不是全部落入 Main；
2. 同一个稳定 Key 被替换时不增加重复 Block；
3. 不支持的视觉能力有可读 fallback，不静默丢失内容；
4. 可交互节点保留 Core 提供的不透明 Interaction ID；
5. 若开始实现输入提交，必须显式保留 Radio group、receiver 和允许值，不能依赖 HTML 属性；
6. TUI crate 没有反向引入平台类型到 Core。

新增行为应写成 `TuiFrame` 精确断言。不要用 ANSI 颜色快照代替语义断言；颜色和终端宽度属于
未来完整 TUI 外壳，当前测试只固定跨 Host 契约。

## 当前完成标准

TUI 验证通过只说明 Presentation 可以被非 Web Host 消费，不代表“终端版游戏已经完成”。完整
TUI Host 还需要独立实现输入循环、Runtime 驱动、异步唤醒、终端尺寸变化和平台文件能力；这些
能力应依赖 Core 公共接口，而不是复制 Tauri Worker。

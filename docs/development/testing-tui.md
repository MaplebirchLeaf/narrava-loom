# TUI Host 开发与测试

## 当前定位

`hosts/narrava-loom-tui` 是 Core Presentation 的终端适配器与阻塞式输入前端，用来证明同一
语义输出和交互协议不依赖 DOM、CSS 或 Tauri。它不使用 alternate screen，普通标准输入／输出
即可操作，也便于管道测试和嵌入其他 Native Host。

当前适配器负责：

- 把 Header、Main、Footer、Bar、BarStowed、Dialog 映射为六组终端文本；
- 保留稳定 Presentation Key，并执行 Region／Key `replace`；
- 把文本样式降级为 Markdown 风格的终端表示，并按 `TextTone` 色阶染色；
- 把带 `delay` 的 StyledText 停放在 `frame.delayed`，由消费方用 `render_at(elapsed)` 到点显示；
- 把图片降级为包含替代文字和 Resource 路径的文本；
- 为未知 Component 显示语义 fallback；
- 把导航、按钮、复选框、单选框和文本框收集为 `TuiInteraction` 清单；
- 保留输入允许值，并把玩家命令解析为 `Activate`、`Input` 或 `Dismiss`；
- 提供可恢复错误的输入循环，以及帮助、重绘和退出命令。

`run_terminal` 不拥有游戏 Runtime；调用方把已验证的 `TuiOperation` 提交给 Core worker，再返回
下一帧。当前 crate 尚未提供加载完整游戏目录的脚本 Worker、存档菜单或语言菜单，因此不能把
交互前端完成写成“与 Tauri 游戏能力完全等价”。

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

它会把同一份 `PresentationOutput` 打印为 Header、Main、Bar、Dialog 和操作列表，并等待玩家
输入。输入 `1` 切换复选框，`set 2 游侠` 修改文本框，输入 `3` 激活“返回大厅”；`help` 显示
命令，`redraw` 重绘，`quit` 退出。示例同时建立
`status` slot，再用 `replace` 替换，因此 Main 中应只出现“替换完成”，不出现“等待替换”。
带 `delay` 的文本保留在 `frame.delayed`，完整 Runtime 驱动方应按最小 `delay_ms` 调用
`render_at`。可用管道做一次真实输入回归：

```bash
printf 'help\n1\nset 2 游侠\n3\nquit\n' | cargo run --locked -p narrava-loom-tui --example visual_demo
```

## 修改 Renderer 时必须覆盖什么

每次新增或修改 Presentation 节点映射，至少检查：

1. 节点进入正确的终端区域，而不是全部落入 Main；
2. 同一个稳定 Key 被替换时不增加重复 Block；
3. 不支持的视觉能力有可读 fallback，不静默丢失内容；
4. 可交互节点保留 Core 提供的不透明 Interaction ID；
5. 输入提交必须使用 Core 提供的 Interaction ID 和允许值，不能依赖显示标签或 HTML 属性；
6. TUI crate 没有反向引入平台类型到 Core。

新增行为应写成 `TuiFrame`、`TuiCommand` 或 `TuiOperation` 精确断言。不要用 ANSI 颜色快照
代替语义断言；终端尺寸与全屏布局不属于当前输入协议。

## 当前完成标准

TUI 已能实际读取命令并产生经验证的 Host 操作，但完整游戏目录仍需要 Runtime 驱动、ECMAScript
Binding、异步唤醒、Save／I18n 文件能力。它们应抽成可复用 Native Binding，不应复制 Tauri
Worker 或让 TUI 依赖 Tauri crate。

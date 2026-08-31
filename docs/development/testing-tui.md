# TUI Host 开发与测试

## 当前定位

`hosts/narrava-loom-tui` 是 Core Surface 的终端适配器与阻塞式输入前端，用来证明同一
语义输出和交互协议不依赖 DOM、CSS 或 Tauri。它不使用 alternate screen，普通标准输入／输出
即可操作，也便于管道测试和嵌入其他 Native Host。

当前适配器负责：

- 把 Header、Main、Footer、Bar、BarStowed、Dialog 映射为六组终端文本；
- 保留稳定 Surface Key，并执行 Region／Key `replace`；
- 把 Container 的 `plain` 保持为透明分组，把 `panel` 映射为按 Unicode 显示宽度对齐的字符方框；
- `stack` panel 上下排列，连续 `row` panel 留一字符间距并逐行横向排列；页眉、页脚与当前侧栏使用 TUI Host 自己的区域边框；
- Dialog 以结构标题划分页，每页使用独立边框；操作列表按正文、Dialog 页、侧栏等来源分组，但仍使用全局连续编号；
- `bar` 与 `bar-stowed` 内容同时保留但只显示当前模式，终端命令 `s` 切换两者；
- 把文本样式降级为 Markdown 风格的终端表示，并按 `TextColor` 色阶染色；
- 把带 `delay` 的 StyledText 停放在 `frame.delayed`，由消费方用 `render_at(elapsed)` 到点显示；
- 把图片降级为包含替代文字和 Resource 路径的文本；
- 为未知 Component 显示语义 fallback；
- 把导航、按钮、复选框、单选框和文本框收集为 `TuiInteraction` 清单；
- 保留输入允许值，并把玩家命令解析为 `Activate`、`Input` 或 `Dismiss`；
- 提供可恢复错误的输入循环，以及帮助、重绘和退出命令。

`run_terminal` 是可复用的纯输入／输出循环；`host::run` 负责装载开发目录或发行 `game.nar`、
建立共享 RuntimeSession、等待 PendingOperation 并返回下一帧。Engine、生命周期事件、脚本
interaction 与特殊区域都由 RuntimeSession 编排。当前 TUI 尚无 Save／I18n 菜单和终端尺寸
自适应布局，因此不能写成“与 Tauri 游戏能力完全等价”。

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

用根目录示例游戏驱动完整 TUI Host（编译 → Engine → 终端渲染与交互）：

```bash
cargo run --locked -p narrava-loom-tui -- examples
```

TUI Host 会加载 `examples/`、执行脚本并渲染 Header、Main、Bar、Dialog 与操作列表，
等待玩家输入编号激活导航/按钮，`set <编号> <值>` 修改文本框，`sidebar`（或 `s`）切换侧栏，`help` 显示命令，
`redraw` 重绘，`quit` 退出。带 `delay` 的文本保留在 `frame.delayed`，完整
Runtime 驱动方应按最小 `delay_ms` 调用 `render_at`。可用管道做一次输入回归：

```bash
printf 'help\nquit\n' | cargo run --locked -p narrava-loom-tui -- examples
```

## 修改 Renderer 时必须覆盖什么

每次新增或修改 Surface 节点映射，至少检查：

1. 节点进入正确的终端区域，而不是全部落入 Main；
2. 同一个稳定 Key 被替换时不增加重复 Block；
3. `panel` Slot 被 Key replace 后仍保留面板边界；
4. 连续 `row` panel 在同一组终端行横向排列且彼此留缝，`stack` panel 保持上下排列，边缘源码空白不会形成断裂竖线；
5. 页眉与页脚为空时不输出标题或边框，侧栏两种状态不会同时显示，Dialog 每个标题页各有一个边框；
6. 不支持的视觉能力有可读 fallback，不静默丢失内容；
7. 可交互节点保留 Core 提供的不透明 Interaction ID，并在操作列表中归入实际可见区域或 Dialog 页；
8. 输入提交必须使用 Core 提供的 Interaction ID 和允许值，不能依赖显示标签或 HTML 属性；
9. TUI crate 没有反向引入平台类型到 Core。

新增行为应写成 `TuiFrame`、`TuiCommand` 或 `TuiOperation` 精确断言。不要用 ANSI 颜色快照
代替语义断言；终端尺寸与全屏布局不属于当前输入协议。

## 当前完成标准

TUI 已能加载完整开发目录或发行包、通过共享 RuntimeSession 执行 ECMAScript Binding、处理
`Host.delay` 并产生经验证的 Host 操作。仍缺 Save／I18n 文件菜单和非阻塞式终端唤醒；这些是
终端平台入口，不应重新把 Narrava 生命周期编排放回 TUI。

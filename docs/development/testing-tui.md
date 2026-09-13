# TUI Host 开发与测试

## 当前定位

`hosts/narrava-loom-tui` 是 Core Surface 的终端适配器，用来证明同一语义输出和交互协议不依赖
DOM、CSS 或 Tauri。真实终端使用 Crossterm + Ratatui 的 alternate screen；标准输入或输出被重定向
时自动退回普通文本循环，便于管道测试和嵌入其他 Native Host。

`run_terminal` 是可复用的纯输入／输出循环；`host::run` 负责装载开发目录或发行 `game.nar`、
建立共享 RuntimeSession、等待 PendingOperation 并返回下一帧。Engine、生命周期事件、脚本
interaction 与特殊区域都由 RuntimeSession 编排。TUI 文件系统层只负责语言包装载和存档字节 IO，
存档校验、恢复事务和语言提交仍属于 RuntimeSession。

## 快速验证

只测试 TUI crate：

```bash
cargo test --locked -p narrava-loom-tui
```

只运行区域与替换测试，并显示测试输出：

```bash
cargo test --locked -p narrava-loom-tui \
  tests::render::region_and_key_replacements_update_terminal_surfaces -- --exact --nocapture
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

TUI Host 会加载 `examples/`、执行脚本并渲染完整屏幕。方向键移动焦点，Enter 激活，`s` 切换侧栏，
`b`／`f` 回溯与前进，F2／F3 快速存读档，F4 切换语言，F10 只读检查，`q` 退出。带 `delay` 的文本保留在
`frame.delayed`，由 `render_at(elapsed)` 到点显示。可用非 TTY 文本回退做一次输入回归：

```bash
printf 'help\n:inspect\nquit\n' | cargo run --locked -p narrava-loom-tui -- examples
```

## 修改 Renderer 时必须覆盖什么

每次新增或修改 Surface 节点映射，至少检查：

1. 节点进入正确的终端区域，而不是全部落入 Main；
2. 同一个稳定 Key 被替换时不增加重复 Block；
3. `panel` Slot 被 Key replace 后仍保留面板边界；
4. 连续 `row` panel 在同一组终端行横向排列且彼此留缝，`stack` panel 保持上下排列，边缘源码空白不会形成断裂竖线；
5. 页眉与页脚为空时不输出标题或边框，侧栏两种状态不会同时显示，BarStowed 顶层块各自形成无间隔方格，Dialog 每个标题页各有一个边框；
6. 不支持的视觉能力有可读 fallback，不静默丢失内容；
7. 可交互节点保留 Core 提供的不透明 Interaction ID，并在操作列表中归入实际可见区域或 Dialog 页；
8. 输入提交必须使用 Core 提供的 Interaction ID 和允许值，不能依赖显示标签或 HTML 属性；
9. TUI crate 没有反向引入平台类型到 Core。

F10 检查覆盖层支持滚动，Esc/F10 关闭后保留原帧、弹窗和焦点；检查期间输入不能穿透到故事。

新增行为应写成 `TuiFrame`、`TuiCommand`、`TuiOperation` 或焦点状态的精确断言。不要用 ANSI
颜色快照代替语义断言；终端尺寸和全屏布局仍是 Host 表现，不进入 Protocol。

完成度与待补能力见[项目状态](status.md)。

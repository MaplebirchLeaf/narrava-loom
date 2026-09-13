# 测试与 Host 验收

编译器和事务测试验证语义，Renderer 测试验证节点与交互映射，真实窗口和设备验证平台表现。
报告结果时写清实际覆盖的层次。标准自动检查见[仓库命令](commands.md)。

## 按改动定位测试

| 改动 | 首先运行 |
| --- | --- |
| Core 编译、状态、存档 | `cargo test --locked -p narrava-loom-core` |
| 领域位置 | `cargo test --locked -p narrava-loom-location` |
| DTO、字段和枚举 | `cargo test --locked -p narrava-loom-protocol` 与 `bun run contract:check` |
| 脚本与 Session | `cargo test --locked -p narrava-loom-script` |
| Tauri Rust | `cargo test --locked -p narrava-loom-tauri` |
| TUI | `cargo test --locked -p narrava-loom-tui` |
| WebView 映射 | `bun run test:frontend` |
| 编辑器与作者类型 | `bun run test:vscode`、`bun run test:types` |
| 文档布局与链接 | `bun run docs:check` |

窄测试可在 cargo test 末尾添加用例名。例如：

```bash
cargo test --locked -p narrava-loom-tauri host_delay_suspends_and_resumes_the_engine_transaction
cargo test --locked -p narrava-loom-tauri packaged_game_starts_without_development_sources
cargo test --locked -p narrava-loom-tui \
  tests::render::region_and_key_replacements_update_terminal_surfaces -- --exact
```

行为测试应经过真实编译或 RuntimeCommand 入口，覆盖失败、取消、恢复与成功提交。
共享音频和存档用例位于 `hosts/tests/`，由两个 Host 加载。
独立代码快照应使用独立构建目录，避免不同源码共用同名项目产物。

## 双 Host 语义对照

新增或修改节点、字段和交互时同时检查：

- 内容进入正确区域；未知 Component 使用 fallback，自定义 Region 不丢失。
- 稳定 key 替换不产生重复节点；panel 保留边界，row/stack 与侧栏状态正确。
- 交互回传不透明 ID，输入只接受 Runtime 允许值，隐藏页面不保留可激活动作。
- Reaction、读档、语言切换和失败回滚同步更新状态、画面和输入。
- 多页 Dialog 独立管理选中页；纯文字页不依赖按钮焦点，重开恢复默认页。
- Engine 根种子与随机进度在脚本、Twee、历史、重绘和 Save 中一致。
- Pending 不泄漏半完成画面，错误 operation ID 不清除合法等待。

## Tauri 窗口

运行 `cargo run --locked -p narrava-loom-tauri -- examples`，按示例流程验证：

1. 从开始菜单创建角色，Textbox/Checkbox/Radio 写入状态，再进入卧室。
2. 探索地点、执行行动、回退和前进；检查任务 Event、阈值 Reaction 与导航。
3. 打开人物和能力手册弹窗，切页、关闭、重开，验证键盘操作及图片、meter、fallback。
4. 缩窄窗口、展开/收起侧栏，检查 Header/Footer、替换区域和弹窗遮挡。
5. 存读档并切换语言，确认得到新画面；验证森林音频连续播放、离开停止。
6. F10 执行、补全和取消受管等待，F12 检查 Renderer；关闭 developer 后验证两端拒绝入口。

F10 在游戏 Boa realm 求值，F12 在 WebView 中检查 DOM；二者边界见[调试](../author/debugging.md)。
自动前端测试不替代系统 WebView 像素、窗口焦点和真实音频设备检查。

## TUI 终端

运行 `cargo run --locked -p narrava-loom-tui -- examples`。
方向键选择、Enter 激活，`s` 切侧栏，`b` / `f` 回溯，F2/F3 存读档，F4 切语言，F10 只读检查，`q` 退出。
弹窗左右键切页、PgUp/PgDn 滚动、Esc/Backspace 关闭；检查覆盖层关闭后保留原帧与焦点。

非 TTY 回退可用于输入流程验证：

```bash
printf 'help\n:inspect\nquit\n' | cargo run --locked -p narrava-loom-tui -- examples
```

断言 TuiFrame、命令、操作与焦点语义，不用 ANSI 截图代替状态验证。
确认 TUI 依赖树没有引入 Tauri/WebView；终端尺寸不进入 Protocol。

## 发行回归

按[打包游戏](../author/publishing.md)构建，移动完整目录后从发行程序无参数启动。
重复资源、CSS、语言包、Save 与交互验收，确认没有依赖开发源文件。
移动端真机验收独立记录；具体缺口见[项目状态](status.md)。

# Tauri Host 开发与测试

## 测试边界

Tauri Host 分为 Rust Worker 与 Renderer。自动测试覆盖共享实现，真实桌面窗口负责视觉和系统
交互验收。Android/iOS 平台工程尚未初始化，因此本页的运行与发行步骤都是桌面命令；移动端
只能复用已测试的共享层，不能据此宣称完成。

`tauri.conf.json` 中出现的 `ipc.localhost` 与 `narrava-resource.localhost` 是 Tauri 内部 IPC／自定义
协议的 CSP 来源，不是需要作者启动的开发服务器，也不会把游戏暴露到外部网络。

## 1. 不打开窗口的自动测试

运行全部 Tauri Host 测试：

```bash
cargo test --locked -p narrava-loom-tauri
```

常用的窄测试：

```bash
cargo test --locked -p narrava-loom-tauri host_delay_suspends_and_resumes_the_engine_transaction
cargo test --locked -p narrava-loom-tauri example_surface_builder_reaches_tauri_semantic_dtos
cargo test --locked -p narrava-loom-tauri packaged_game_starts_without_development_sources
cargo test --locked -p narrava-loom-tauri packaged_host_styles_are_restored_from_the_reserved_resource_namespace
cargo test --locked -p narrava-loom-tauri protocol_reads_only_the_requested_validated_resource
```

这些测试分别覆盖 Pending／Resume、Script Surface 到 DTO、发行包启动、打包 CSS 恢复和
Resource 自定义协议。它们不会验证 WebKit 实际像素布局。

Rust 静态检查和全仓库门禁见[仓库命令](commands.md)。

## 2. WebView Renderer 静态检查

前端源码位于 `hosts/narrava-loom-tauri/frontend/`。修改后运行：

```bash
bun run check
```

重点检查：

- 交互只回传 DTO 中的不透明 ID；
- Resource URL 只由已验证清单生成；
- 作者 CSS 在 Host 默认 CSS 后加载；
- WebView DevTools（F12）只在 `developer = true` 时可用。

## 3. 真实窗口开发验收

启动完整示例：

```bash
cargo run --locked -p narrava-loom-tauri -- examples
```

第一次编译可能需要较长时间。Linux 若缺少 WebKit/GTK，请先安装 Tauri 2 的系统依赖。窗口启动
后按以下顺序验收：

1. Start 保持开始菜单；“新的一天”进入开局表单，开始后进入卧室；
2. 表单的 Textbox、Checkbox 和 Radio 能写回 State，行动、返回和前进正常；
3. 地图显示统一坐标，进入医院等室内地点后，人物页展示正确的地点信息；
4. 作者手册展示 Header、Bar、Footer、图片、Meter 和 Component fallback；
5. Dialog 页签、窄屏布局和侧栏收起正常；模态框打开时 F10 控制台仍能输入；
6. `replace` 更新稳定区域，正文不出现原始控制节点；
7. 存档与设置可导出／导入 Save、切换局部英文包；森林音频连续播放并在离开时停止；
8. Reaction 的行动事件、体力阈值和夜间导航按示例说明触发。

示例启用了开发者模式。按 F12 可开关 WebView DevTools，检查 Renderer 的 DOM、控制台与网络面板；
它只是 Renderer 调试入口，不属于游戏脚本 API。F10 打开单行脚本控制台，验证 `V`、
`Location.current()`、赋值后的 Reaction 与输入同步、错误回滚、命令历史
和清屏。还应验证 `State`、`Save`、`Engine` 的属性树，`Save.ex` 的 Tab 补全与参数帮助，
`Save.export/import`、`Engine.goto/back` 和 `await Host.delay(10000)` 的停止回滚。
关闭 developer 后前端入口隐藏，Rust 查询和执行同样拒绝。
输入触发 Reaction 替换或导航后，控件值与显示内容应同步；读档和语言切换也必须返回新画面。

## 4. 发行目录回归

开发目录成功不代表 `game.nar` 成功。发布前还要用真实 Host 二进制构建可移动目录：

按[发行命令](commands.md#发行与编辑器包)构建桌面目录。

在新的终端中从发行目录启动 `dist/NarravaGame/narrava`，重复 Resource、CSS、语言与存档检查。
构建器不会覆盖已有的 `dist/NarravaGame`；需要重建时先把旧目录移动到备份位置或选择新输出目录。

## 5. 提交前总门禁

按[仓库命令](commands.md)运行 Rust 全工作区门禁和 `bun run check`。

自动测试通过但没有启动真实窗口时，应明确写“自动测试通过，桌面视觉尚未验收”，不能把它描述
成完整 Tauri UI 验证。

# Tauri Host 开发与测试

## 测试边界

Tauri Host 分为两层：Rust Worker 编译并执行游戏，WebView 只把 DTO 渲染为 DOM 并回传交互。
因此开发测试也分为三类：不打开窗口的 Rust 测试、前端静态检查，以及真实 Tauri 窗口验收。
只有三类都通过，才能说明桌面 Host 的修改已经闭合。

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
cargo test --locked -p narrava-loom-tauri example_presentation_builder_reaches_tauri_semantic_dtos
cargo test --locked -p narrava-loom-tauri packaged_game_starts_without_development_sources
cargo test --locked -p narrava-loom-tauri packaged_host_styles_are_restored_from_the_reserved_resource_namespace
cargo test --locked -p narrava-loom-tauri protocol_reads_only_the_requested_validated_resource
```

这些测试分别覆盖 Pending／Resume、Script Presentation 到 DTO、发行包启动、打包 CSS 恢复和
Resource 自定义协议。它们不会验证 WebKit 实际像素布局。

Rust 静态检查：

```bash
cargo clippy --locked -p narrava-loom-tauri --all-targets -- -D warnings
cargo fmt --all -- --check
```

## 2. WebView Renderer 静态检查

前端没有独立打包器，源码直接位于 `hosts/narrava-loom-tauri/frontend/`。修改 `main.js` 后至少运行：

```bash
node --check hosts/narrava-loom-tauri/frontend/main.js
```

同时检查以下安全边界：

- 游戏文本只通过 `textContent` 写入，不使用游戏数据拼接 `innerHTML`；
- 交互只回传 DTO 中的不透明 ID；
- Resource URL 只由已验证清单生成；
- 作者 CSS 在 Host 默认 CSS 后加载，但不能执行脚本；
- `window.narrava` 只在 `developer = true` 时公开。

## 3. 真实窗口开发验收

启动完整示例：

```bash
cargo run --locked -p narrava-loom-tauri -- examples
```

第一次编译可能需要较长时间。Linux 若缺少 WebKit/GTK，请先安装 Tauri 2 的系统依赖。窗口启动
后按以下顺序验收：

1. Start 能进入大厅，前进、后退和侧栏收起不会报错；
2. “语义渲染”显示 Header、Bar、Footer、图片、Meter 和 Component fallback；
3. Dialog 默认定位第一页，切换页签后活动页签底部没有多余分隔线；
4. Checkbox 可切换，同组 Radio 互斥，Textbox 修改后能写回 State；
5. `replace` 能更新固定区域，页面中不出现原始控制节点；“查看内容替换”还应只显示
   `status-panel` 的替换后内容；
6. `:: Bar` 中的 `<<barDemo>>` 显示完整侧栏和管理入口，`:: BarStowed` 显示窄栏摘要；“存档”能在 `examples/save/` 写入并读取命名
   `.nsave`；删除该 Bar 内容后，Host 不得自行注入存档、语言或日志按钮；
7. “语言”列出已安装 `.nlang`，切换后在下一次渲染生效；
8. “日志”能显示 Host 成功或错误记录；
9. 作者主题与 `resource("images/loom.svg")` 在开发目录中正常解析。

示例启用了开发者模式。按 F12 后可用：

```js
await window.narrava.state()
await window.narrava.assets()
window.narrava.current()
window.narrava.help()
```

F12 与 `window.narrava` 只是调试入口；游戏脚本仍在 Rust Worker 中，不能访问 `window` 或 DOM。

## 4. 发行目录回归

开发目录成功不代表 `game.nar` 成功。发布前还要用真实 Host 二进制构建可移动目录：

```bash
cargo build --release --locked -p narrava-loom-tauri
cargo run --release --locked -p narrava-loom-core -- \
  build examples dist/NarravaGame target/release/narrava-loom-tauri
```

在新的终端中从发行目录启动 `dist/NarravaGame/narrava`，重复 Resource、CSS、语言与存档检查。
构建器不会覆盖已有的 `dist/NarravaGame`；需要重建时先把旧目录移动到备份位置或选择新输出目录。

## 5. 提交前总门禁

Host 改动最终仍需通过整个工作区：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo doc --workspace --no-deps --locked
node --check hosts/narrava-loom-tauri/frontend/main.js
```

自动测试通过但没有启动真实窗口时，应明确写“自动测试通过，桌面视觉尚未验收”，不能把它描述
成完整 Tauri UI 验证。

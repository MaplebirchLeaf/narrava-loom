# Narrava Loom 综合示例

`examples/` 是一个可以检查、运行和打包的纯游戏项目，不包含 Rust 源码。建议先运行它，再复制
为自己的游戏：

```bash
cargo run --locked -p narrava-loom-core -- examples
cargo run --locked -p narrava-loom-tauri -- examples
```

第一条命令检查完整编译管线，不打开窗口；第二条命令启动当前桌面 Tauri Host。移动端复用同一
Host crate 和 Renderer 契约，但本仓库尚未提供 Android/iOS 工程与打包命令。

## 目录

```text
examples/
├── config.toml
├── contents/
│   ├── story/main.twee       # 可游玩的总览与功能验收页
│   ├── story/widgets.twee    # 跨文件 Widget 示例
│   ├── story/author-tools.twee   # Save、I18n、State 与 Reaction 页面
│   ├── scripts/main.ts       # V/T/setup、Reaction、Resource 与 Surface
│   └── scripts/author-tools.ts   # Save、Logger、I18n 与语言切换封装
├── resources/
│   ├── data/guide.txt
│   └── images/loom.svg
├── languages/en/             # 开发态语言包输入
└── save/                     # Tauri 首次导出时创建 schema 2 存档
```

Twee 源码中的普通换行只用于排版；游戏内换行一律显式写 `<br>`，或由脚本 Surface 使用
`Surface.hardBreak()`。

## 从哪里看

从 `Start` 进入 `Hall` 后可以逐项打开：

- `SurfaceGallery`：Region、Component、Resource 图片、语义文字和未知组件 fallback；
- `DialogGallery`：WebView Dialog 页签、TUI 独立页面边框及按页分组的四种动作角色；
- `FormGallery`：checkbox、radiobutton、textbox 与 State 写回；
- `ReplaceGallery`：透明 plain slot、有间距的显式 `row` 相邻 panel、后续正文换行、Region 与稳定 key 替换；
- `StateGallery`：scripts 的 `V/T/setup` 与 Twee `$/_/setup` 共享状态；
- `AuthorToolsGallery`：Tauri Save 导出/导入、Logger、I18n 模板导出与语言切换；
- `ReactionGallery`：Event、State、lifecycle、replace、include、goto、once/limit 与 Save 状态；
- `TextGallery`：`print` 的 64 色阶、8 种语义字形、heading 和 delay；
- `MacroGallery`：switch、for、while、break/continue、unset 与 include。

从任意演示页使用侧栏的后退／前进按钮即可检查 Story 历史；进入第二个 Passage 后后退才会启用，
后退一次后前进才会启用。TUI 对应命令为 `b` 和 `f`，使用 `s` 在 `Bar` 与
`BarStowed` 两套互斥侧栏内容之间切换。

各脚本文件在自己的末尾通过 `State.global.extend()` 暴露函数给 Twee；日常状态访问使用 `V.name`、
`T.name` 和 `setup.name`。`State.*` 留给动态键、旧值返回与批量导入。作者工具页调用
`I18n.select(locale)`，由 Runtime 和 Host 完成语言包校验并立即重绘当前 Passage；
`I18n.export()` 的完整模板写入 `i18n.export` 日志。

Tauri Host 会把 `Save.export("manual-1")` 写到 `save/manual-1.nsave`，随后同页按钮可以实际读回。
TUI 目前没有文件存档 IO，因此会显示稳定的 `runtime_session.save_unsupported` 提示；这不影响语言、
日志与 I18n 模板演示。

`Bar` 和 `BarStowed` 的内容同样由游戏的 Passage 与脚本 Macro 提供，Host 不会注入存档、语言、
日志或模组管理界面。示例在 `[host.tauri]` 中启用了 `developer = true`，因此桌面开发窗口可用
F12 开关 WebView DevTools；制作发行配置时应关闭它。

## 当前边界

示例只展示已接通的能力。表单目前支持 `checkbox`、`radiobutton` 与 `textbox`；`numberbox`、
`textarea`、`listbox`、`cycle` 尚未进入协议。表单 receiver 支持 `$` 和 `_`，不伪造已结束
Macro 调用帧中的 `@` 状态。`replace` 接受固定 Region 或稳定 Surface key，不接受 CSS selector。

完整教程见[游戏作者手册](../docs/author/guide.md)，精确接口见
[API 与语法速查](../docs/reference/api-and-syntax.md)。

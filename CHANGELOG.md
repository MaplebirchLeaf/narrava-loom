# 变更记录

本文件只记录使用者能够观察到的版本变化，不复制提交日志。

## 0.10.0 - 未发布

- 根种子归属 Engine，由 `[engine].seed` 或 Rust `Engine::new(seed)` 在开局提供；脚本通过只读 `Engine.seed` 查询。
- 脚本 `Math.random()` 与 Twee `random()` / `either()` 共用 Engine 序列；失败、历史、重绘与存读档保持随机进度一致。
- 移除独立 `Random` API 和示例开局种子输入，示例根种子改在项目配置中提供。
- 存档使用 schema 5，保存 Engine 根种子与随机进度，拒绝此前实验格式；旧文件不会自动删除。
- Tauri 与 TUI 共用存档文件 IO，统一目标名校验、大小限制与覆盖写入，保留各 Host 错误来源。
- 合并重复源码索引与测试说明，删除冗余文档及双 Host 存档实现。
- 项目包统一为 `0.10.0`，后续按发布周期定版本；开发提交不再逐次涨版本。

## 0.9.0 - 2026-09-13

### 新增

- Tauri F10 提供底部单行脚本控制台：对象树、成员补全、双语参数帮助、命令历史和统一日志；支持 Save、Engine 与受管异步等待，取消等待时回滚状态。
- Core、Twee 表达式和脚本共用可保存、可回放的随机序列，新增 `Random` API；历史、读档和失败回滚保持随机状态一致。
- TUI 可通过 F10 或 `:inspect` 只读查看 State、Location、随机状态与日志；错误保留来源文件和位置。
- `examples` 更新为原创“小镇的一天”，展示角色创建、城镇探索、打工购物、委托、人物弹窗、存档与随机回放，并保留独立能力手册。

### 调整与迁移

- **接口更名**：地点领域统一使用 `narrava-loom-location` / `Location`，原 `narrava-loom-world` 依赖和脚本 `World` 调用需要改名；已注册英文地点 ID、Passage 标签及二维坐标语义不变。
- 地点支持负坐标、多边形包含查询、父子范围与地点内移动，开始菜单保持未定位；位置参与事务、历史与存档。
- 存档格式升级至 v4，兼容读取 v2/v3；旧格式缺少随机状态时使用种子 0，v2 缺少位置时保持未定位。
- API 双语帮助从作者类型声明生成，与编辑器共用来源；整理作者指南、源码索引和测试说明。

### 修复

- 修复 Tauri 输入、读档和语言切换丢帧；Reaction 后同步输入控件，失败时恢复状态与画面。
- 接通跨文件纯逻辑 Twee Widget 调用，保留脚本同名宏的覆盖优先级；编辑器高亮语料移出可运行示例。
- 修复 Reaction 回调查询规则时的借用冲突，保留生命周期规则发出事件时的载荷。
- 隔离作者 TypeScript 全局类型，修复 `setup` 与 Mocha、`Event` 与 DOM 类型冲突；独立类型包包含完整契约声明。

## 0.8.0 - 2026-09-12

- 原生 `dialog/page` 支持单页与多页、按标题选择默认页；无导航 `link "文本"` 点击执行正文，不新增 Story 历史。支持异步恢复、取消和错误回滚。
- Tauri 页签支持键盘操作、窄窗口滚动与同一弹窗保持选中页；TUI 纯文字页也可独立切换、滚动和关闭。普通 heading 不再划分页面。
- Audio 通过 Runtime effect 交给 TUI/Tauri 本地播放；宏使用资源、channel、tag 位置参数，支持 Header/Footer 声明、Passage 生命周期、连续播放与自动停止；复杂配置使用 Script Audio.play/stop。
- Image/Audio 路径分别相对于 `resources/img/`、`resources/audio/`，兼容省略或带一次根前缀。
- Protocol 更新至 2；Navigation/Button 的 target 可为空。Bytecode 格式更新至 2，已有编译产物需要重新编译。
- 删除旧的标题推断分页示例与渲染逻辑，补充作者弹窗/音频文档、原生示例和 VS Code 悬停说明；统一 Cargo/package.json/Tauri 版本。

## 0.7.0 - 2026-09-09

- Twee 新增原生 `image` Macro，复用 Resource 与 Image 语义链，支持表达式路径与位置参数 alt；Tauri 显示图片，TUI 将 alt 显示在方框内。
- **不兼容变化**：Image 的 Semantic、Protocol 与 `Surface.image` 选项移除 caption；作者改用 `<<image "images/tree.png" "树">>`，说明正文单独编写。
- 原生 `meter` Macro 复用 meter@1 component，TUI 显示十格字符条，Tauri 显示图形状态条。
- TUI 收起侧栏相邻方格共享边框；VS Code 扩展为原生宏提供悬停及补全说明，分隔符保留原有 TextMate scope。
- 作者 Macro/Save 使用手册与内部执行/格式文档分离；dialog/page 尚未实现，单独保留设计与验收要求。
- Cargo、package.json 与 Tauri 版本联动升级，并纳入检查命令。

## 0.6.0 - 2026-09-07

- Rust Host 直接驱动 `RuntimeSession`；移除 `RuntimeSessionDriver`、`RuntimeSessionHandle`、`ScriptAdapter` 及未接入执行链的 Core Script 门面。Save/I18n 构造改用 `RuntimeData` 与 `with_data`。
- Script 输出与原生渲染统一使用 Core `SemanticOutput`，移除重复 Surface 类型；Protocol wire 字段不变，TypeScript DTO 改从 Rust 声明生成。
- 命令事务统一恢复 State、Story、Reaction、交互与上一帧；错误操作 ID 不再干扰活动事务，失败的输入存档恢复输入前状态。
- Protocol Session ID 反序列化复用构造校验，拒绝绕过类型约束的非法身份。

## 0.5.3 - 2026-09-02

### 已包含

- Story 回退与前进会先恢复目标历史项对应的 `$variables`，再重放 Passage；`.nsave` 同步保存各历史位置的持久状态，不再用额外访问次数 Macro 模拟回溯。
- 作者脚本可用 `I18n.select(locale)` 请求 Host 切换运行语言，并复用原生 Runtime pending 与语言包校验路径。
- 综合示例将作者工具拆到自然命名的 `author-tools.*` 源文件，增加游戏内语言切换、实际 Tauri 存档读写与 I18n 模板日志导出，并改为运行时生成 schema 2 示例存档。
- 修复脚本请求切换语言后当前页面仍返回旧语言内容的问题；切换成功后立即按新语言重绘正文、`Bar` 与 `BarStowed`，折叠同名刷新导航且不重复递增持久变量。

## 0.5.2 - 2026-09-01

### 已包含

- 修正 Tauri 中 `row` panel 被空白正文或内部替换指令意外拆成竖排的问题；同行 panel 使用紧凑间距，后续普通正文从下一行继续；
- 整理 WebView Renderer 的函数职责与命名，并补充横排分组、忽略项和结束边界的静态回归测试。
- 调整 TUI 帧顺序为页眉、侧栏、正文、弹窗、页脚，并停止显示内部 Passage 标题；正文继续作为无外框的内容画布。
- 修正 TUI 将相邻普通文字、样式文字和标点擅自拆行的问题；只有显式换行和块级内容建立行边界。
- TUI 取消页眉、侧栏、正文、弹窗、页脚和操作区的 Host 语言标签，按实时终端宽度生成等号与短横线分隔区域和操作组。
- 按职责拆分 TUI 命令、终端循环与 Surface Renderer，`lib.rs` 只保留稳定导出。
- `.nsave` 改为带版本头的紧凑二进制文档，保存与恢复不再经由 JSON 或重复的完整状态快照；失败恢复只保留一份事务检查点。
- Story 建立借用 Passage 名称的运行时索引，I18n 包、翻译模板和会话语言改用共享所有权，并在验证后释放无需常驻的源文件字节。
- I18n 支持将 `translations.nmsg` 与 `messages/*.nmsg` 合并为同一消息表，消息查找使用延迟哈希索引；示例语言包同步展示了拆分方式。

## 0.5.1 - 2026-08-31

### 已包含

- Script Runtime 与 Protocol 收敛为 Host-neutral `RuntimeSession`，Tauri 与 TUI 共享事务、挂起操作、Save、I18n、Event 与 Reaction 调度；
- Bootstrap 按职责拆成 TypeScript 模块，由 Bun 在开发期生成单一脚本；发布后的 Boa Runtime 和游戏作者不依赖 Bun；
- 新增 Native Reaction 注册、索引、Event/State/lifecycle 触发、次数状态、Save 恢复、循环保护与事务回滚；
- 作者脚本可直接使用 `V`、`T`、`setup` 代理，以及 Event 与 Reaction 的 TypeScript 契约和 VS Code 导航声明；
- `slot` 新增可选 `panel` 与 `stack`／`row` 标准语义：Protocol 保留容器意图和上下／同行排列，TUI 映射为字符方框，Tauri 映射为主题面板；默认 `plain + stack`；
- TUI 为页眉、页脚和侧栏增加区域边框，Dialog 按标题页分别成框，操作按正文、页面和侧栏分组；空页眉／页脚仍不显示，`s` 命令在互斥的展开与收起侧栏内容之间切换；
- TUI 与 Tauri 的同行 panel 之间保留 Host 间距，panel 组后的普通正文从下一行继续；
- 修正 Reaction widget/include 的追加时机说明与根 README 中已失效的 TUI example 命令。

## 0.3.1 - 2026-08-27

### 已包含

- `.nar` 发布容器增加 `NAR1` 魔数头：Host 加载时校验并拒绝无头文件，杜绝任意 ZIP 伪装成游戏包；容器内部仍是确定性 ZIP（可标准解压查看），哈希校验链不变；
- TUI Host 补全：支持 `game.nar` 发行包加载（含魔数校验与哈希验证），并渲染 `Bar`／`BarStowed` 特殊区域（隔离 State/Story 视图）；`Source`/`SourceList` 支持 Clone；
- 文档补充 `.nar` 容器结构与魔数说明。

## 0.3.0 - 2026-08-27

### 已包含

- 拆出独立 `narrava-loom-protocol` crate：跨 Host 的 Surface 传输协议（`HostErrorDto`、节点/更新 DTO 与脚本 bridge 的受验证转换），单向依赖 Core；
- Host 与 `narrava-loom-modloader` 统一改为同时依赖 `narrava-loom-protocol` 与 `narrava-loom-core`，依赖方向固定为 `host/modloader → protocol → core`；
- 纯内部重构：Core 语义（Surface 等）与 Host 传输层分离，作者侧 API 与既有 DTO 公开形态不变。

## 0.2.0 - 2026-08-27

### 已包含

- 64 级状态色阶（灰阶 0-7 ＋ 光谱 8-63，二进制对齐）与 8 个语义字形（emphasis／strong／code／quote／marked／small／inserted／deleted）；
- StyledText 新增可选 `delay` 延迟浮现与结构性 `heading`（1/2，弹窗页签等页面划分），WebView 与 TUI 同步支持；
- Twee `text` Macro 并入 `print`：`<<print value [tone] [style...]>>` 或对象形式 `{tone, styles, delay, heading}`，单参数仍输出纯文本；
- 脚本侧 `Presentation.text()` 支持 `heading`，Dialog 按结构性标题恢复页签切换；
- 综合示例扩至 19 个 Passage：控制流范本（switch／for／while＋break/continue／unset/include）、作者工具、Twee 内 Presentation 等；
- 修复 `if`／`switch` 默认分支与后续文本合并导致的 I18n placeholder 错位；
- 文档按读者收束：`reference/` 只保留契约速查，设计说明移入 `architecture/`，作者手册去编号并统一入口，清除遗留合并冲突标记。

## 0.1.0 - 2026-08-25

Narrava Loom 的首个开发基线。

### 已包含

- Host-neutral 的 Twee 编译、Expression、Macro、State、Story、Event、I18n、Save、Resource、VM 与 Engine；
- 拥有型 `.nar`、Script Bundle、TypeScript 声明和无 Rust 示例游戏；
- Tauri 桌面 Host、最小 TUI Presentation 适配器和 VS Code Twee 扩展；
- 作者控制的 `Bar`／`BarStowed` 侧栏、Footer Runtime 状态和存档／语言／日志 Host 工具；
- 可移动 `NarravaGame/` 发行目录与 GitHub Actions 构建流水线。

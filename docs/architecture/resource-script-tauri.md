# Narrava Loom Resource、Script API 与 Tauri Host 规格

> 状态：Core 已完成，基础 Tauri Host 已实现
>
> 更新日期：2026-08-23

## 目标

补齐 `narrava-loom-core` 的 Resource 与 Event 本体能力，为游戏作者提供完整 TypeScript
声明，并让 `narrava-loom-tauri` 真正加载和执行游戏 `.ts/.js`、渲染 Core Presentation。
游戏作者只需要 `config.toml`、Twee、TS/JS 和资源；Rust 不是游戏项目内容，CSS 是可选的
Tauri Host 外观输入。

## 架构决定

1. Core 只拥有 Resource 的逻辑路径、字节、媒体类型、完整性和生命周期；URL、Blob、解码、
   DOM 与缓存属于 Host。
2. Core Event 是结构化事实总线，不替代 Logger，不保存平台回调。
3. `ModLoader`、`ModUtils`、模组资源覆盖和 ResourceSelector 只属于
   `narrava-loom-modloader`，本轮不实现。
4. Tauri 默认 CSS 位于 `hosts/narrava-loom-tauri/frontend/`。游戏不提供 CSS 也必须获得完整、
   可访问、响应式的默认界面；作者可在游戏根目录 `styles/` 提供 CSS，并按路径稳定顺序
   追加在默认样式之后。
5. 游戏 ECMAScript Runtime 位于 Tauri Rust Runtime Worker，不位于 WebView。这样 State 读取、
   Expression 函数和同步 Macro 不需要跨异步 IPC；JS Promise 映射 Core Pending/Resume。
6. WebView 只拥有 Host Renderer，不承载 Engine、ModLoader 或 Core Presentation 语义。
7. `.nar` 保存 `resources/<logical-path>`，清单记录每个资源哈希；Script Bundle 继续由源码记录
   建立。

## 作者侧 API

`bindings/typescript/narrava.d.ts` 声明以下全局只读单例：

- `Engine`：启动状态、导航入口以及原子重新开始 `restart()`；
- `State`：global、setup、`$`、`_` 的显式读写；
- `Macro`：add/update/del/get/has、before/after/off；
- `Story`：has、current、get、visits；导航由 `Engine` 提供；
- `Logger`：结构化日志；
- `Event`：emit/subscribe/take/unsubscribe；
- `Save`：捕获、导入导出请求；
- `Resource`：has/paths/pick/info/read/text；
- `I18n`：当前 locale 与默认 locale；正文翻译由 Core 渲染链自动解析。
- `Presentation`：安全建立 StyledText、Image、Region、Action、Component 与 fallback 片段。
- `Host`：当前提供 `delay(milliseconds)`，把 Promise 映射为 Core Pending/Resume。

`ModLoader` 与 `ModUtils` 不在该声明文件中提前出现。Host Renderer API 若开放给游戏脚本，
放在 Tauri Host 自己的可选声明扩展中，不伪装成跨 Host Core API。

## Resource 契约

- 逻辑路径使用 `/`，拒绝绝对路径、空段、`.`、`..`、反斜杠和重复路径；
- Core 不按扩展名拒绝资源；媒体类型由显式值或受控扩展表推断，未知时为
  `application/octet-stream`；
- `ResourceCatalog::discover()` 只读取路径、媒体类型和文件大小；`read()`/UTF-8 `text()`
  首次访问单个磁盘文件时才读取并缓存成功结果，I/O 错误不会伪装成“资源不存在”；
- `.nar`/`.nres` 的内存 backing 使用共享不可变字节，跨 Host adapter clone 不复制整份内容；
- 返回字节不允许调用者修改目录内部数据；
- `.nar` 对清单、源码、Bytecode 和每个资源分别校验哈希；
- 基础 Core 只有 `game` 来源。模组来源与覆盖顺序以后由 ModLoader 组合。

## Event 契约

- Event 使用稳定序号、名称和拥有型 `Value` 载荷；
- `emit` 先保存事实，再投递给当时存在且匹配的订阅；
- `subscribe` 返回进程内稳定 ID；`take` 一次性取走待处理事件；
- `unsubscribe` 释放订阅及队列；`clear` 清空历史与队列但不重置序号；
- ScriptCallable 等不可拥有平台函数的数据不得作为跨边界事件载荷。
- Tauri Host 把五阶段 Passage 生命周期发布为保留事件
  `passage:init/start/render/display/end`；统一载荷为 Passage 名和 tags，作者不能伪造保留名。

## Tauri Renderer

- 默认 DOM 使用 `nv-story`、`nv-passage` 和五个稳定插槽：`passage-header`、
  `passage-main`、`passage-footer`、`bar`、`dialog`；
- Text 不解释 HTML；StyledText 使用语义元素；Navigation/SafeReturn/Action 使用原生 `button`；
- 所有动作可键盘操作，状态使用 `aria-live`，支持 reduced-motion；
- 默认 CSS 内置于 Host。没有游戏 CSS 时仍完整可用；
- 游戏作者可选的 `styles/*.css` 由 Tauri Host 发现和拼接，不进入 Twee/Script Core Source；
  推荐只使用 `nv-story`、主题变量和五个稳定插槽，其他内部 DOM 不构成兼容 API；
- 作者 CSS 使用正常层叠覆盖默认变量和规则，不能替换 Renderer 脚本或注入 Rust 配置；
- `resource("path")` 是 Host CSS 解析能力，路径交给 Core Resource 目录解析；Tauri 将其改写为
  `narrava-resource://` 按需请求，不通过 assets IPC 发送全部字节；
- URL 中的 `localhost` 只是 Tauri 自定义协议要求的虚拟 host，不监听端口、不访问网络，
  也不依赖开发服务器；开发运行与正式安装包使用同一条进程内 Resource resolver；
- `HostAssetsDto` 只包含 path/mediaType/size 元数据。Boa 的启动配置同样不含 bytes/text，
  `Resource.read/text` 通过只读原生 adapter 在调用时解析单个资源；
- Renderer 按 `PresentationKey` 协调 DOM；Image、Region、Dismiss Action 和 `meter@1` Component
  已接通，未知 Component 使用 fallback；WebView 不反向拥有 State 或 Story。

## 技术栈与命令

- Core/Host：Rust 2024、Serde、Tauri 2；
- ECMAScript：Worker 使用纯 Rust Boa；microtask 由 Boa job queue 排空，`Host.delay()` 通过
  Core continuation 暂停和恢复事务；
- TypeScript：Worker 使用 Oxc Transformer 去除类型后交给 Boa，作者无需预构建；
- 验证：
  - `cargo test --workspace --all-targets`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --all -- --check`
  - `cargo run -p narrava-loom-core -- examples`
  - `cargo run -p narrava-loom-tauri -- examples`

## 测试策略

- Resource、Event、Script API 先写 Core 单元测试；
- `.nar` 增加资源往返、路径攻击、重复资源和哈希篡改测试；
- Tauri Worker 使用无窗口集成测试验证 TS/JS 加载、State、Macro、Promise 和错误映射；
- Renderer 使用真实浏览器检查 DOM、控制台、键盘、可访问树和 320/768/1024/1440 宽度；
- 最终以唯一 `examples/` 验证作者项目不含 Rust、无需 CSS。

## 边界

- 始终：保持 Core 无 DOM/URL/ECMAScript 引擎依赖；验证所有外部路径和字节；保持错误码稳定。
- 后续：把需要计时器、网络或平台回调的 Promise 接入 Core Pending／Resume／Cancel；
- 不做：ModLoader/ModUtils、模组资源覆盖、在 Core 中引入特定 Renderer 协议、让游戏作者编写
  Rust。

## 验收条件

1. Core Resource/Event 有稳定公开类型、测试和 `.nar` 完整性链。
2. `.d.ts` 与 Rust Script API 对应，不声明占位功能。
3. Tauri Worker 实际执行示例 TypeScript，脚本能使用列出的 Core API。
4. 同步 Macro 与 Promise Macro 都能经 VM 正确完成或恢复。
5. 默认 Renderer 无作者 CSS也可用；提供 `styles/*.css` 时能覆盖默认外观；真实浏览器无错误
   并满足键盘/可访问性检查。
6. 文档不再把未接入的 ECMAScript、Resource 或 Event 描述为已完成。

## 已确认决定

- 游戏 ECMAScript 位于 Rust Worker，WebView 仅负责 Renderer；
- Tauri 提供完整默认 CSS，作者可以选择 `styles/*.css` 自定义容器外观；
- Renderer 稳定插槽为 `passage-header`、`passage-main`、`passage-footer`、`bar`、`dialog`；
- ECMAScript 使用 Boa，TypeScript 转译使用 Oxc；两者均已由最小原型和 Host 测试验证。

# Runtime Session

`RuntimeSession` 是一局游戏的所有权根。Tauri Worker 与 TUI 直接调用 `execute`，
传入 owned `RuntimeCommand`，接收 `RuntimeUpdate`；Host 负责 IO、等待和呈现。

```text
Tauri commands → Worker ┐
                       ├→ RuntimeSession → Core HostApi → Engine / VM
TUI command loop ───────┘         ↕                        ↕
                            EcmaBinding            State / Story
                                 ↓
                      SemanticOutput → Protocol DTO → Host Renderer
```

## 所有权

Session 持有 State、Story、交互、上一帧、continuation 和命令事务。`RuntimeData` 只保存游戏
身份、I18n 目录和已验证语言包。Protocol 定义 owned、serializable 数据；Adapter 负责转换
Core 与 Script、Host 的类型。实现入口统一见[源码索引](source-record.md#script-runtime)。

官方 Host 没有 Native Session registry。`RuntimeSessionId`、request/response envelope 是
Protocol 数据契约，不对应独立的 Handle/Driver 执行对象。`EcmaBinding` 直接持有实际
ECMAScript 实现；Core 通过 `ScriptCallDispatcher` 调回脚本，避免依赖 Script crate。

## 命令与事务

命令入口先校验当前是否允许新命令，或 resume/cancel 是否匹配挂起操作。
被拒绝的命令不改变活动事务。执行链为：

```text
command → checkpoint → execute → pending / commit / rollback
```

`RuntimeTransaction` 整体保存完整 State 检查点、Story 快照、Reaction 次数状态、
交互表、上一帧，以及 Reaction 比较基线和未完成输出。Pending 期间保留同一事务；
Reaction 导航延续它，结算完成后一次释放。执行错误或取消恢复整个事务。

State 的两种快照有不同内容：`StateCheckpoint` 覆盖全部命名空间、Location 定义与位置，用于
短期回滚；两种快照均保存随机状态。`StateSnapshot` 保存持久变量与 `LocationState`，用于历史、Save 和 Reaction 变化比较。
地点定义在脚本装载结束后固定，不复制进持久快照。
Story 快照恢复时间线，但不回退身份分配高水位。

Core Engine 的检查点负责单条执行链；Session 事务还覆盖执行链完成后的 Reaction、
特殊区域和呈现状态，不能把两者视为同一生命周期。特殊区域使用隔离的 State/Story 视图。

Save Import 使用 Resume 的命令事务。失败先回滚，再通知 `Save.after`；允许作为 notice
报告的失败继续从恢复后的状态结算。Input 的脚本存档请求额外保留原输入的完整命令事务，跨越 Reaction 导航与 Pending 恢复：
编码或 IO 失败时一起恢复 State、Story、Reaction、交互表和上一帧。只有成功建立保存请求后才移交
事务，不重复捕获，也不会在编码失败时提前丢失恢复点。
脚本 State API 直接访问活动 Rust State，没有 JS 变量镜像或同步步骤。

## Pending 与 Host

- `continuations` 保存 Core 执行所有权，包含 VM frame、局部域及 Script 挂起凭据；
- `pending` 表示等待 Host 的唯一操作，并记录恢复主链、特殊区域还是 Save/语言操作；
- 操作元数据直接取自 continuation，不保留 `scheduled` 副本或独立 waiting 状态。

Protocol 只公开 operation ID、请求和完成结果。Tauri facade 异步等待 timer 或文件 IO，
完成后向 Worker 发送 Resume；TUI 同步完成相同操作。两端都不直接恢复 VM 或修改 State。

语言刷新和 Import 重绘统一使用 `RefreshCurrent`，不新增历史项。Location Adapter 的只读视图
跨 Pending 保留；完成、错误或取消时清除。刷新途中发生新导航时立即解除，包括同名导航。
作者可见的位置行为及与历史重放的区别见 [Location](../author/location.md#刷新历史与存档)。

## 信任边界与验证

用户交互身份和值、Host 完成结果、Script 返回值、Save 内容、资源路径与语言包仍严格校验。
Session ID 的构造与反序列化使用同一规则。已安装 Boa slot、私有事务和已登记 continuation
属于内部不变量。语言包格式校验与绑定当前 I18n 目录的校验保留，
因为它们验证不同约束。

状态机测试使用真实 ECMAScript、编译管线和 RuntimeCommand，覆盖多次挂起、特殊区域、
错误操作 ID、取消、可重试回滚、历史、Save round-trip、Host 失败及语言刷新。

## 契约生成

Protocol Rust 数据声明是 wire 字段与 variant 的唯一来源。
[`bindings/script-contract.json`](../../bindings/script-contract.json) 保存脚本全局、内建事件、
Surface builder 名称与协议版本。`contract:generate` 从这两处生成 TypeScript DTO 和 Rust
名称目录；`contract:check` 拒绝生成物漂移。生成器仅接受当前 DTO 使用的数据声明语法，
遇到不支持的类型会失败。

Bootstrap 的 TypeScript 源码位于 `crates/narrava-loom-script/bootstrap/`，由 Bun 在开发期
打包为嵌入 Rust 的 ECMAScript。运行游戏时由 Boa 执行，不依赖 Bun。

## 开发命令、只读检查与随机状态

`debug_snapshot()` 是独立查询，返回 `HostDebugSnapshotDto`，不进入 `RuntimeCommand`，
也不求值作者表达式。Pending 时拒绝查询；State 值图的循环、深度、集合数量和节点数都有
明确显示边界，Location 预览最多展示 128 个地点及各 128 个顶点。完整数据仍在 State 中。

`DebugScript` 与输入共用 State/Story/Reaction 事务及安全点，宿主入口校验 developer 权限。
命令在已加载作者脚本的 Boa realm 中求值。独立 JobExecutor 隔离控制台 Promise 队列，
受管等待通过 `Pending::Console` 恢复或取消；失败会清理队列、作者事件和待提交的 Host 请求。
Engine 导航复用正常 Passage 流程，Save 复用存档事务。错误的 operation ID 不会清除挂起任务。

`HostDebugEvaluationDto` 保存有界的对象树结果，与 Logger 分离；快照读取不会重新执行脚本。
成员补全只解析属性路径，不执行表达式、普通 getter 或未知 Proxy trap。内置参数与帮助从
`bindings/typescript/narrava.d.ts` 生成，由 `console:check` 检查漂移。
临时 realm 声明与注册不承诺事务回滚，控制台也不强制重放有副作用的 Passage。

SplitMix64 状态由 Core State 持有；脚本 Random/Math.random 与表达式抽样共用这一源。
RefreshCurrent 使用临时随机游标重放正文并重建交互，保留已提交 State；公共区域在隔离视图
中执行。刷新不是任意脚本的纯渲染器：正文输出仍按重放生成，带副作用的作者脚本仍会执行。
输入呈现从权威 State 同步。后续真实导航结束刷新视图，恢复正常提交。

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

## 所有权与源码入口

- `session.rs` 持有 State、Story、交互、上一帧、continuation 和命令事务；
- `session/state_io.rs` 处理 Save 与语言选择；`RuntimeData` 只保存游戏身份、I18n 目录和已验证语言包；
- `dispatch.rs` 分派 Macro，`reaction_runtime.rs` 执行 Reaction 效果；
- `state_adapter.rs`、`resource_adapter.rs`、`reaction_adapter.rs` 是 Boa 与 Core 类型/API 的适配边界；
- `protocol_adapter/script_output.rs` 校验作者 `Surface` builder 数据，直接生成 Core `SemanticOutput`；
- `protocol_adapter/host_update.rs` 将 Core 输出转换为 owned Host DTO；
- `narrava-loom-protocol` 只定义 owned、serializable 数据，不依赖 Core、Script 或 Host。

官方 Host 没有 Native Session registry。`RuntimeSessionId`、request/response envelope 是
Protocol 数据契约，不再对应另一层 Handle/Driver 执行对象。`EcmaBinding` 直接持有实际
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

State 的两种快照有不同内容：`StateCheckpoint` 覆盖全部命名空间，用于短期回滚；
`StateSnapshot` 只包含持久变量，用于历史、Save 和 Reaction 变化比较。
Story 快照恢复时间线，但不回退身份分配高水位。

Core Engine 的检查点负责单条执行链；Session 事务还覆盖执行链完成后的 Reaction、
特殊区域和呈现状态，不能把两者视为同一生命周期。特殊区域使用隔离的 State/Story 视图。

Save Import 使用 Resume 的命令事务。失败先回滚，再通知 `Save.after`；允许作为 notice
报告的失败继续从恢复后的状态结算。Input 的脚本存档请求额外携带原输入前 State 检查点：
它跨越已经完成的输入命令，用于后续 IO 失败时撤销输入。该检查点从命令事务移交，不重复捕获。
脚本 State API 直接访问活动 Rust State，没有 JS 变量镜像或同步步骤。

## Pending 与 Host

- `continuations` 保存 Core 执行所有权，包含 VM frame、局部域及 Script 挂起凭据；
- `pending` 表示等待 Host 的唯一操作，并记录恢复主链、特殊区域还是 Save/语言操作；
- 操作元数据直接取自 continuation，不保留 `scheduled` 副本或独立 waiting 状态。

Protocol 只公开 operation ID、请求和完成结果。Tauri facade 异步等待 timer 或文件 IO，
完成后向 Worker 发送 Resume；TUI 同步完成相同操作。两端都不直接恢复 VM 或修改 State。
语言刷新和 Import 重绘统一使用 `RefreshCurrent`，不增加同名历史重访。

## 信任边界与验证

用户交互身份和值、Host 完成结果、Script 返回值、Save 内容、资源路径与语言包仍严格校验。
Session ID 的构造与反序列化使用同一规则。已安装 Boa slot、私有事务和已登记 continuation
属于内部不变量；不再以可恢复错误重复检查。语言包格式校验与绑定当前 I18n 目录的校验保留，
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

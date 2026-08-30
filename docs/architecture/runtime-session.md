# Runtime Session 收敛规格

> 状态：Tauri 与 TUI 已接入

## 目标

统一 Native Host 对 Engine、Macro continuation、脚本事件和挂起操作的编排。Tauri 与 TUI
只负责装载平台文件、等待平台操作、Renderer 与玩家输入，不各自保存第二套 Narrava 生命周期。

统一边界为：

```text
RuntimeCommand → RuntimeSession → RuntimeUpdate | PendingOperation
```

## 所有权

- 零 Core 依赖的 `narrava-loom-protocol` 拥有可序列化的 `RuntimeCommand`、`RuntimeUpdate`、`PendingOperation` 与 Host DTO；
- `narrava-loom-script::protocol_adapter` 负责 Surface builder 校验与 Core 输出到拥有型 DTO 的转换；
- Native RuntimeSession 内部借用已准备的 HIR/Bytecode，独占 State、Story、interaction 与 continuation；
- 跨语言侧只保存无 Rust 生命周期的 `RuntimeSessionHandle`；Native registry 以 `RuntimeSessionDriver` 保存实际编译借用与 ScriptAdapter；
- Script Contract 由 `ScriptAdapter` 表达，Boa/Oxc 的 `EcmaBinding` 只是当前实现；
- Host 只保存平台资源和 IO 句柄，不读取 Engine continuation；
- Boa/Oxc 是 `narrava-loom-script` 的 ECMAScript Adapter，不进入 Script Contract 或 Protocol；
- JavaScript `Surface` 只构造 Protocol 已定义的 Surface 节点，不拥有第二套语义。

`narrava-loom-script::RuntimeSession` 是内部 Native 实现，Tauri/TUI 共同消费
`RuntimeSessionDriver`，跨语言调用方只看到 `RuntimeSessionHandle`。两端不再直接调用
`HostApi::start_mir`、`advance_mir`、`resume_pending` 或 `render_special_mir`，也不保存
`HostPendingExecutions`。Host 按 `PendingOperation` 完成 delay、Save 文件 IO 或语言平台确认，
再用同一 operation ID 和拥有型 `PendingResult` 恢复 Runtime。

## 当前命令

- `start`：启动当前单局 Session；首帧产生后再次启动会被拒绝；
- `back`／`forward`：沿 Story 游标重放历史 Passage，不新增访问记录；
- `activate`：激活上一份更新公开的 interaction；
- `input`：提交上一份更新公开且校验通过的输入值；
- `save`：Runtime 先准备拥有型平台请求；Host 只读写文件，Resume 后由 Session 验证、恢复并同步 Script State；
- `selectLanguage`：产生平台挂起请求，Resume 后由 Session 原子提交 Script locale 与当前语言；
- `resume`／`cancel`：以不透明 operation ID 恢复或取消挂起操作。

每条成功命令在呈现边界进入一次 Reaction 安全点：Runtime 排空作者 Event、比较命令前后的
持久 State，顺序执行结构化效果，并把 Reaction `goto` 送回同一 Engine continuation 链。
State Reaction 的首轮 `before/after` 分别来自命令开始与安全点进入时的快照；它记录提交边界，
不保存 setter 级变更日志。效果引发的新变化使用效果前后的快照继续下一轮。
条件、效果、导航、恢复或 Script 同步失败时，State、Story、Reaction 状态、交互表和上一份
可展示更新一起回滚；pending 期间检查点由 Session 持有，Host 不参与事务。

命令集合只归纳现有能力，不增加新的作者 API。

RuntimeSession 的状态机测试直接替换 `ScriptAdapter`，覆盖未启动命令、挂起期拒绝新命令、
operation mismatch 不丢失 continuation、cancel、再次 pending 以及特殊区域 pending。

每份 Ready 更新携带 `can_back`／`can_forward`。Tauri 据此启用侧栏历史按钮，TUI 使用
`back`／`forward`（简写 `b`／`f`）；Host 不用浏览器或终端自己的历史代替 Story。

## 挂起模型

`Host.delay`、Save 与语言选择共用一个 pending/resume 状态机。RuntimeSession 保存真实 Engine
continuation和平台事务上下文；Protocol 只公开拥有型请求、operation ID 与完成结果。Host 不得持有
或伪造 VM frame，也不能直接修改 State/Story。未来操作只能增加新的 tagged variant。

## Canonical Script Contract

[`bindings/script-contract.json`](../../bindings/script-contract.json) 是作者脚本全局名称、内建事件和
Surface builder 种类的 canonical 清单。`bun run contract:generate` 从中生成 Rust 名称目录和
TypeScript 标签联合类型、Runtime command/update/pending/result/envelope 结构与协议版本；
`bun run contract:check` 禁止生成物漂移。内部 Bootstrap 源码按职责位于
`crates/narrava-loom-script/bootstrap/`；开发时由 Bun 打包为一个已提交的 IIFE，Rust 仅通过
`include_str!` 在编译期嵌入该生成文件。最终 Runtime 仍由 Boa 执行 ECMAScript，不携带或调用 Bun。
Bootstrap 读取同一 canonical 清单建立内建事件集合，并验证全部全局对象与 Surface builder 已真实安装。

Save 文件读写是 Host IO，但存档捕获、解析、兼容校验、State/Story 恢复和 Script 同步均在
RuntimeSession 的恢复事务内完成。内部 `RuntimeServices` 只准备/应用 Core 数据，不执行文件选择和读写；
直接 UI 操作与脚本请求进入同一命令流。

## 本阶段不包含

ModLoader、Godot Host、Python/Java Binding、新 Renderer、新 Host capability，以及二维坐标系统
均不属于本阶段。

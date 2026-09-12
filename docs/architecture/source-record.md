# 源码与模块索引

本页只说明源码存放位置与模块职责。运行行为见各架构页，目录规则见
[仓库布局](../development/repository-layout.md)，命令见[仓库命令](../development/commands.md)。

## Core 编译与执行

| 模块                | 职责                                                  |
| ------------------- | ----------------------------------------------------- |
| `config` / `source` | 项目配置、Source 路径、发现与读取                     |
| `twee`              | Twee 词法、AST、Fragment 和 Story 聚合                |
| `hir`               | AST 到 HIR lowering 与 Widget 静态校验                |
| `expression`        | Expression AST、Parser、Evaluator、Value 与 Prototype |
| `macro_runtime`     | Definition、参数、调用帧、Widget 与 suspension        |
| `mir`               | 显式控制流、迭代槽、Passage 身份与执行位置            |
| `lir`               | Passage 索引、重名与跳转地址验证                      |
| `bytecode`          | 可序列化的拥有型 VM 指令、操作数与常量目录            |
| `vm`                | Bytecode frame、调用栈、迭代状态与 Surface 单步执行   |
| `runtime`           | Macro/Widget 执行、控制信号与 Runtime 身份            |
| `engine`            | 启动、生命周期、导航、continuation 与事务             |

## Core 领域与支撑

| 模块                           | 职责                                                                                       |
| ------------------------------ | ------------------------------------------------------------------------------------------ |
| `state`                        | 命名空间、Value 图、世界状态与检查点                                                       |
| `world`                        | World 类型重导出、Passage 地点与环境 Tag 绑定                                              |
| `story`                        | Passage 查询、history、游标与待确认请求                                                    |
| `semantic`                     | Host-neutral Surface 语义                                                                  |
| `host`                         | Host 输入、Core 更新与统一 Diagnostic 边界                                                 |
| `events` / `reaction`          | 结构化事件与 Reaction 类型；`reaction/registry.rs` 管理索引和状态，`resolve.rs` 解析触发链 |
| `i18n`                         | 文本目录、语言包、校验、字典与 fallback                                                    |
| `save`                         | 版本化存档、Value 图、兼容校验与原子恢复                                                   |
| `resource` / `nar` / `release` | 资源、发布容器与交付目录                                                                   |
| `diagnostic` / `logger`        | 稳定问题数据与结构化运行记录                                                               |
| `script`                       | 有序 Script Bundle 与 ScriptCallDispatcher 函数边界                                        |

## Script Runtime

以下模块位于 `crates/narrava-loom-script/src/`：

| 模块                                          | 职责                                                     |
| --------------------------------------------- | -------------------------------------------------------- |
| `session.rs` / `session/`                     | 命令事务、导航与历史、交互、Reaction 结算、公共区域和 IO |
| `ecma.rs` / `binding/`                        | 脚本装载、转译及 Macro、Reaction、Audio、Save 等桥接     |
| `dispatch.rs` / `reaction_runtime.rs`         | Macro 分派与 Reaction 效果执行                           |
| `state_adapter.rs` / `world_adapter.rs`       | 活动 State 访问、World 注册与刷新视图                    |
| `resource_adapter.rs` / `reaction_adapter.rs` | Resource 与 Reaction 的 Boa 类型/API 适配                |
| `protocol_adapter/`                           | 校验 Script 输出为 Core 语义，再转换为 Host DTO          |

## 外部契约

| 路径                                      | 职责                                                       |
| ----------------------------------------- | ---------------------------------------------------------- |
| `bindings/script-contract.json`           | 脚本全局、内建事件、builder 名称与协议版本                 |
| `bindings/typescript/narrava.d.ts`        | 游戏 scripts 的 TypeScript API                             |
| `crates/narrava-loom-world/`              | 地点定义、二维多边形与位置规则，独立于编译器和宿主         |
| `hosts/narrava-loom-tui/src/renderer/`    | Core/Protocol 节点适配及终端布局，帧缓冲留在 `renderer.rs` |
| `hosts/audio.rs` / `hosts/tests/audio.rs` | 共享本地音频后端与离线回归                                 |
| `editors/vscode-narrava-loom/`            | Twee 语法、导航与 VS Code 扩展                             |
| `examples/`                               | 无 Rust 的综合游戏示例                                     |

# 源码与模块索引

本页只说明源码存放位置与模块职责。运行行为见各架构页，目录规则见
[仓库布局](../development/repository-layout.md)，命令见[仓库命令](../development/commands.md)。

## Crate

| 路径 | 职责 |
|---|---|
| `src/lib.rs` | `narrava-loom-core` library 入口 |
| `src/main.rs` | 最小 CLI Host，不拥有 Core 实现 |
| `crates/narrava-loom-protocol/` | 零 Core 依赖的拥有型 Runtime/Host DTO |
| `crates/narrava-loom-script/` | Boa ECMAScript、RuntimeSession 与 Core/Protocol 适配 |
| `hosts/narrava-loom-tauri/` | Tauri Worker、平台 IO 与 WebView Renderer |
| `hosts/narrava-loom-tui/` | 终端 Host 与 Protocol 语义验证 |

## Core 编译与执行

| 模块 | 职责 |
|---|---|
| `config` / `source` | 项目配置、Source 路径、发现与读取 |
| `twee` | Twee 词法、AST、Fragment 和 Story 聚合 |
| `hir` | AST 到 HIR lowering 与 Widget 静态校验 |
| `expression` | Expression AST、Parser、Evaluator、Value 与 Prototype |
| `macro_runtime` | Definition、参数、调用帧、Widget 与 suspension |
| `mir` | 显式控制流、迭代槽、Passage 身份与执行位置 |
| `lir` | Passage 索引、重名与跳转地址验证 |
| `bytecode` | 可序列化的拥有型 VM 指令、操作数与常量目录 |
| `vm` | Bytecode frame、调用栈、迭代状态与 Surface 单步执行 |
| `runtime` | Macro/Widget 执行、控制信号与 Runtime 身份 |
| `engine` | 启动、生命周期、导航、continuation 与事务 |

## Core 领域与支撑

| 模块 | 职责 |
|---|---|
| `state` | 命名空间、Value 图与检查点 |
| `story` | Passage 查询、history、游标与待确认请求 |
| `semantic` | Host-neutral Surface 语义 |
| `host` | Host 输入、Core 更新与统一 Diagnostic 边界 |
| `events` / `reaction` | 结构化事件、订阅与 Reaction 触发状态 |
| `i18n` | 文本目录、语言包、校验、字典与 fallback |
| `save` | 版本化存档、Value 图、兼容校验与原子恢复 |
| `resource` / `nar` / `release` | 资源、发布容器与交付目录 |
| `diagnostic` / `logger` | 稳定问题数据与结构化运行记录 |
| `script` | Script Bundle、加载上下文与 Binding 契约 |

## 外部契约

| 路径 | 职责 |
|---|---|
| `bindings/script-contract.json` | 跨 Rust/TypeScript 的 canonical 标签和 DTO 名称 |
| `bindings/typescript/narrava.d.ts` | 游戏 scripts 的 TypeScript API |
| `crates/narrava-loom-script/src/protocol_adapter/` | Core Surface 与 Protocol DTO 转换 |
| `editors/vscode-narrava-loom/` | Twee 语法、导航与 VS Code 扩展 |
| `examples/` | 无 Rust 的综合游戏示例 |

Core 源码只识别 `.twee`、`.ts` 和 `.js`。`SourcePath` 使用相对路径与 `/`，拒绝
绝对路径、`..` 和反斜杠。`.ts/.js` 不进入 Narrative IR，`.css` 由具体 Host 管理。

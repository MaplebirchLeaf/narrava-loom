# 仓库开发指南

先确定改动属于哪个领域，再查其实现与测试。依赖关系见[架构](../architecture/README.md)，
代码组织规则见[源码规范](code-style.md)。

## 工作顺序

1. 检查当前工作区，保留已有改动；阅读受影响的作者契约与实现。
2. 在所属模块完成一个可验证的改动，跨 Host 字段同步更新两端与生成物。
3. 运行[仓库命令](commands.md)中的针对性检查，再完成工作区门禁。
4. Host 行为变更按[测试指南](testing.md)验收；提交说明写清自动测试与真实设备的区别。
5. 更新唯一的主题文档。版本按[发布周期](release.md)管理，开发提交不单独涨版本。

## 源码入口

| 位置 | 责任 |
| --- | --- |
| [src/config.rs](../../src/config.rs)、[src/source.rs](../../src/source.rs) | 配置、游戏身份与内容发现 |
| [src/twee/](../../src/twee)、[src/hir/](../../src/hir)、[src/mir/](../../src/mir) | 叙事语法与降低 |
| [src/lir.rs](../../src/lir.rs)、[src/bytecode.rs](../../src/bytecode.rs)、[src/vm/](../../src/vm) | 可执行编码与 VM |
| [src/expression/](../../src/expression)、[src/macro_runtime/](../../src/macro_runtime) | 表达式与 Macro |
| [src/engine/](../../src/engine)、[src/state.rs](../../src/state.rs)、[src/story/](../../src/story) | 运行进度、状态与历史 |
| [src/reaction/](../../src/reaction)、[src/events.rs](../../src/events.rs) | 事件与声明式响应 |
| [src/i18n/](../../src/i18n)、[src/save/](../../src/save)、[src/resource/](../../src/resource) | 语言、存档与资源 |
| [src/nar.rs](../../src/nar.rs)、[src/release.rs](../../src/release.rs) | 容器与发行目录 |
| [crates/narrava-loom-location/](../../crates/narrava-loom-location) | 独立地点与几何领域 |
| [crates/narrava-loom-protocol/](../../crates/narrava-loom-protocol) | 不依赖 Core 的 DTO |
| [crates/narrava-loom-script/src/](../../crates/narrava-loom-script/src) | RuntimeSession、Boa 与领域 Adapter |
| [hosts/](../../hosts) | Tauri、TUI、共享 audio/save_io 与共享测试 |
| [bindings/](../../bindings)、[editors/](../../editors) | 作者契约与编辑器 |
| [examples/](../../examples) | 可运行、无 Rust 的作者内容 |
| [scripts/](../../scripts)、[.github/workflows/](../../.github/workflows) | 生成、检查、构建和 CI |

Core 公共语义留在 `src/`，平台类型留在对应 Host，跨 Core/Protocol 转换留在 Script。
新算法和测试优先放入所属领域，不新增同义包装或独立状态副本。

## 生成文件

| 来源 | 生成物 | 命令 |
| --- | --- | --- |
| Protocol Rust DTO + `bindings/script-contract.json` | Rust 名称目录、TS DTO | `bun run contract:generate` |
| `crates/narrava-loom-script/bootstrap/*.ts` | 嵌入的 bootstrap.generated.js | `bun run bootstrap:build` |
| `bindings/typescript/narrava.d.ts` | 控制台参数与双语帮助 | `bun run console:generate` |

修改来源再生成，禁止直接维护生成物。详细全局签名在作者声明中人工维护，覆盖测试验证契约。
运行游戏不需要 Bun，Bun 只用于开发期生成和检查。

`target/` 是 Cargo 缓存，`dist/` 是可交付游戏和 VSIX，二者都不入库。
`node_modules/`、本机配置、临时日志与 Tauri 平台生成目录不放进源码树。

## 文档维护

| 目录 | 唯一责任 |
| --- | --- |
| `author/` | 按任务提供操作步骤与示例 |
| `reference/` | 当前可接受的配置、语法、参数与边界 |
| `architecture/` | 所有权、数据流、不变量与取舍 |
| `development/` | 仓库组织、工具、验证与发布 |

每个分区由 README 提供阅读路径，根文档目录只分流。一个主题只有一处详细说明，
其他页面链接它；状态与未完成项集中在[项目状态](status.md)，历史归 CHANGELOG 和 Git。
不要保留过期进度日期、开发过程流水账或“以后会实现”的 API 示例。

重命名文件时同步所有链接与标题锚点；用 `bun run docs:check` 检查本地链接和文档可达性。
可复制的最小示例应实际编译；片段应注明需要的变量、资源或已注册内容。

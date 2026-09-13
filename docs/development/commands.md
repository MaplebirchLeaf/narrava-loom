# 仓库命令

以下命令都从仓库根目录运行。`--locked` 要求使用已提交的锁文件，适合 CI 和完成前验证；
日常开发也建议保留，避免依赖解析结果意外变化。

## 游戏与 Host

| 命令 | 作用 |
|---|---|
| `cargo run --locked -p narrava-loom-core -- examples` | 编译示例的 Config、Source、Twee、IR 与 Bytecode，不打开窗口 |
| `cargo run --locked -p narrava-loom-tauri -- examples` | 启动 Tauri 桌面 Host；这不是 Android/iOS 命令 |
| `cargo run --locked -p narrava-loom-tui -- examples` | 用根目录示例游戏驱动 TUI Host（终端交互） |

把 `examples` 换成游戏目录即可检查或启动其他项目。Core CLI 的 `--` 用来结束 Cargo 参数，
其后的路径交给 Narrava 程序。

## Rust 质量门禁

| 命令 | 作用 |
|---|---|
| `cargo fmt --all -- --check` | 检查 Rust 格式，不修改文件 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 检查整个 workspace，并把警告视为错误 |
| `cargo test --workspace --all-targets --locked` | 运行 Core、Protocol、Script、Tauri 与 TUI 的 Rust 测试 |
| `cargo doc --workspace --no-deps --locked` | 生成 workspace API 文档，不构建依赖文档 |

只修改一个 crate 时可把 workspace 命令缩窄为 `-p <crate>`；完成前仍应运行全工作区门禁。

## TypeScript、前端与编辑器

仓库使用 Bun 执行根脚本：

| 命令 | 作用 |
|---|---|
| `bun run check` | 依次检查文档、版本、契约生成物、Bootstrap、TypeScript、Oxlint、Oxfmt 和前端/编辑器测试 |
| `bun run bootstrap:build` | 将 Script Runtime 的内部 TypeScript bootstrap 打包为 Rust 编译期嵌入的单文件 JS |
| `bun run bootstrap:check` | 检查 bootstrap 生成物同步并严格检查内部 bridge 类型；不改文件 |
| `bun run vsix` | 测试并打包 Twee 扩展到 `dist/vscode-narrava-loom/`；可追加 `--install` |
| `bun run contract:generate` | 从 Protocol Rust DTO 与 `bindings/script-contract.json` 更新 Rust/TypeScript 生成文件 |
| `bun run contract:check` | 检查生成文件与两处契约来源同步，不写文件 |
| `bun run docs:check` | 检查本地 Markdown 链接、标题锚点及 docs 页面可达性 |
| `bun run typecheck` | 只检查 TypeScript 声明与示例脚本 |
| `bun run lint` | 只运行 Oxlint |
| `bun run format:check` | 检查 JS、TS、JSON 格式 |
| `bun run format` | 写入 JS、TS、JSON 格式化结果 |
| `bun run test:frontend` | 验证 Tauri Renderer 的文本、颜色、图片、状态条与弹窗契约 |
| `bun run test:vscode` | 验证 Twee 语法目录、grammar 与编辑器契约 |
| `bun run test:types` | 验证编辑器作者项目隔离，以及独立游戏完整加载类型包 |

不要再使用 `npx tsc` 作为本仓库标准命令；根脚本已经固定 workspace、配置和 Bun 工具链。

## 构建与版本

[打包游戏](../author/publishing.md)维护可移动目录的完整命令；
[版本与发布](release.md)维护统一版本、依赖与流水线规则。
`bun run vsix` 输出编辑器安装包到 `dist/vscode-narrava-loom/`。

Linux Host 的系统依赖与首次运行见[快速入门](../author/quick-start.md#准备环境)。

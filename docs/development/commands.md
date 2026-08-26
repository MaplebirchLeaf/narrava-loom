# 仓库命令

以下命令都从仓库根目录运行。`--locked` 要求使用已提交的锁文件，适合 CI 和完成前验证；
日常开发也建议保留，避免依赖解析结果意外变化。

## 游戏与 Host

| 命令 | 作用 |
|---|---|
| `cargo run --locked -p narrava-loom-core -- examples` | 编译示例的 Config、Source、Twee、IR 与 Bytecode，不打开窗口 |
| `cargo run --locked -p narrava-loom-tauri -- examples` | 启动 Tauri 桌面 Host；这不是 Android/iOS 命令 |
| `cargo run --locked -p narrava-loom-tui --example visual_demo` | 启动可输入编号、帮助、重绘和退出的交互式终端示例 |

把 `examples` 换成游戏目录即可检查或启动其他项目。Core CLI 的 `--` 用来结束 Cargo 参数，
其后的路径交给 Narrava 程序。

## Rust 质量门禁

| 命令 | 作用 |
|---|---|
| `cargo fmt --all -- --check` | 检查 Rust 格式，不修改文件 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 检查整个 workspace，并把警告视为错误 |
| `cargo test --workspace --all-targets --locked` | 运行 Core、Tauri 与 TUI 的 Rust 测试 |
| `cargo doc --workspace --no-deps --locked` | 生成 workspace API 文档，不构建依赖文档 |

只修改一个 crate 时可把 workspace 命令缩窄为 `-p <crate>`；完成前仍应运行全工作区门禁。

## TypeScript、前端与编辑器

仓库使用 Bun 执行根脚本：

| 命令 | 作用 |
|---|---|
| `bun run check` | 依次执行 TypeScript、Oxlint、Oxfmt、Tauri 前端测试和 VS Code 扩展测试 |
| `bun run typecheck` | 只检查 TypeScript 声明与示例脚本 |
| `bun run lint` | 只运行 Oxlint |
| `bun run format:check` | 检查 JS、TS、JSON 格式 |
| `bun run format` | 写入 JS、TS、JSON 格式化结果 |
| `bun run test:frontend` | 验证 Tauri Renderer 的 64 级 tone 等纯前端契约 |
| `bun run test:vscode` | 验证 Twee 语法目录、grammar 与编辑器契约 |

不要再使用 `npx tsc` 作为本仓库标准命令；根脚本已经固定 workspace、配置和 Bun 工具链。

## 发行与编辑器包

```bash
cargo build --release --locked -p narrava-loom-tauri
cargo run --release --locked -p narrava-loom-core -- \
  build examples dist/NarravaGame target/release/narrava-loom-tauri
scripts/build-vscode-extension.sh
```

第一条构建桌面 Host，第二条生成可移动桌面游戏目录，第三条输出 VSIX。构建器不会覆盖已经
存在的 `dist/NarravaGame`。当前仓库没有 Android/iOS 工程或移动打包脚本；移动构建要先用
Tauri 2 工具初始化目标平台，再补齐签名、权限、资源和真机验证，不能用上述桌面命令代替。

# 仓库布局与文件归属

本页规定源码、文档、示例和构建产物放在哪里。依赖方向与领域职责见
[总体架构](../architecture/overview.md)。

## 顶层目录

```text
Narrava Loom/
├── src/                            narrava-loom-core 源码与单元测试
├── hosts/
│   ├── narrava-loom-tauri/         官方 Tauri Host；桌面可运行，移动共享层待平台工程
│   └── narrava-loom-tui/           Host-neutral 终端 Renderer 与输入前端
├── crates/
│   ├── narrava-loom-protocol/      跨 Host 的 Surface 协议语义、双向转换与传输 DTO，依赖 Core
│   ├── narrava-loom-script/        ECMAScript 游戏脚本执行（Boa + Oxc）与宏分发，依赖 Core/Protocol
│   └── narrava-loom-modloader/     独立演进的可选附属，不属于 Core workspace
├── bindings/typescript/            游戏脚本 TypeScript 契约
├── editors/vscode-narrava-loom/    Twee 编辑器扩展源码
├── examples/                       唯一完整、无 Rust 的示例游戏
├── docs/                           作者、参考、架构与仓库开发文档
├── scripts/                        仓库级构建和打包入口
├── .github/workflows/              CI 与正式发行流水线
├── dist/                           可交付构建结果，不入库
└── target/                         Cargo 缓存，不入库
```

## 存放规则

- Core 公共语义、编译器和运行时放在 `src/`；不得导入 Tauri、DOM、CSS 或 ModLoader 类型。
- 平台实现放在 `hosts/<host>/`，Host 只通过 Core 的公开类型消费 Surface 和 Runtime；
  Host 与 Core 之间的传输 DTO 与脚本 bridge 位于 `crates/narrava-loom-protocol/`，
  依赖方向固定为 `host → narrava-loom-protocol → narrava-loom-core`。
- 游戏作者可直接复制或修改的内容放在 `examples/`；示例不得要求作者编写 Rust。
- 游戏脚本声明只在 `bindings/typescript/narrava.d.ts` 维护，编辑器扩展从公开语义目录提供辅助。
- 仓库操作脚本放在 `scripts/`；Host 内部的构建逻辑留在对应 Host crate。
- 教程放在 `docs/author/`，已实现契约放在 `docs/reference/`，设计理由放在
  `docs/architecture/`，仓库维护说明放在 `docs/development/`。
- `dist/` 只保存可分发结果，例如 `NarravaGame/` 和 `.vsix`；`target/` 只保存 Cargo 中间产物。
- `node_modules/`、Tauri `gen/`、本机配置和临时日志不得进入版本库。

## Rust 依赖方向

```text
narrava-loom-core
       ↑
narrava-loom-protocol   （Surface 语义 + 双向转换 + 传输 DTO）
       ↑
narrava-loom-script     （ECMAScript 执行 + 宏分发）
       ↑
Host / Binding / narrava-loom-modloader
       ↑
Host Renderer
```

`narrava-loom-modloader` 可以依赖 Core，Core 不能感知 ModLoader。Host 若需要模组能力，应显式依赖
两者，不得让 Core 提供 `mod_loader` feature 或另一套 Engine、State、Story、I18n 类型。

## 构建输出

所有可交付文件统一写入 `dist/`：

```text
dist/
├── NarravaGame/
│   ├── narrava
│   ├── game.nar
│   ├── languages/
│   ├── resources/
│   ├── mods/
│   └── save/
└── vscode-narrava-loom/
    └── *.vsix
```

`dist/` 与 `target/` 都被忽略，但职责不同：删除 `target/` 只清理编译缓存；删除 `dist/` 会删除本地
发行结果。构建脚本不得把交付物放进源码目录。

## 新文件判断

新增文件前依次判断：

1. 它是否是 Core 语义或测试？放入 `src/` 的对应领域目录。
2. 它是否依赖具体平台？放入对应 `hosts/` 子目录。
3. 它是否供游戏作者直接使用？放入 `examples/` 或 `bindings/`。
4. 它是否解释现有行为？按读者放入 `docs/` 四个分区之一。
5. 它是否由命令生成？放入 `dist/`、`target/` 或工具约定的已忽略缓存目录。

不要在根目录建立临时计划、测试输出或重复 API 清单。

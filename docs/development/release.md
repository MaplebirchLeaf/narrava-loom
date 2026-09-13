# 版本、依赖与发布

## 统一项目版本

根 `Cargo.toml` 的 package.version 是唯一项目版本来源。Rust crates、根 package.json、
作者类型包、VS Code 扩展和 Tauri 配置保持一致，Cargo/Bun 锁文件同步更新。

从 `0.10.0` 起按发布周期管理版本，开发提交不逐次递增。准备下一次发布时按累计变更选择：

| 变化 | 版本级别 |
| --- | --- |
| 仅修复 | patch |
| 新增兼容能力 | minor |
| 0.x 的破坏性 API 变化 | minor |
| 1.x 起的破坏性 API 变化 | major |

```bash
bun run version:bump patch
bun run version:check
```

把 patch 换成 minor 或 major 可选择升级级别。`bun run version:sync` 只将其他包与根版本对齐，
用于修正漂移。不要各自编辑包版本或为了拆分提交连续涨版本。

## 独立的兼容身份

| 身份 | 决定什么 |
| --- | --- |
| 项目包版本 | 引擎与工具 API 的发布周期 |
| 游戏 ID + 游戏版本 | 游戏内容与存档精确匹配 |
| 语言包版本 + game.versions | 翻译内容与目标游戏兼容范围 |
| Save schema / Bytecode / Protocol / NAR 格式版本 | 数据能否解码或执行 |

调整项目版本不重排数据格式版本，也不自动修改示例游戏和语言包身份。
既有提交和标签保留历史事实；历史日志不改写为当前行为。

## 公开边界与依赖

Twee 和脚本契约由[参考页](../reference/README.md)与作者类型声明维护；Rust pub 项由 Rustdoc 说明。
`pub` 表示 crate 外可访问，不等于 0.x 已承诺永久兼容。缩小已有可见性属于 API 变更，
不能藏在文档或格式整理中。新增异步操作必须定义请求、结果与取消语义。

Library 依赖使用兼容 semver 范围，已知不兼容时才固定版本。根 Cargo.lock 纳入仓库，
验证和发布使用 `--locked`；锁文件变化必须对应有意的依赖或本地包版本调整。

## 发布步骤

1. 根据累计变化确定版本，通过统一脚本同步包和锁文件。
2. 更新 CHANGELOG 的作者可见行为与迁移要求，不复制全部提交记录。
3. 完成[自动检查](commands.md)，按[测试指南](testing.md)记录真实 Host 验收。
4. 用[游戏构建流程](../author/publishing.md)验证可移动目录，并运行 `bun run vsix` 打包编辑器扩展。
5. 提交已验证内容；只有实际执行后才记录推送、标签与产物上传结果。

仓库 `.github/workflows/release.yml` 在 `v*` 标签或手动触发时构建 Linux、Windows、macOS
可移动游戏压缩包及 VSIX。工作流配置存在不等于本次已经发布，失败产物不应标为可用。

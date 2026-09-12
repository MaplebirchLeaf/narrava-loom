# 公开 API 与依赖锁定

## 公开边界

| 边界 | 面向谁 | 稳定性依据 |
|---|---|---|
| Twee、Expression、Macro | 游戏作者 | `docs/reference/` |
| `narrava.d.ts` 详细全局签名与说明 | TS/JS 游戏脚本 | `bindings/typescript/narrava.d.ts`（人工维护、由契约覆盖测试校验） |
| Script/Runtime 名称与 tagged union | Binding、Host、跨语言调用方 | Protocol Rust DTO 与 `bindings/script-contract.json` 共同生成类型和名称目录 |
| Rust `pub` 项 | Host 和工具作者 | Rustdoc 与语义化版本 |

Rust 中的 `pub` 只表示当前 crate 外可访问，不代表游戏作者需要 Rust，也不自动
代表 `0.x` 期间已承诺长期兼容。公开 Rust 项应有文档注释；编译器内部结构不应为了
方便调用而无条件扩大。缩小已有 `pub` 是破坏性变更，需要单独评审，不在文档整理中暗改。

## 异步边界

Core 已有 Pending/Resume/Cancel 的所有权模型。ECMAScript Binding 会立即排空 Boa
microtask；`Host.delay(ms)` 则建立真实 Core suspension，由 Rust Worker 到期后恢复原事务。
没有等待受管 Host 操作的未决 Promise 返回
`script.macro_unmanaged_promise`，不得伪装成 `undefined` 或普通 JSON 值。

Script Macro 的受管等待为 `Host.delay`；Session 另用 Pending 处理 Save 和语言请求。
新增平台操作必须定义各自的输入、输出与取消契约。

## 依赖锁定

- library 在 `Cargo.toml` 使用可兼容的 semver 范围，除非已知上游版本不兼容。
- 仓库提交根 `Cargo.lock`，Core CLI 和 Tauri Host 的可重现检查使用 `--locked`。
- 更新 lockfile 必须是有意的依赖更新，不应成为普通构建的副作用。

仓库标准检查见[仓库命令](commands.md)。

## 统一版本与提交

根 `Cargo.toml` 的 package.version 是项目版本来源。各 Rust crate、根 package.json、
作者 TypeScript 包、VS Code 扩展与 Tauri 配置保持相同版本；`bun run check` 会检查一致性。

每次提交前按变更性质递增语义化版本：修复用 patch，新增兼容能力用 minor；
0.x 阶段的破坏性 API 变化也递增 minor，1.x 起使用 major。不要分别修改包版本。

```bash
bun run version:bump patch
# 或 minor / major
```

命令会同步所有项目包版本并刷新 Cargo/Bun 锁文件。仅修正已有版本漂移使用
`bun run version:sync`，它不替代提交前的版本递增。
示例游戏版本与语言包兼容范围属于游戏内容身份，不随引擎包自动升级；修改它们时需配套验证存档与语言包。

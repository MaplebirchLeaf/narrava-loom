# 公开 API 与依赖锁定

## 三种“公开”不是一回事

| 边界 | 面向谁 | 稳定性依据 |
|---|---|---|
| Twee、Expression、Macro | 游戏作者 | `docs/reference/` |
| `narrava.d.ts` 详细全局签名与说明 | TS/JS 游戏脚本 | `bindings/typescript/narrava.d.ts`（人工维护、由契约覆盖测试校验） |
| Script/Runtime 名称与 tagged union | Binding、Host、跨语言调用方 | `bindings/script-contract.json` 生成 `narrava-contract.generated.d.ts` 与 Rust 常量 |
| Rust `pub` 项 | Host 和工具作者 | Rustdoc 与语义化版本 |

Rust 中的 `pub` 只表示当前 crate 外可访问，不代表游戏作者需要 Rust，也不自动
代表 `0.3.x` 期间已承诺长期兼容。公开 Rust 项应有文档注释；编译器内部结构不应为了
方便调用而无条件扩大。缩小已有 `pub` 是破坏性变更，需要单独评审，不在文档整理中暗改。

## 异步边界

Core 已有 Pending/Resume/Cancel 的所有权模型。Tauri ECMAScript Binding 会立即排空 Boa
microtask；`Host.delay(ms)` 则建立真实 Core suspension，由 Rust Worker 到期后恢复原事务。
没有等待受管 Host 操作的未决 Promise 返回
`script.macro_unmanaged_promise`，不得伪装成 `undefined` 或普通 JSON 值。

当前只有 delay capability。文件选择、网络等能力需要各自的权限、输入输出与取消契约，不能
借一个“任意 Host 回调”绕过边界验证。

## 依赖锁定

- library 在 `Cargo.toml` 使用可兼容的 semver 范围，除非已知上游版本不兼容。
- 仓库提交根 `Cargo.lock`，Core CLI 和 Tauri Host 的可重现检查使用 `--locked`。
- 独立的 ModLoader 有自己的 lockfile，不属于根 workspace 验证。
- 更新 lockfile 必须是有意的依赖更新，不应成为普通构建的副作用。

仓库标准检查见[仓库命令](commands.md)。

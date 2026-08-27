# Narrava ModLoader

> 状态：仅建立独立 crate 边界；尚无可用的模组加载功能
>
> 更新日期：2026-08-27

## 当前真实实现

`crates/narrava-loom-modloader` 当前只有 crate 声明和对 `narrava-loom-protocol` 与
`narrava-loom-core` 的单向依赖（顺序固定为 `modloader → protocol → core`），
源码尚未定义 `ModLoader`、`.nmod` 清单、选择顺序、Patch、有效构建或 ZIP 导入 API。它被排除在
根 workspace 之外，因此根目录的 Rust CI 也不会验证该附属项目。

这一空壳只记录两条已经确定的边界：

- ModLoader 不回到 Core 内部，也不让 Core 依赖模组类型；
- ModLoader 可以依赖 Core 的公开数据、验证能力与 `narrava-loom-protocol` 的传输类型，
  但不能依赖 Tauri、TUI 或具体 Renderer。

I18n 的 `.nlang` 校验、导入和运行时语言链已经由 Core 实现；那部分不是 ModLoader 已完成的证据。

## 尚未实现

以下内容都是后续设计，不是当前 API：

- `.nmod` 文件结构与清单 schema；
- 模组身份、版本兼容、依赖和显式排序；
- I18n 之后的 Story／Script／Resource Patch；
- 候选构建的完整验证与原子切换；
- Binding 侧 ZIP 读取、持久化选择和游戏内管理界面。

后续只有在一条真实纵向用例能够同时覆盖包输入、Patch、重新编译、Resource 组合和失败回滚时，
才应把这些概念写成“当前实现”。在此之前，Host 文档和发布说明只能把 ModLoader 标记为未完成。

## 预留依赖方向

```text
narrava-loom-modloader
        ↓
narrava-loom-protocol
        ↓
narrava-loom-core public API

Tauri / TUI / Renderer 不进入 ModLoader
```

具体格式与执行顺序应在实现时由测试和公开类型共同确定；本页不提前承诺尚不存在的协议。

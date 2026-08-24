# Narrava Diagnostic 与 Logger

> 状态：Diagnostic 与 Logger 基础结构已建立
>
> 更新日期：2026-08-20

## 边界

Diagnostic 描述一个结构化问题，Logger 记录运行事件。两者互相配合，但不互相拥有：

```text
Compiler / Runtime → Diagnostic → 调用方或 Logger
普通运行事件 ─────────────────→ Logger
```

Diagnostic 不负责缓存、输出或订阅；Logger 不替代各模块的错误返回值。

Logger 同时服务引擎与游戏内容。引擎、游戏 scripts 和获准的模组都可以通过同一公开接口写入结构化日志；Logger 汇总记录并提供查询、过滤、订阅和清理，但不决定显示界面。游戏作者可以把筛选后的记录放入自制调试菜单、Tauri 窗口、开发者控制台或完全不显示。

## 当前实现

- `DiagnosticSeverity` 包含 `Error`、`Warning`、`Note`；
- `Diagnostic` 保留稳定代码、严重级别、消息和可选位置；
- `DiagnosticLocation` 保留省略 `contents/` 的相对 Source、UTF-8 字节范围及 1-based 行列；
- `DiagnosticLocator` 将嵌入片段的局部 UTF-8 Span 映射回完整 Source，并按 Unicode 字符计算列号；
- `LogLevel` 包含 `Trace`、`Debug`、`Info`、`Warn`、`Error`；
- `LogEvent` 保留级别、目标模块、消息和可选 Diagnostic；
- `LogEvent` 是待写入事件，`LogRecord` 是 Logger 生成的已记录事件；
- 每条 `LogRecord` 具有从 1 开始单调递增的 `LogSequence`，历史与订阅共享同一记录身份；
- 内存 `Logger` 提供 `log`、`get`、`clear`，并保持记录写入顺序；
- `LogFilter` 支持最低级别和精确目标筛选，`query` 保持原始写入顺序；
- `subscribe` 返回稳定订阅 ID，只收集订阅后产生且符合过滤条件的事件；
- `take` 取走订阅的待处理事件，`unsubscribe` 释放订阅及其队列；
- `clear` 同时清空历史和待处理事件，但保留订阅关系且不重置记录序号；
- 核心 Logger 不保存 Rust 或 JS 回调，WASM Bridge 后续把订阅队列转成脚本通知；
- Twee ParseError 已将全部结构错误转换为稳定代码、原消息和公共源码位置；
- Twee SemanticError 已将重复 Passage 转换为稳定 Diagnostic，`StoryError` 可统一委托转换；
- Expression LexError、ParseError 与 EvalError 已提供稳定 Diagnostic；显式 Macro 参数通过统一 Locator 将局部 Span 映射到 Source 位置；
- Macro Definitions 与 Local Scope 错误已能转换为稳定 Diagnostic，但不会自动写入 Logger；
- 当前不记录时间戳；Tauri 已提供 Host 诊断页，但 Core `Logger` 全量订阅桥仍未接入该面板。

## 平台扩展顺序

1. 确定具体 Host 是否需要独立于顺序号的 Runtime 时间；
2. 按需把 Core Logger 订阅安全映射到 Host Debug API；
3. 由游戏作者或平台层决定是否把脚本日志显示到浏览器控制台或游戏内面板。

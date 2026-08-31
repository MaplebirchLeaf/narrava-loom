# Diagnostic 与 Logger

Diagnostic 描述结构化问题，Logger 记录运行事件：

```text
Compiler / Runtime → Diagnostic → 调用方或 Logger
普通运行事件 ──────────────→ Logger
```

Diagnostic 不缓存、输出或订阅问题；Logger 不代替错误返回。界面可查询和展示日志，
但 Core 不决定呈现方式。

## Diagnostic

- `DiagnosticSeverity`：`Error`、`Warning`、`Note`。
- `Diagnostic`：稳定代码、严重级别、消息和可选位置。
- `DiagnosticLocation`：省略 `contents/` 的 Source 路径、UTF-8 字节范围与从 1 开始的行列。
- `DiagnosticLocator`：将嵌入片段的局部 Span 映射回完整 Source，列号按 Unicode 字符计算。

Twee、Expression、Macro 和 Story 的解析或运行错误转换为稳定 Diagnostic。转换不得
丢失原始代码、消息或 Source 位置。

## Logger

- `LogLevel`：`Trace`、`Debug`、`Info`、`Warn`、`Error`。
- `LogEvent` 是待写入事件；`LogRecord` 是 Logger 生成的记录。
- `LogSequence` 从 1 开始单调递增，历史与订阅共享记录身份。
- `log`、`get`、`query` 和 `clear` 管理内存历史；`LogFilter` 按最低级别和精确目标筛选。
- `subscribe` 返回稳定 ID，`take` 取走待处理记录，`unsubscribe` 释放订阅。
- `clear` 清空历史和待处理记录，保留订阅且不重置序号。

Core Logger 不保存 Rust 或 JavaScript 回调。Host 通过拥有型查询或订阅结果展示记录。

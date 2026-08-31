# Macro

Macro 负责 Twee 中的叙事动作、控制流和作者扩展。Twee Parser 保留语法事实，HIR/MIR
保留可执行语义，Runtime 所有 Definition、局部域、事务和 suspension。可用 Macro 及参数
见 [API 与语法速查](../reference/api-and-syntax.md)。

## 定义与语法形状

`MacroDefinition` 由 `name`、`syntax_kind`、`handler` 和 hooks 组成。名称使用小写 ASCII 字母
开头，只允许小写字母、数字、`_` 和 `-`。Runtime 拒绝重复注册和不合法名称。

`Inline` 不接收正文：

```twee
<<set $score = 1>>
```

`Container` 接收正文并需要闭合标签：

```twee
<<if $score > 0>>...<</if>>
```

AST 和 HIR 显式保留形状，因此 Inline 与空正文 Container 不会混淆。结构化子句
如 `elseif`/`else` 和 `case`/`default` 由 Parser 归入所属容器，不作为普通
Definition 调用。

## 参数

Macro 参数按源码顺序解析为 Expression 或 Interaction Target。Expression 的局部字节偏移
用于将 Diagnostic 映射回 Macro Header。参数之间用空白分隔，不接受顶层逗号。

```twee
<<link [[前往大厅|Hall]]>>
```

Interaction Target 只保留 `label` 和 `target`。Parser 校验外壳和非空两端；Runtime
负责求值和生成 Navigation 语义。

## 局部域

`MacroLocalScopes` 持有调用帧：

- `@args`：当前调用的有序参数；
- `@name`：当前帧的局部值；
- 父帧：Widget 或嵌套调用的外层域。

读取从内向外查找，写入只修改当前帧。正常完成、`exit`、取消和错误都必须退出
当前帧，不向下一次调用泄漏局部状态。Widget 调用创建独立帧，并与调用者共享
受 Runtime 管理的 State 与事务。

## Handler 与 hooks

Handler 分为：

- Native：Rust 实现，返回 Value、Surface 输出、控制信号或 suspension；
- Widget：执行已编译的 HIR 正文；
- Script：通过 `narrava-loom-script` 执行作者函数。

hooks 按注册顺序执行：

```text
before hooks → Handler → after hooks
```

`before` 可修改本次调用的参数；Handler 接收修改后的同一组值。`after` 只能处理
本次调用的隔离输出，不得跨越到后续 Twee 节点或其他 Macro 输出。任一阶段
失败都终止链并交给 Engine 回滚。

## 输出与控制信号

Macro 输出是 Host-neutral Surface 节点，不是 HTML 或终端字符串。`print`、`link`、`button`、
`slot` 和 `replace` 产生跨 Host 语义；方框、card、动画与焦点属于 Renderer。

Runtime 内部控制信号包含：

- `Continue`：继续当前执行单元；
- `Break` / `ContinueLoop`：由最近循环消费；
- `StopPassage`：终止当前 Passage，用于 `goto` 等导航；
- `Exit`：终止当前包含或 Widget 正文。

信号只能由对应边界消费。信号泄漏到 Engine 提交点视为运行错误。

`run` 执行 Expression 并丢弃结果；`include` 在当前 Surface 中执行另一 Passage，
不改变 Story 历史；`goto` 请求导航并停止当前 Passage 的剩余指令。三者使用
独立 HIR/MIR 节点，不用动态 Definition 冒充控制流。

## 异步所有权

Native 或 Script Handler 可返回 `Pending`。Runtime 将本次调用的 Definition、参数、局部
链、after hooks 和 VM 位置封装进 continuation。Host 只获得不透明 operation ID 和类型化
输入/输出契约。

恢复时有三种结果：

- Complete：执行剩余 after hooks，退出当前帧并继续 VM；
- Pending：保留同一 VM 位置和局部链，替换调度句柄；
- Error/Cancel：释放当前帧，由 Engine 恢复 State/Story 检查点。

Runtime 在 continuation 中验证执行链身份和 VM 位置。Host 不得伪造 frame、检查点或平台
句柄。完整驱动边界见 [Runtime Session](runtime-session.md)。

## 事务与预算

Macro 执行与当前 Engine 事务共享 State、Story 和 Surface 检查点。成功时一次提交；
Evaluator 错误、未解析 Definition、无效控制信号、取消、include 或指令预算耗尽时全部
回滚。预算限制作用于整条嵌套执行链，不在 include 或异步恢复时重置。

## 保留语法

`return` 只由 Parser/HIR 保留，Runtime 尚未定义可返回值调用域。它不属于当前可执行
Macro API，也不能被动态 Definition 覆盖。

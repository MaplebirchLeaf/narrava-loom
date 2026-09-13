# 编译与表达式

Twee 与 TypeScript/JavaScript 使用独立管线。叙事程序降低到 Bytecode，脚本保留在
ECMAScript Runtime，通过受控调用连接两者。

```text
contents/**/*.twee → AST → HIR → MIR → LIR → Bytecode → VM
contents/**/*.ts   → Oxc 去除类型 ┐
contents/**/*.js   ──────────────┴→ ScriptBundle → Boa
```

## Source 与语法结构

`SourceList` 递归发现内容，按平台无关的相对路径排序。`SourcePath` 省略 `contents/` 前缀，
只使用 `/`，拒绝绝对路径、父目录跳转与反斜杠。CSS 由 Host 处理。

Twee Parser 保存 Passage 名、Tag、Macro 形态、原始参数和源码 Span，汇总时拒绝跨文件重名。
特殊 Passage 的名称和无 Tag 约束在编译期校验。`/% ... %/` 注释在降低前移除。

正文默认是字面 Text，`$name` 与 `${expression}` 不自动求值；动态输出由 `print` 显式建立。
源码换行折叠为空白，`<br>` 降低为 HardBreak，其他 HTML 标签没有平台语义。
结构容器、子句和闭合关系由 Parser 保存，不用 Runtime 字符串扫描还原。

## IR 各层的责任

| 阶段 | 保留或建立的内容 |
| --- | --- |
| AST | Passage、Text、Macro 源码形态与位置 |
| HIR | 分支、循环、赋值、导航、Widget 等叙事结构及 Expression AST |
| MIR | 显式跳转、临时值槽、迭代槽、Macro 调用和 I18n 文本片段 |
| LIR | Passage 索引与经过验证的跳转地址 |
| Bytecode | 拥有型操作码、入口表、字符串、Expression、Macro 与 I18n 常量 |

`set` 与 `run` 都降低为求值并丢弃结果；`unset` 保持独立删除指令。
`switch` 主值只求值一次、分支不贯穿；循环目标栈限定 break/continue 的最近循环边界。
`include` 压入执行帧，`goto` 结束当前 Passage 并等待 Engine 确认，两者不会合并成同一指令。

`silently` 的输出抑制属性保留在输出和 include 调用点，避免控制跳转绕过开关恢复。
通用 Macro 调用保留名称与形态，执行时查找 Definition；编译器不把可替换的脚本实现静态绑定。
`return` 只保留在 Parser/HIR，当前执行层拒绝该节点。

## Bytecode 与发布

VM 只接受 `BytecodeProgram` / `BytecodeMacroBody`。编码使用 `NRVA` 魔数与独立格式版本，
解码验证头、目录和指令边界；不保留构建期借用。编译后的叙事程序可独立序列化。

`.nar` 使用 `NAR1` 魔数和确定性 ZIP 负载，携带配置、Source 记录、拥有型 Bytecode、
资源索引与内容哈希。Host 校验后直接运行 Bytecode，从 Source 记录建立 ScriptBundle。
格式版本独立于项目版本；不兼容的编译产物需重新编译。

I18n ID 由 Passage 名与结构路径构成，行号只作定位。连续可见 Text/Print 形成消息，
VM 按源码顺序先求值 placeholder，再按已验证译文顺序生成 Text，详见[I18n](i18n.md)。

## Expression 求值

Expression 使用自己的 Lexer、Parser、AST 与 Evaluator，不委托给 JavaScript 求值。
Token/AST 保存 UTF-8 字节 Span，嵌入 Macro 时通过 DiagnosticLocator 映射回源码。
可用操作与限制集中在[Expression 参考](../reference/expressions.md)。

Value 使用 f64 Number 和 UTF-16 String，区分 null 与 undefined；保留 NaN、无穷和有符号零。
Array/Object 持有引用身份，普通克隆保留别名，显式 `clone(value)` 深复制值图并保留内部共享和循环。
Callable 只携带受控身份，真实脚本函数由 Binding registry 持有，不能进入存档值图。

Evaluator 不拥有 State。`EvaluationContext` 提供读取、调用、随机和写入授权，State 将随机请求
交给注入的 Engine。可写路径先解析根与索引，再求右值并提交；索引只求值一次。
共享集合修改也必须获得写入授权，错误不得留下部分根替换。

成员与原型只解析登记的能力，不暴露 `__proto__`、`constructor`、`prototype` 或宿主对象。
可选链只处理 nullish 短路，不吞掉真实访问错误；未知全局不回退到浏览器环境。

## 源码位置

- [Source](../../src/source.rs)、[Twee](../../src/twee)、[HIR](../../src/hir)、[MIR](../../src/mir)。
- [LIR](../../src/lir.rs)、[Bytecode](../../src/bytecode.rs)、[VM](../../src/vm)。
- [Expression](../../src/expression)、[ScriptBundle](../../src/script)。

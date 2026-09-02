# Narrava Runtime

Runtime 组合 Engine、State、Story、Macro、Script、Event、Reaction、I18n、Resource 和 Save。
Core 保留游戏语义与事务真相；Host 只处理平台 IO 和呈现。Surface 边界见
[Host Surface](protocol.md)，对外驱动协议见 [Runtime Session](runtime-session.md)。

## 所有权

| 领域 | 职责 |
|---|---|
| Engine | 生命周期、连续导航、跨领域事务与回滚 |
| State | `global`、`setup`、`variables`、`temporary` 四个命名空间 |
| Story | Passage 索引、当前位置、history 和导航请求 |
| Macro | Definition、Widget、调用帧、`@args` 与 `@` 局部值 |
| Script | ECMAScript 函数、Promise 和 Core API 适配 |
| Event / Reaction | 结构化事实、订阅与声明式触发规则 |
| I18n | 稳定文本身份、译文校验、字典与 fallback |
| Resource | 逻辑路径、媒体类型、字节与完整性 |
| Save | 可持久领域状态、版本与游戏兼容校验 |

Renderer、输入设备、窗口、文件选择器和平台对象不属于 Core。Binding 不得保存第二份
State、Story、Macro 或 Reaction 真相。

## Engine 事务

Engine 在执行前捕获 State、Story 和 Reaction 检查点。成功时一次提交变量、导航、
触发状态和 Surface；Runtime 错误、无效请求、取消、Script 同步失败或预算耗尽会
恢复整个检查点。

`goto` 先建立已验证但未提交的 Story 请求。只有当前 Passage 以 `StopPassage`
结束时 Engine 才确认目标并继续导航链。`include` 在源码位置压入 VM frame，不创建
history。未消费的 include/goto 请求不得静默提交。

`EngineExecutionLimits` 限制单条执行链的 Passage 与 include 数量。预算沿同一 continuation
保留，不在 Widget、include 或异步恢复时暗中重置。

## 启动、新游戏与生命周期

启动顺序为：

```text
注册 Widget → StoryInit → Start.Init → Start.Start → Reaction → 正文 → Render → Display
```

`StoryInit` 只执行逻辑初始化，不创建 history 或可见输出。`new_game` 先结束旧 Passage，
再重置 State/Story/Reaction，重新执行 StoryInit 与 Start；任一步失败都恢复调用前状态。

普通 Passage 生命周期为：

```text
Init → Start → Reaction → Body → Render → Display → End
```

入口 `params` 只属于本次导航，不自动写入 State。include 与特殊 Passage 不创建独立
Reaction lifecycle。`[exit]` Passage 执行逻辑但跳过 Render/Display，也不作为 SafeReturn
目标。

## State 与 Story

| Twee | State 命名空间 | 生命周期 |
|---|---|---|
| 普通名称 | `global` | Host 或 scripts 显式登记 |
| `setup` | `setup` | 启动配置与共享值 |
| `$name` | `variables` | 游戏进度，进入 Save/history |
| `_name` | `temporary` | 当前导航过程 |

`@name` 与 `@args` 属于 Macro 帧，不属于 State。`StateCheckpoint` 用于短期 Engine
事务；Save 使用独立的可持久快照。

Story 的 Passage 名和 Tag 区分大小写。`request_goto()` 只验证目标，`confirm_navigation()`
才修改 history。`back`/`forward` 移动游标并恢复对应 `$variables` 快照；从旧位置导航
会截断原前进分支。history ID 不复用。

## Macro、Script 与 Expression

Macro 持有 Definition、Widget 正文和调用帧。嵌套 Widget 使用独立 `@args` 和局部域；
`exit`、循环控制和 `goto` 只由各自最近的语义边界消费。普通字符串不会自动二次
解析为 Twee；动态 Fragment 必须经过显式 Parser 入口。完整契约见 [Macro](macro.md) 与
[Expression](expression.md)。

`.ts/.js` 形成有序 `ScriptBundle`，由 `narrava-loom-script` 使用 Boa 执行，Oxc 移除
TypeScript 类型语法。脚本必须经 State API 显式导出全局值；ECMAScript `import/export`
不会自动进入 Twee。

`ScriptCallable` 只保留身份和调试名，真实函数由 Binding Registry 持有，不进入 IR、
Save 或可序列化 Value 图。普通 Script 函数是同步 Expression 能力；Promise Macro 必须通过
Pending/Resume/Cancel 链运行，不得伪装成普通值。作者契约在
`bindings/typescript/narrava.d.ts` 维护。

## Surface

Runtime 以 `BodyExecution` 同时返回控制信号和有序 Surface。节点包含 Text、HardBreak、
StyledText、Image、Region、Container、Replace、Component、Input、Navigation 与
SafeReturn。

Twee 普通正文整体是字面文本；`$name` 和 `${expression}` 不自动求值。`print` 显式
生成动态文本。include 和 Widget 在源码位置执行；Reaction 的输出按触发安全点追加。
`silently` 保留状态副作用和控制信号，丢弃本块 Surface。

Navigation 与 SafeReturn 携带 `InteractionId`。Host 只能激活上一份 Surface 中存在的
动作，不能自行构造 Passage 目标。当普通 Passage 没有作者导航时，Engine 可指向最近
安全 history 项追加 SafeReturn；文字与控件形态由 Host 决定。

## Event、Reaction、I18n、Resource 与 Save

Event 先以单调序号记录结构化事实，再投递给当时存在的匹配订阅。`take` 消费待处理
队列；`clear` 清空历史和队列但不重置序号。ScriptCallable 等平台函数不得作为
事件载荷。Reaction 在同一 Engine 事务内消费 Event、State 变化或 lifecycle 事实。

I18n 选择是 Runtime 上下文，不写入 State。目标语言与 fallback 链随 Engine progress 跨暂停和导航
保留；语言切换后 Host 刷新所有依赖语言的可见区域。

Resource 逻辑路径使用 `/`，拒绝绝对路径、空段、`.`、`..`、反斜杠和重复路径。
Core 可延迟读取并缓存成功结果；URL、Blob、解码与 DOM 对象属于 Host。

Save 捕获当前及历史 `$variables`、Story 时间线、Reaction 状态和游戏身份，编码为版本化
二进制文档并原子恢复。格式与校验见 [Save](save.md)，翻译契约见 [I18n](i18n.md)。
模组组合尚未实现，不属于当前 Runtime API。

## VM 与 continuation

VM 只接收可序列化的拥有型 Bytecode，并在 `Halt`、`NavigationPending` 或
`MacroPending` 停下。State、Story、Macro Definitions、资源缓存和 Renderer 状态由各自领域
持有。

`HostApi::drive_stable()` 驱动 Macro、导航和 Halted 提交，只在得到可呈现的
`HostUpdate` 或异步 operation 时返回。`resume_and_drive()` 将 Handler 恢复纳入同一
事务。Host 令牌只携带执行身份；VM frame、State/Story/Reaction 检查点、局部域、
待确认请求和平台句柄由 continuation 所有。

恢复链验证执行身份和指令位置，然后继续同一帧。迭代器、include 栈、语言选择和
输出抑制随 frame 或 Engine progress 保留，不在异步恢复时重置。执行失败会返还完整
所有权供 Engine 回滚，不丢弃 frame、检查点或 suspension handle。

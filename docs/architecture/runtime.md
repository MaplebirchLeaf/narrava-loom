# 运行时与事务

`RuntimeSession` 是一局游戏的所有权根。Tauri Worker 和 TUI 直接调用 `execute`，
传入拥有型 RuntimeCommand，取得 RuntimeUpdate 或 PendingOperation。

```text
Host command → RuntimeSession → Core HostApi → Engine → VM / Macro / Script
                      ↓
             SemanticOutput → Protocol DTO → Host Renderer
```

## 所有权

| 对象 | 拥有的内容 |
| --- | --- |
| Engine | 根种子、随机进度、生命周期、导航执行链与 Core 事务 |
| State | global/setup/variables/temporary、地点定义与位置、注入的 Engine 句柄 |
| Story | Passage 索引、历史、当前游标与待确认导航 |
| RuntimeSession | State/Story、Reaction 运行状态、交互表、上一帧、continuation 与命令事务 |
| EcmaBinding | Boa realm、函数与 Promise、脚本桥接、有界日志 |
| RuntimeData | 游戏身份、I18n 目录与已验证语言包 |

`@` 局部域属于 Macro 调用帧。I18n 选择属于运行上下文，不写入 State。
官方 Host 没有独立 Native Session registry；Session ID 和 envelope 是 Protocol 数据，
不代表另一个 Handle/Driver 对象。Core 通过 ScriptCallDispatcher 回调脚本，避免依赖 Script crate。

## 启动与导航

Host 在脚本装载前按配置或熵建立 Engine，并通过 `State::with_engine` 注入。
脚本登记地点和领域定义后关闭地点注册，Session 保存启动基线。

```text
注册 Widget → StoryInit → Start.Init → Start.Start → Reaction → Body → Render → Display
普通 Passage：Init → Start → Reaction → Body → Render → Display → End
```

StoryInit 只做逻辑初始化，不建立历史或可见输出。Core `new_game` 重置游戏状态和原根种子序列；
Session restart 先恢复本次脚本启动基线，再重新开始。任何一步失败恢复调用前的事务。

`request_goto` 先验证目标，Engine 在 StopPassage 后确认并延续导航链。
`include` 在原位置压入 VM frame，不建立独立历史或生命周期。
入口 params 只属于本次导航；exit-tag Passage 跳过 Render/Display，不成为 SafeReturn 目标。
未消费的导航请求和泄漏的控制信号不能静默提交。

## 命令事务

```text
校验 command → checkpoint → execute → pending / commit / rollback
```

RuntimeTransaction 捕获 State、Story、Reaction 次数、交互表、上一帧、变化比较基线及未完成输出。
Pending 和后续 Reaction 导航延续同一事务，成功结算后释放；错误或取消恢复整份检查点。
无效命令或错误 operation ID 在执行前拒绝，不破坏当前挂起操作。

Core 检查点覆盖单条 Engine 执行链；Session 事务还覆盖链完成后的 Reaction、公共区域和呈现。
两者生命周期不同，不能用一个短期检查点替代命令事务。

| 快照 | 范围与用途 |
| --- | --- |
| StateCheckpoint | 全部命名空间、地点定义/位置、Engine 正文与临时重绘进度；短期回滚 |
| StateSnapshot | 持久变量、LocationState、EngineSnapshot；历史和存档 |
| Story 快照 | 时间线及游标；恢复不回退历史 ID 分配高水位 |

State 的脚本代理直接访问活动 Rust State，不维护 JS 镜像。`fork_view` 复制执行快照并建立
独立 Engine 游标，使公共区域执行不消耗正文状态。地点定义共享，位置和变量按快照隔离。

## 随机、历史与重绘

Engine 拥有唯一 SplitMix64 序列，Twee 与 Math.random 使用同一取样入口。
EngineSnapshot 用两个 u64 保留根种子和序列状态；脚本种子查询使用字符串避免精度丢失。
事务和存档附带 EngineSnapshot，不在 State 另建种子控制接口。

历史 back/forward 恢复目标页的入页状态再执行正文。从旧位置发起新导航会截断前进分支。
`RefreshCurrent` 用于语言切换和 Import 重绘：按入页状态重建呈现与交互，保留操作后已提交的
State 与随机进度。随机使用临时序列，Location 只读视图跨 Pending 保留。
新导航开始时解除刷新视图，包括同名导航。

重绘仍会执行作者脚本，不是任意脚本的纯渲染器；JS realm 声明、注册和日志不承诺随 State 回滚。
输入呈现从权威 State 同步，普通文本不会因任意赋值自动重绘。

## 挂起与恢复

continuation 保留 VM frame、Macro 局部域、调用链和脚本挂起凭据；pending 表示唯一的 Host 等待。
Host 只接收 operation ID 与类型化请求，完成后发送 Resume 或 Cancel，不接触执行栈。

Tauri facade 用异步 timer 和 blocking pool 处理等待及存档，Runtime Worker 可继续接收控制命令；
TUI 命令循环完成相同平台操作。恢复继续原帧、include 栈、迭代状态、语言和执行预算。
预算沿整条链保留，不因异步恢复而重置。脚本 Promise 只能立即结算或等待受管 Host 操作。

输入触发的存档请求保留原命令事务，跨 Reaction 与 IO 完成后才提交。
编码、导入、IO 或 Save.after 失败时从相应恢复点结算，不能提前丢失输入前检查点。
持久化步骤见[Save 格式](save-format.md)。

## Event 与 Reaction

Event 以单调序号记录结构化事实，再投递给已存在的匹配订阅。订阅通过 take 拉取，
不持有作者回调；事件句柄和队列不进入存档。
Reaction 在命令安全点处理 Event 和持久 State 变化，lifecycle 规则在 Start 与正文之间执行。
候选解析、派生事件、内容和导航属于原事务，失败回滚次数与输出；细节见[事件与 Reaction](../author/events.md)。

## 诊断与调试

Diagnostic 保存稳定代码、严重级别、消息与可选源码位置，不替代错误返回。
DiagnosticLocator 将局部 UTF-8 Span 映射到省略 contents 前缀的相对路径和从 1 开始的行列，
列号按 Unicode 字符计算。Logger 记录历史与订阅队列，默认各有 1024 条容量，清空不重置序号。
脚本、Runtime 失败和 Host 提示写入同一 Logger，宿主预览不消费作者订阅。

`debug_snapshot` 在无 Pending 时返回有界只读结果，不求值作者表达式。
`DebugScript` 复用输入事务与安全点，控制台 Promise 通过独立队列和 Pending::Console 恢复或取消。
成员补全只读属性路径，不执行表达式、getter 或未知 Proxy trap；帮助来自作者类型声明。
Host 的 developer 开关同时控制界面与 Rust 入口。操作和显示限制见[调试指南](../author/debugging.md)。

## 源码位置

[Engine](../../src/engine)、[State](../../src/state.rs)、[Story](../../src/story)、
[RuntimeSession](../../crates/narrava-loom-script/src/session)、
[Refresh](../../crates/narrava-loom-script/src/refresh.rs)。

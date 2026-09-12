# Save 格式与恢复事务

本文面向 Core 与 Host 开发者；作者用法见 [Save](../author/save.md)。

## 边界

Save 记录能够恢复游戏进度的持久领域数据，不保存宿主或当前执行栈：

| 保存                                     | 不保存                                   |
| ---------------------------------------- | ---------------------------------------- |
| `State.variables`（`$name`）             | `State.global`                           |
| 当前地点 ID、坐标与环境                  | World 地点定义、多边形、父子索引         |
| Story 完整导航时间线                     | `State.setup`                            |
| Story 当前游标                           | `State.temporary`（`_name`）             |
| 各历史项进入前的 `$variables` 与世界位置 | `State.global`／`State.setup` 的历史版本 |
| 每次 Passage 是否产生作者导航            | Macro `@locals`、`@args`                 |
| Reaction 启用、次数与销毁状态            | Reaction Definition 与 `cond` 函数       |
| 精确游戏 ID 与版本                       | Function、Macro Handler、Promise         |
| Narrava Array/Object 引用图              | VM frame、Pending、Host/Renderer 对象    |

`global` 与 `setup` 属于启动环境，由配置、StoryInit 和 scripts 重新建立。`temporary` 与 Macro Local 只服务当前执行范围，加载后清空。

## 二进制文档结构

`.nsave` 是正式二进制协议，不是 Rust 内存布局、JSON 或 ZIP。文件以 `NRSAVE\0` magic 和单字节
schema version 开始，当前写入版本为 `3`；payload 使用 postcard 编码稳定字段、Value 图节点 ID
和长度前缀数据。v3 为当前状态与每个历史项增加世界位置；读取 v2 时将两处位置补为未定位，
不根据当前 Passage 猜测旧位置。未知 magic/version 在解析 payload 前拒绝。

Core 通过 `SaveDocument::to_bytes()`／`from_bytes()` 编解码，Host 只读写 `Vec<u8>`。

游戏身份仍要求精确 `id + version`；v2 格式兼容不放宽游戏版本匹配。通用游戏存档迁移尚未实现。

## Value 图

Array 与 Object 不递归嵌入 payload，而是使用单调节点 ID 建立图：

- 多个 `$variables` 指向同一集合时，恢复后仍共享同一身份；
- Array/Object 循环引用不会无限递归；
- String 保存 UTF-16 码元，保留孤立代理项；
- Number 保存原始 `f64` 位，保留 `NaN`、正负无穷和 `-0`；
- `undefined`、`null`、Boolean、Array、Object 与 String 可保存；
- NativeCallable、ScriptCallable 与 NativeNamespace 会返回 `save.unsupported_value`。

解码会拒绝 ID 0、悬空引用和节点类型不一致。Host 仍应在把外部文件交给 Core 前限制总文件大小；Core 的结构校验不能代替平台 I/O 配额。

## Story 与恢复事务

时间线按顺序保存 PassageName、导航标记、当前游标，以及每项进入前的 `$variables` 图与世界
位置；不保存进程内 `StoryHistoryId`。加载时使用当前 HIR 重建时间线，PassageName 区分大小写，
`StoryInit` 不得进入历史。Back/Forward 恢复目标项的进入前状态，再重放 Passage。

恢复顺序固定为：

1. 校验精确游戏身份；
2. 校验 Story history 与当前 HIR；
3. 完整解码当前及逐历史项的 Value 图，并以本次启动定义校验所有地点 ID、坐标与环境；
4. 在临时所有权中建立新 `$variables`、世界位置、Story 时间线与历史快照；
5. 校验全部通过后一次性提交 State 与 Story，并清空 `_temporary`；
6. RuntimeSession 根据当前启动脚本已注册的 ID 恢复 Reaction 状态；
7. RuntimeSession 使用 Resume 命令事务恢复 State/Story/Reaction；Import 或 Save.after 失败时统一回滚，脚本直接读取活动 Rust State。

捕获直接借用活动 `$variables` 和已经隔离的历史快照进行 ValueGraph 编码。Story history 在运行期
保存 Passage 引用及进入前的持久状态，只有可移植存档边界写入 PassageName；因此 Save 大小取决于
实际 history 与其持久状态，而不是 Story 总 Passage 数。当前或任一历史项的未知地点、越界坐标
等错误都以 `save.invalid_world` 原子拒绝，不留下部分恢复的状态。

Core `restore()` 只恢复稳定领域状态；官方 RuntimeSession 随后通过 `RefreshCurrent` 重绘当前
Passage。刷新所有权见 [Runtime Session](runtime-session.md#pending-与-host)，
作者可见的位置行为见 [World](../author/world.md#刷新历史与存档)。

## 请求与平台 IO

Core 的 `SaveController` 提供有序请求与 `before/after` 订阅，供 Rust 调用方使用，
本身不访问文件系统。脚本侧的 Hook 与待处理请求由 `bootstrap/save.ts` 持有，经
`EcmaBinding::take_save()` 交给 `RuntimeSession`，不共享 Rust Controller 队列。

Script Export/Import 的执行链为：

1. Bootstrap 顺序运行 before Hook，并记录最终 target；
2. Session 取走请求，为 Export 编码存档，或为 Import 请求字节；
3. Host 消费 PendingOperation，完成文件 IO 后发送 Resume；
4. Session 完成恢复或导出结算，再通过 `complete_save()` 通知 after Hook。

Hook 身份和队列只在当前进程有效，不进入存档。after 不能修改已导出的文档；
失败与取消的命令事务见 [Runtime Session](runtime-session.md)。脚本用法及临时
`Save.capture/restore` 与正式存档的区别见[作者 Save 指南](../author/save.md)。

Tauri 与 TUI 使用命名槽位 `save/<target>.nsave`。成功进入另一 Passage 后写入 `autosave`；
语言刷新、历史回溯和侧栏切换不触发自动保存。文件选择、云同步与存档迁移尚未实现。

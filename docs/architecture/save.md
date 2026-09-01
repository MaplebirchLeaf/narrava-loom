# Narrava Save

> 状态：Core 文档、Host 请求与生命周期边界已实现
>
> 更新日期：2026-09-01

## 边界

Save 记录能够恢复游戏进度的持久领域数据，不保存宿主或当前执行栈：

| 保存 | 不保存 |
| --- | --- |
| `State.variables`（`$name`） | `State.global` |
| Story 完整导航时间线 | `State.setup` |
| Story 当前游标 | `State.temporary`（`_name`） |
| 各历史项进入前的 `$variables` | `State.global`／`State.setup` 的历史版本 |
| 每次 Passage 是否产生作者导航 | Macro `@locals`、`@args` |
| Reaction 启用、次数与销毁状态 | Reaction Definition 与 `cond` 函数 |
| 精确游戏 ID 与版本 | Function、Macro Handler、Promise |
| Narrava Array/Object 引用图 | VM frame、Pending、Host/Renderer 对象 |

`global` 与 `setup` 属于启动环境，由配置、StoryInit 和 scripts 重新建立。`temporary` 与 Macro Local 只服务当前执行范围，加载后清空。

## 二进制文档结构

`.nsave` 是正式二进制协议，不是 Rust 内存布局、JSON 或 ZIP。文件以 `NRSAVE\0` magic 和单字节
schema version 开始，当前版本为 `2`；payload 使用确定性的 tagged value、varint 长度、节点 ID 和
长度前缀数据。未知 magic/version 在解析 payload 前直接拒绝，不提供未发布 JSON 草案的兼容分支。

Core 通过 `SaveDocument::to_bytes()`／`from_bytes()` 编解码，Host 只读写 `Vec<u8>`。导出不再建立
pretty JSON `String`，导入也不再同时保留文件 bytes、UTF-8 String 与解析文档。

游戏身份使用精确 `id + version`。当前不提供隐式迁移，不允许另一游戏或另一版本直接恢复。未来迁移必须是独立、显式的输入转换，不进入正常 `restore()`。

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

时间线按顺序保存 PassageName、导航标记、当前游标，以及进入每个历史项前的 `$variables` 图；不保存进程内 `StoryHistoryId`。加载时使用当前有效 HIR 逐项验证并重建时间线和状态关联，因此 PassageName 仍区分大小写，`StoryInit` 不得进入历史。Host 执行 back／forward 时先恢复目标项的进入前状态，再重放该 Passage；页面上的变量修改因此与原导航一致。

恢复顺序固定为：

1. 校验精确游戏身份；
2. 校验 Story history 与当前 HIR；
3. 完整解码当前及逐历史项的 Value 图；
4. 在临时所有权中建立完整的新 `$variables`、Story 时间线与历史状态关联；
5. 校验全部通过后一次性替换 `$variables`、清空 `_temporary` 并提交 Story；
6. RuntimeSession 根据当前启动脚本已注册的 ID 恢复 Reaction 状态；
7. RuntimeSession 只保留一份 State/Story/Reaction 回滚检查点，后续 Script 同步失败时统一恢复。

捕获直接借用活动 `$variables` 和已经隔离的历史快照进行 ValueGraph 编码。Story history 在运行期
保存 Passage 引用及进入前的持久状态，只有可移植存档边界写入 PassageName；因此 Save 大小取决于
实际 history 与其 `$variables`，而不是 Story 总 Passage 数。

这条入口只恢复稳定领域状态，不自动执行 Passage 生命周期或渲染。Host 后续应明确决定加载完成后从当前 Passage 的哪个生命周期阶段重新进入 Engine。

## Controller、Host 与生命周期

`SaveController` 不直接访问文件系统。它把游戏侧调用转换为有序
`SaveRequest`，每项包含进程内请求 ID、`Export`／`Import` 操作与不透明
`target`。Tauri 可以把 target 解释为槽位、相对文件或云端键；Godot 与其他
Host 可以采用自己的持久化方式。Core 不接受绝对路径语义。

调用顺序固定为：

1. `before(operation, hook)` 按注册顺序运行，可修改 target；
2. 非空 target 进入请求队列，游戏侧立即得到请求 ID；
3. Host 使用 `take()` 取得请求，执行平台 I/O，并调用 capture／restore；
4. Host 使用 `complete()` 回报 `Succeeded` 或带 Diagnostic 的 `Failed`；
5. 对应 `after` Hook 按注册顺序观察只读完成结果。

after 不能修改已导出文档或把失败伪装成成功。Hook 身份只在当前进程内有效，
不进入存档。`off(id)` 取消一条 before 或 after 订阅。

`.twee` 表达式由 Core 求值，不能直接访问 Worker 的 `Save` 全局。游戏作者在
`.ts/.js` 里把导出/导入封装成普通函数，再经 `State.global.extend` 暴露后即可在
`.twee` 中调用：

```ts
function saveGame(slot = "manual-1"): void {
  Save.export(slot)
}
```

```twee
<<run saveGame("manual-1")>>
```

`run` 会丢弃请求 ID，但导出请求仍进入 Host 队列；需要跟踪结果时应在 scripts
中保存 ID 并登记 after。该调用不等于“Core 写入 manual-1 文件”。Tauri
Binding 必须把全局 `Save` 对象连接到同一个 Rust `SaveController`。

## 当前限制

当前 Rust API：

- `SaveDocument::capture()`；
- `to_bytes()` / `from_bytes()`；
- `restore()`；
- `SaveError::diagnostic()`；
- `SaveController::export()` / `import()` / `take()` / `complete()`；
- `SaveLifecycleSubscriptions::before()` / `after()` / `off()`。

Tauri Host 已实现命名槽位与 `save/<target>.nsave` 落盘，侧栏面板和脚本请求复用同一保存边界。
仍未实现的是文件选择器、压缩/加密、缩略图、自动存档、云同步与迁移；这些属于后续 Host
能力，不写进当前基础文档格式。

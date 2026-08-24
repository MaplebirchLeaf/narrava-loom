# Narrava Save

> 状态：Core 文档、Host 请求与生命周期边界已实现
>
> 更新日期：2026-08-22

## 边界

Save 记录能够恢复游戏进度的持久领域数据，不保存宿主或当前执行栈：

| 保存 | 不保存 |
| --- | --- |
| `State.variables`（`$name`） | `State.global` |
| Story 完整导航时间线 | `State.setup` |
| Story 当前游标 | `State.temporary`（`_name`） |
| 每次 Passage 是否产生作者导航 | Macro `@locals`、`@args` |
| 精确游戏 ID 与版本 | Function、Macro Handler、Promise |
| Narrava Array/Object 引用图 | VM frame、Pending、Host/Renderer 对象 |

`global` 与 `setup` 属于启动环境，由配置、StoryInit 和 scripts 重新建立。`temporary` 与 Macro Local 只服务当前执行范围，加载后清空。

## 文档结构

`SaveDocument` 使用 JSON 作为首个可检查的交换格式，根层只有 `game`、`state` 与 `story`。当前没有 `format_version`：引擎尚未发布，也没有旧格式迁移需求；出现首个不兼容发布格式时再建立明确版本边界。

`.nsave` 是该文档的游戏侧文件后缀，当前内容就是 UTF-8 JSON，而不是 ZIP。
Core 只编码和解码文档；具体 Host 决定存放位置。综合示例在内存中展示捕获、JSON 编码与恢复，不把固定存档文件作为源码真值。

游戏身份使用精确 `id + version`。当前不提供隐式迁移，不允许另一游戏或另一版本直接恢复。未来迁移必须是独立、显式的输入转换，不进入正常 `restore()`。

## Value 图

Array 与 Object 不递归嵌入 JSON，而是使用单调节点 ID 建立图：

- 多个 `$variables` 指向同一集合时，恢复后仍共享同一身份；
- Array/Object 循环引用不会无限递归；
- String 保存 UTF-16 码元，保留孤立代理项；
- Number 保存原始 `f64` 位，保留 `NaN`、正负无穷和 `-0`；
- `undefined`、`null`、Boolean、Array、Object 与 String 可保存；
- NativeCallable、ScriptCallable 与 NativeNamespace 会返回 `save.unsupported_value`。

解码会拒绝 ID 0、悬空引用和节点类型不一致。Host 仍应在把外部文件交给 Core 前限制总文件大小；Core 的结构校验不能代替平台 I/O 配额。

## Story 与恢复事务

时间线按顺序保存 PassageName、导航标记与当前游标，不保存进程内 `StoryHistoryId`。加载时使用当前有效 HIR 逐项验证并重建时间线，因此 PassageName 仍区分大小写，`StoryInit` 不得进入历史。

恢复顺序固定为：

1. 校验精确游戏身份；
2. 校验 Story history 与当前 HIR；
3. 完整解码 Value 图；
4. 捕获活动 State/Story 检查点；
5. 替换 `$variables`、清空 `_temporary` 并重建 Story；
6. 运行时失败时同时回滚 State 与 Story。

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

游戏作者最终可以写：

```twee
<<run Save.export("manual-1")>>
```

`run` 会丢弃请求 ID，但导出请求仍进入 Host 队列；需要跟踪结果时应在 scripts
中保存 ID 并登记 after。该调用不等于“Core 写入 manual-1 文件”。Tauri
Binding 必须把全局 `Save` 对象连接到同一个 Rust `SaveController`。

## API 与后续

当前 Rust API：

- `SaveDocument::capture()`；
- `to_json()` / `from_json()`；
- `restore()`；
- `SaveError::diagnostic()`；
- `SaveController::export()` / `import()` / `take()` / `complete()`；
- `SaveLifecycleSubscriptions::before()` / `after()` / `off()`。

Tauri Host 已实现命名槽位与 `save/<target>.nsave` 落盘，侧栏面板和脚本请求复用同一保存边界。
仍未实现的是文件选择器、压缩/加密、缩略图、自动存档、云同步与迁移；这些属于后续 Host
能力，不写进当前基础文档格式。

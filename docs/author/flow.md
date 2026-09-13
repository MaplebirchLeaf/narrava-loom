# 游戏流程、随机与存档

## 初始化和导航

启动时先装载作者脚本与 Widget，再执行 `StoryInit` 和 `Start`。
`StoryInit` 用于逻辑初始化，不建立历史项，也不显示正文。
脚本顶层可以登记函数、地点和 Reaction；正式进度放在 `V`（Twee 的 `$` 变量）。

```ts
Story.has("Hall")
Story.current()
Story.get("Hall")
Story.visits("Hall")

Engine.goto("Hall")
Engine.back()
Engine.forward()
Engine.restart()
```

`Story` 查询故事，`Engine` 请求流程操作。Twee 导航使用 `link` 或 `goto`；
`include` 只执行当前位置的内容，不增加历史。回退和前进恢复目标页的入页状态并重放正文；
从旧历史位置进行新导航会截断原前进分支。

`Engine.restart()` 恢复本次启动脚本完成后的初始状态与随机序列，再开始游戏。
没有 `newGame()` 或特殊的 `StoryRestart` Passage。

## 根种子

根种子在脚本装载之前交给 Engine：

```toml
[engine]
seed = 42
```

省略时由 Host 生成。脚本读取只读十进制字符串 `Engine.seed`，完整保留 u64 精度。
脚本 `Math.random()` 与 Twee `random()`、`either(...)` 按调用顺序共用一条序列；
同一根种子、内容和调用顺序产生相同抽样。运行期间没有重新设种的作者 API。

```ts
V.reward = Math.random() < 0.5 ? "coin" : "herb"
```

失败与取消回滚随机进度，历史重放使用入页进度，读档恢复根种子与当前位置。
当前页和语言重绘使用临时序列；Header/Footer/Bar 的隔离执行也不消费正文进度。
行动结果应保存在 `V` 中，避免在反复执行的呈现正文中重新计算奖励。

## 保存游戏

| 保存 | 不保存 |
| --- | --- |
| `$` 变量与集合引用关系 | `global`、`setup`、`_` 临时变量、`@` 局部变量 |
| Story 历史、当前游标、各页入页状态 | VM 执行栈、未完成操作、脚本函数 |
| 当前及历史地点位置、Engine 随机进度 | 地点定义、临时重绘序列 |
| Reaction 启用、成功次数、销毁状态 | Reaction 定义和条件函数 |

```ts
const snapshot = Save.capture()
Save.restore(snapshot)
Save.export("quick")
Save.import("quick")
```

`capture/restore` 在内存中同步捕获和恢复；`export/import` 请求 Host 读写
`save/<target>.nsave`。target 限制为 1–80 个 ASCII 字母、数字、`-` 或 `_`。
请求返回不等于磁盘操作已完成，使用 Hook 获取最终结果：

```ts
Save.after("export", completion => {
  if (completion.succeeded) Logger.info("save", "保存完成")
  else Logger.error("save", completion.error ?? "保存失败")
})
```

`Save.before(operation, hook)` 按注册顺序执行；对 export/import 返回字符串可以修改 target。
`Save.after` 在真实结算后执行，不能把失败改成成功；`Save.off(subscription)` 移除 Hook。
四种 operation 均可订阅，Hook 和订阅身份不进入存档。

普通导航成功进入另一 Passage 后，官方 Host 写入 `autosave`；语言刷新、历史回溯和侧栏切换
不触发自动保存。导入失败保持原游戏状态，导入成功后重绘画面。

存档要求游戏 ID 和游戏版本精确匹配。当前文件 schema 为 5，不读取实验格式 1–4；
项目版本号与存档格式号各自独立。旧文件不会自动删除，通用迁移尚未实现。
内部编码和恢复校验见[Save 格式](../architecture/save-format.md)。

# Reaction

`Reaction` 用声明式规则描述“事实发生后，叙事如何响应”。scripts 只负责注册规则和条件；Runtime 负责候选索引、执行顺序、事务、次数、Save 与循环保护。Host 只呈现最终 Surface、转交输入并完成平台 IO。

## 最小示例

```ts
Reaction.add({
  id: "quest.completed",
  event: "quest:completed",
  cond: (payload) => payload.quest === "old_mine",
  widget: '<<QuestNotice "old_mine">>',
  replace: "quest-panel",
})

Reaction.add({
  id: "reputation.50",
  state: "$reputation",
  cond: ({ before, after }) => before < 50 && after >= 50,
  include: "ReputationNotice",
  once: true,
})

Reaction.add({
  id: "locked-room",
  lifecycle: true,
  passage: { match: ["LockedRoom"], tags: { all: ["locked"] } },
  include: "LockedRoomFallback",
  replace: "main",
  exit: true,
})
```

一条规则必须有唯一、非空且不含空白的 `id`，并且只能声明 `event`、`state`、`lifecycle` 三种触发源之一。

## 触发源

### Event

```ts
Event.emit("quest:completed", { quest: "old_mine" })
```

`event` 精确匹配事件名称，`cond(payload)` 接收 payload。条件也可以读取当前活动 State：

```ts
cond: (payload) => payload.quest === "old_mine" && V.quest_open === true
```

`Event.emit` 只把事实加入队列，不会从 setter 或脚本调用栈中重入 Engine。事件名称、订阅与 Engine 保留事件见 [Event](event.md)。

### State

`state` 只接受 `$name` 或 `$name.path` 形式的持久 State 路径，不观察 `T` 临时变量和 `setup`。

`cond({ before, after })` 中：

- `before` 是本次 Runtime 命令开始前的持久 State 快照值。
- `after` 是命令完成并进入 Reaction 安全点时的值。
- 它们不是每次 `V.x = ...` 的逐次赋值记录。同一命令依次写入 `40`、`45`、`50`，只观察“命令前 → 50”。
- Reaction 效果若继续修改 State，Runtime 会以“本轮效果前 → 本轮效果后”进入下一轮。
- 前后值严格相等时，该路径没有 State Reaction 候选。

State 条件不限于被观察路径，也可以组合其他游戏状态：

```ts
cond: ({ before, after }) => before < 50 && after >= 50 && V.chapter >= 3
```

### Lifecycle

`lifecycle: true` 在普通 Passage 的 Start 之后、正文之前执行。特殊 Passage 与 `include` fragment 不产生新的 lifecycle。

Lifecycle 没有触发参数，但仍可直接读取 `V`：

```ts
cond: () => V.lockdown === true
```

### 当前 Passage 过滤

`passage` 是 Event、State 与 lifecycle 共用的候选过滤器。它读取规则触发时的当前
Passage；没有活动 Passage 时，声明了 `passage` 的规则不会入选。写法包括：

```ts
passage: "Hall"
passage: /Room$/
passage: ["Hall", /Room$/]
passage: {
  match: ["Hall", /Room$/],
  exclude: ["LockedRoom"],
  tags: {
    any: ["public", "safe"],
    all: ["visited"],
    none: ["hidden"],
  },
}
```

`match` 为空表示不限制名称；`exclude` 命中即排除。`any` 至少命中一个，`all`
必须全部命中，`none` 必须全部不命中。正则保留 JavaScript flags；当前支持
`i`、`m`、`s`、`u`，不支持或重复的 flag 会在注册时明确报错，不会静默降级。

## 效果

一条规则可以组合内容、`goto`、`emit` 等效果，但内容来源只能有一个。

| 字段 | 含义 | 限制 |
| --- | --- | --- |
| `widget` | 执行一段已有 Twee Widget 调用源码 | 可直接追加；提供 `replace` 时替换目标 |
| `include` | 原地执行一个 Passage fragment | 可直接追加；提供 `replace` 时替换目标 |
| `replace` | 把内容包装为对稳定 Slot 或 Region 的替换 | 不能脱离 `widget` 或 `include` 单独出现 |
| `goto` | 通过 Engine 事务导航到 Passage | 会正常产生 lifecycle 与 history |
| `emit` | 继续派发结构化 Event | 进入同一安全队列并接受循环保护 |
| `exit` | 在正文前终止当前 Passage | 仅 lifecycle 可用 |

`widget` 与 `include` 不能同时出现。二者未提供 `replace` 时，内容按 Reaction
提交顺序追加到当前输出：

```ts
Reaction.add({
  id: "notice.append",
  event: "notice",
  include: "Notice",
})
```

需要更新已有区域时，先在 Twee 中声明稳定目标，再由 Reaction 替换：

```twee
<<slot "reputation-panel">>尚无声望消息。<</slot>>
```

```ts
Reaction.add({
  id: "reputation.notice",
  event: "reputation:changed",
  include: "ReputationNotice",
  replace: "reputation-panel",
})
```

`include` 不导航、不写 history，也不会让被包含 Passage 获得独立生命周期。内容自身若已请求导航，不能再与规则的 `goto` 同时请求第二次导航。

## 通用字段

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `cond` | 始终成立 | 除各触发源参数外，三者都能读取当前活动 `V` State |
| `enabled` | `true` | 初始启用状态 |
| `once` | `false` | 首次成功后销毁，不能与 `limit` 同时使用 |
| `limit` | 无限制 | 正整数；达到次数后禁用 |
| `tags` | `[]` | 作者自定义分类，只随状态返回，不参与触发 |

只有条件成立且效果成功提交才增加 `triggered`。条件返回普通 JavaScript truthy/falsy 值。

## 管理 API

```ts
const status = Reaction.add(definition)
Reaction.get("quest.completed")
Reaction.disable("quest.completed")
Reaction.enable("quest.completed")
Reaction.reset("quest.completed")
```

状态对象为只读快照：

```ts
interface NarravaReactionStatus {
  readonly id: string
  readonly enabled: boolean
  readonly triggered: number
  readonly tags: readonly string[]
}
```

- `add` 注册规则并返回初始状态；重复 ID 会失败。
- `get` 返回当前状态；不存在或已经被 `once` 销毁时返回 `undefined`。
- `enable`、`disable` 返回状态是否发生变化。
- `reset` 把次数与启用状态恢复到定义初值，但不能复活已经被 `once` 销毁的规则。
- `limit` 达到后 `enable` 返回 `false` 且规则保持禁用；只有 `reset` 清零次数后才能重新启用。

## 执行顺序与事务

字段在对象中的书写顺序不影响执行。Runtime 固定使用两个阶段：

1. **解析阶段**：按候选顺序检查 `passage`，执行 `cond`，计算动态 `emit.payload`，
   再解析并发布派生 Event 链。任何一步失败都不会留下成功次数。
2. **应用阶段**：对已经解析的每条规则依次执行内容（`widget` 或 `include`），再执行
   `replace`（没有时追加），然后执行 `goto`；只有没有 `goto` 时才应用 `exit`。

一次普通命令完成后，Runtime 进入统一安全点：

1. 按注册顺序处理已排队 Event Reaction。
2. 比较命令前后的持久 State，按路径与注册顺序处理 State Reaction。
3. 按上述固定顺序应用效果；新的 State 变化进入下一轮。
4. 队列稳定后一次性发布最终 Surface；`goto` 继续走正常 Engine 导航。

Lifecycle Reaction 位于目标 Passage 的 Start 与正文之间，不等待上述命令后安全点。单个安全点最多执行 256 轮；Event 后代链重复同一“事件 + 规则”会作为循环拒绝。一个安全点只能提交一次 `goto`。

Reaction 与触发它的命令属于同一事务。条件、内容、导航、脚本同步或循环检查失败时，Runtime 回滚 State、Story、Reaction 次数、当前 Surface 与交互状态，不把半成品交给 Host。

## Save

Save 只保存规则的 `id`、启用状态、成功次数与销毁状态。定义、条件函数和索引仍由启动 scripts 注册；导入时把存档状态恢复到当前定义集合。未知 ID、重复 ID 或不符合当前 `once/limit` 定义的状态会拒绝整次恢复。

完整可操作示例见 `examples` 的“Reaction 规则系统”页面。

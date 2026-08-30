# Event

`Event` 是 Runtime 内的结构化事实流。它适合表达“任务已经完成”“物品已经取得”这类已经发生的事实；`Reaction` 再决定这些事实是否产生叙事效果。Event 不是 DOM Event，也不会直接调用订阅回调。

## 发出作者事件

```ts
const sequence = Event.emit("quest:completed", {
  quest: "old_mine",
  reward: 500,
})
```

- 名称区分大小写，不能为空或包含空白。
- 建议使用 `领域:动作`，例如 `quest:accepted`、`inventory:changed`。
- payload 只能是 `NarravaData`：空值、布尔值、数值、字符串及由它们组成的数组和普通对象。
- `emit` 返回本局 Runtime 内单调递增的事件序号。
- `passage:*` 是 Engine 保留名称，作者不能通过 `Event.emit` 伪造。

## 拉取订阅

```ts
const quests = Event.subscribe({ name: "quest:completed" })

Event.emit("quest:completed", { quest: "old_mine" })

for (const event of Event.take(quests) ?? []) {
  Logger.info("quest", `收到 ${event.name} #${event.sequence}`)
}

Event.unsubscribe(quests)
```

`subscribe({ name })` 只接收订阅之后发生且名称完全相等的事件；省略过滤器会接收之后发生的所有作者与 Engine 事件。订阅不是回调：`take(id)` 在脚本下一次获得执行机会时取出并清空积压，有效订阅没有新事件时返回 `[]`，未知或已取消的 ID 返回 `undefined`。`unsubscribe(id)` 返回是否实际取消了订阅。

```ts
interface NarravaEventRecord {
  readonly sequence: number
  readonly name: string
  readonly payload: NarravaData
}
```

订阅句柄和待取队列只属于当前 Runtime，不进入 Save。需要持久化的游戏事实应写入 `V`。

## Event 与 Reaction

只需要叙事响应时，不必先 `subscribe`；Reaction 直接匹配作者事件：

```ts
Reaction.add({
  id: "quest.reward",
  event: "quest:completed",
  cond: (payload) => payload.quest === "old_mine" && V.rewards_enabled === true,
  emit: { name: "inventory:changed", payload: { item: "old_key" } },
  widget: '<<QuestReward "old_key">>',
  replace: "quest-panel",
})
```

`cond` 可以同时读取 payload 与当前活动 `V` State。Reaction 的 `emit` 会继续同一受保护的事件链，也会发布给普通 Event 订阅者；Runtime 会限制执行量并拒绝后代循环。完整效果规则见 [Reaction](reaction.md)。

## Engine Passage 事件

Engine 自动发布五个只读生命周期事件，payload 均为 `{ passage: string, tags: readonly string[] }`：

| 事件名 | 时机 |
| --- | --- |
| `passage:init` | 确认进入 Passage |
| `passage:start` | 正文即将执行 |
| `passage:render` | Core 已形成 Surface |
| `passage:display` | 输出进入 Host 显示阶段 |
| `passage:end` | 真正离开当前 Passage |

```ts
const passage_start = Event.subscribe({ name: "passage:start" })
```

这些事件用于观察生命周期，不进入作者 `Event.emit` 队列；需要在进入正文前执行规则时，应使用 Lifecycle Reaction。`include` fragment 不创建独立 Passage 生命周期。

## Event、State 与 Logger 怎么选

- 已发生、可携带数据、允许多个消费者观察：`Event`。
- 需要跨命令或 Save 保留的游戏事实：`V`。
- 根据 Event/State/Passage 自动产生叙事效果：`Reaction`。
- 仅供开发者诊断、不参与游戏逻辑：`Logger`。

# 脚本 API 索引

游戏脚本直接使用下列全局；完整类型签名位于
[`bindings/typescript/narrava.d.ts`](../../bindings/typescript/narrava.d.ts)。Worker 没有
`window`、`document` 或 Tauri API，也不存在统一的 `narrava` 聚合对象。

## `State`

日常状态使用属性语法：

- `V.name`：持久变量，与 Twee `$name` 相同，进入存档；
- `T.name`：临时变量，与 Twee `_name` 相同，恢复存档时清空；
- `setup.name`：启动配置，与 Twee `setup.name` 相同。

三者支持读取、赋值、动态方括号、`in`、`Object.keys()` 和 `delete`，并直接代理活动 Rust
State，不保留 JavaScript 镜像。需要旧值或批量导入时使用完整 `State` API：

- `State.global/variables/temporary.get(name)`、`has(name)`、`set(name, value)`、`del(name)`
- `State.global/variables/temporary.extend(values)`
- `State.setup.get()`、`State.setup.set(value)`

## `Location`

- 注册与查询：`add`、`get`、`places`、`locate`。
- 玩家位置：`current`、`move`。

参数、返回值以及坐标、导航和恢复规则统一见 [Location](../author/location.md)。

## `Macro`

- 定义：`add`、`update`、`del`、`get`、`has`
- 生命周期：`before(name, hook)`、`after(name, hook)`、`off(subscription)`
- 定义项声明 `body`、`arguments`、`execution` 和 `handler`

## `Engine` 与 `Story`

- `Engine.seed`（只读根种子字符串）、`Engine.started`
- `Engine.goto(target)`、`back()`、`forward()`、`restart()`
- `Story.has(name)`、`current()`、`get(name)`、`visits(name)`

## `Logger`

- 写日志：`trace`、`debug`、`info`、`warn`、`error`
- 读取订阅：`subscribe(filter?)`、`take(subscription)`、`unsubscribe(subscription)`

脚本与宿主共用有界 Core Logger，订阅按过滤条件接收后续记录。控制台与用法见
[调试与控制台](../author/debugging.md)。

## `Event`

- 作者事件：`emit(name, payload?)`
- 订阅：`subscribe(filter?)`、`take(subscription)`、`unsubscribe(subscription)`
- Engine 保留事件：`passage:init`、`passage:start`、`passage:render`、
  `passage:display`、`passage:end`
- 完整使用方式与事件链见 [Event](../author/events.md)

## `Reaction`

- `add(definition)`、`get(id)`、`enable(id)`、`disable(id)`、`reset(id)`
- 触发源三选一：`event`、`state: "$path"`、`lifecycle: true`
- 当前 Passage 过滤：`passage` 对 Event、State、lifecycle 都有效，支持字符串、数组、
  `RegExp`（保留 `i/m/s/u`）与名称/Tag 选择器
- 效果：`widget`、`include`、可选 `replace`、`goto`、静态或动态 payload 的 `emit`，
  以及 lifecycle 专用 `exit`；`widget/include` 未提供 `replace` 时追加到当前输出
- 状态：`enabled`、`once`、`limit`、`tags`；完整规则见 [Reaction](../author/events.md)

## `Host`

- `await Host.delay(milliseconds)`：暂停当前异步 Macro，时间到后恢复同一 Engine 事务；
- 毫秒数必须在 `0..=86400000`，一次 Macro 同时只能等待一个 Host 操作；
- 普通 `setTimeout`、`fetch`、DOM 和 Tauri API 不存在于游戏 Worker。

## `Save`

- 内存：`capture()`、`restore(json)`
- Host 槽位：`export(target?)`、`import(target?)`
- 生命周期：`before(operation, hook)`、`after(operation, hook)`、`off(subscription)`
- operation：`capture`、`restore`、`export`、`import`

## `Audio`

- `Audio.play(resource, { channel?, tags?, loop?, volume? })`
- `Audio.stop(channel)`
- 声明与生命周期见[Audio](../author/audio.md)。

## `Resource` 与 `I18n`

- `Resource.paths()`、`has(path)`、`pick(candidates)`、`info(path)`、`read(path)`、
  `text(path)`
- `I18n.defaultLocale`、`I18n.locale`、`I18n.select(locale)`、`I18n.export()`

## `Surface`

- `text(text, { key?, styles?, color?, delay?, heading? })`
- `hardBreak()`
- `image(resource, { key?, alt? })`
- `region(region, children, { key? })`
- `component(capability, version, properties, fallback, { key? })`
- `action(label, "dismiss", { key?, role? })`
- `fragment(...children)`

Surface builder 接受纯数据和语义节点，不接受 HTML、CSS class 或 DOM 对象。
文本参数的取值见[Print 文本选项](macros.md#print-文本选项)，组合示例见[脚本](../author/scripting.md)。

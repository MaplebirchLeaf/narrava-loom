# Story、Engine、Event、I18n 与 Save

## Story、Engine 与重新开始

声明文件提供：

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

命名是 `Engine.restart()`，不是 `newGame()`，也没有特殊 `StoryRestart` Passage。

但请注意当前开发版边界：普通 Twee 导航和 Tauri Interaction 已接通；脚本侧 Engine/Story 的
完整实时同步与平台动作仍在收束。现阶段正式剧情导航优先使用 `<<link>>` 和 `<<goto>>`，不要把
关键流程只押在脚本 `Engine.goto()` 上。

## Logger 与 Event

### 19.1 当前到底有哪些基础事件

当前有 5 个 Engine 自动发出的内置 Passage 事件：

| 事件名 | 发出时机 |
|---|---|
| `passage:init` | 确认进入 Passage、执行正文之前 |
| `passage:start` | Passage 正文即将开始执行 |
| `passage:render` | Core 已形成 Presentation 输出 |
| `passage:display` | 输出进入 Host 显示阶段 |
| `passage:end` | 真正离开当前 Passage |

订阅 `PassageInit` 对应事件：

```ts
const passageInit = Event.subscribe({ name: "passage:init" })

// 在脚本下一次通过 Macro/函数获得执行机会时排空：
for (const event of Event.take(passageInit) ?? []) {
  Logger.debug("passage", `进入 ${event.payload.passage}`)
}
```

五个事件的 payload 都是 `{ passage: string, tags: readonly string[] }`。它们的名字由 Engine
保留，游戏不能用 `Event.emit("passage:init", ...)` 伪造。`story:start`、`save:complete` 等
仍不是内置事件。示例中的 `quest:completed` 和 `game:ready` 是作者自定义事件。

Logger 用于开发诊断：

```ts
Logger.info("game.start", "游戏脚本已加载")
Logger.warn("game.balance", "金币数量异常")
```

Event 用于游戏自己的结构化事实：

```ts
const subscription = Event.subscribe({ name: "quest:completed" })
Event.emit("quest:completed", { quest: "library" })
const records = Event.take(subscription)
Event.unsubscribe(subscription)
```

### 19.2 Event 四个方法的精确行为

- `emit(name, payload)`：记录并投递事件，返回从 1 开始递增的序号；
- `subscribe({ name })`：只接收订阅之后发生、名称完全相等的事件；
- `subscribe()`：接收订阅之后发生的所有作者事件；
- `take(id)`：取出并清空当前积压；有效订阅无新事件时返回 `[]`；
- 对不存在或已经取消的 ID 调用 `take` 返回 `undefined`；
- `unsubscribe(id)`：取消成功返回 `true`，ID 不存在返回 `false`。

名称区分大小写，不能为空，不能包含空格、换行或其他空白。建议采用 `领域:动作`，例如
`quest:accepted`、`quest:completed`、`inventory:changed`；这些只是命名建议，不是内置事件。
载荷只能是 Narrava 数据：空值、布尔值、数值、字符串以及由它们组成的数组和普通对象，不能
放函数、DOM、Tauri 对象或循环引用。

```ts
interface NarravaEventRecord {
  readonly sequence: number
  readonly name: string
  readonly payload: NarravaData
}
```

订阅不是回调，不会自动执行函数。脚本需要在下一次获得执行机会时调用 `Event.take(id)`。
Event 也不是 DOM Event，不能使用 `window.addEventListener`。当前 Tauri Runtime 已实现作者
事件的订阅、投递、排空和取消，并自动投递五个 Passage 生命周期事件；Logger 和 Event
都没有玩家可见面板。

Engine 内部仍以同步事务回调执行
`PassageInit → PassageStart → PassageRender → PassageDisplay → PassageEnd`，Tauri Host 在每个
回调中把同名事实投递进 Event。事件订阅本身不是回调，不允许在 `passage:init` 的订阅处理器
里阻塞或改写当前事务；脚本只能在下一次获得执行机会时 `take()`。include 不创建独立 Passage
生命周期，StoryInit 也不是 PassageInit。

## I18n 多语言

原文语言由 `default_locale` 决定。一个解包语言目录：

```text
languages/en/
├── manifest.json
├── translations.nmsg
└── dictionary.json
```

- `manifest.json`：语言、版本、目标游戏兼容信息；
- `translations.nmsg`：稳定文本身份对应的译文；
- `dictionary.json`：动态运行时值的翻译；
- `.nlang`：上述内容的单语言发布包，不是第四种翻译格式。

脚本可读并导出译者模板：

```ts
I18n.defaultLocale
I18n.locale
const templateJson = I18n.export()
```

`I18n.export()` 返回当前编译故事的完整、格式化 JSON 翻译模板，包含 `language`、`dictionary`
和按稳定文本 ID 排列的 `passages`。Rust Worker 没有浏览器 `File` 和下载能力，因此返回
`string`；把字符串保存成文件属于 Host 功能。

开发运行时会直接校验并导入 `languages/<locale>/` 解包目录；发行构建会把同一目录编码为
`languages/<locale>.nlang`，发行 Host 再导入该包。两条路径共用 Core 校验，不存在只在开发
模式可用的宽松翻译格式。

Core 已有 fallback 和翻译数据模型；Tauri 提供语言查询与切换命令，但可见的语言选择界面仍由
游戏作者通过脚本和 Twee 定义。不要在脚本中自行复制一套语言状态。

详细格式见 [/docs/architecture/i18n.md](/docs/architecture/i18n.md)。

## Save 存档

进入存档：`$` 变量、Story 历史以及 Core 规定的数据。
不进入存档：`_` 临时变量、`@` 局部变量、脚本函数、DOM、Blob URL、平台对象。

Core 已实现存档捕获、校验和恢复模型。声明文件包含：

```ts
const json = Save.capture()
Save.restore(json)
Save.export()
Save.import()
```

Save 生命周期 Hook：

```ts
const beforeExport = Save.before("export", ({ target }) => {
  Logger.info("save", `准备导出到 ${target}`)
  return target === "quick" ? "quick-backup" : undefined
})

const afterExport = Save.after("export", completion => {
  if (completion.succeeded) Logger.info("save", "导出完成")
  else Logger.error("save", completion.error ?? "导出失败")
})

Save.off(beforeExport)
Save.off(afterExport)
```

支持 `capture/restore/export/import` 四种 operation。before 按登记顺序执行；export/import 的
before 可以返回新字符串改写 Host target。after 只在操作取得真实完成结果后执行，不能把失败
改成成功。capture/restore 在 Worker 内同步完成；export/import 由 Tauri Host 写入或读取游戏
目录中的 `save/<target>.nsave`，磁盘操作完成后才触发 after。target 只允许 1 至 80 个 ASCII
字母、数字、`-` 或 `_`，所以不能借此访问 `save/` 外的文件。

详细边界见 [/docs/architecture/save.md](/docs/architecture/save.md)。

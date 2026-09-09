# Story、Engine、Logger 与 I18n

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

## Logger

Logger 只用于开发诊断：

```ts
Logger.info("game.start", "游戏脚本已加载")
Logger.warn("game.balance", "金币数量异常")
```

结构化游戏事实、拉取订阅、Engine Passage 事件及其与 Reaction 的关系统一见 [Event](event.md)。

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
I18n.select("en")
const templateJson = I18n.export()
```

`I18n.export()` 返回当前编译故事的完整、格式化 JSON 翻译模板，包含 `language`、`dictionary`
和按稳定文本 ID 排列的 `passages`。Rust Worker 没有浏览器 `File` 和下载能力，因此返回
`string`；把字符串保存成文件属于 Host 功能。

开发运行时会直接校验并导入 `languages/<locale>/` 解包目录；发行构建会把同一目录编码为
`languages/<locale>.nlang`，发行 Host 再导入该包。两条路径共用 Core 校验，不存在只在开发
模式可用的宽松翻译格式。

`I18n.select(locale)` 把切换请求交给 Host，再由 Runtime 校验语言包并同步 `I18n.locale`；它不会
在脚本中另建语言状态。故事已经开始时，Runtime 会恢复当前 Passage 的进入前 `$` 快照并立即重放
完整呈现帧，包括 `Bar` 与 `BarStowed`；同名刷新导航会被折叠，因此不会新增历史项或重复累加
持久变量。可见的语言选择界面仍由游戏作者通过脚本和 Twee 定义。

详细格式见 [/docs/architecture/i18n.md](/docs/architecture/i18n.md)。

## 存档

存读档用法、保存范围与失败处理见 [Save](save.md)。

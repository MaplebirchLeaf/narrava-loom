# Story、Engine、Logger、I18n 与 Save

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
在脚本中另建语言状态。可见的语言选择界面仍由游戏作者通过脚本和 Twee 定义。

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

/// <reference types="@narrava-loom/types" />
// 综合示例脚本：演示脚本全局（State/Resource/Story/Engine/Save/Logger/I18n/Event）
// 与 Presentation 宏的用法；这些函数经 State.global 暴露后可在 .twee 中调用。

/** 记录客人名到 temporary 并返回欢迎语。 */
function scriptedGreeting(name: string): string {
  State.temporary.set("lastGuest", name)
  return `欢迎阅读，${name}`
}

/** 按优先级挑选并读取指南文本；找不到时给出提示。 */
function resourceSummary(): string {
  const guide = Resource.pick(["data/guide.zh-CN.txt", "data/guide.txt"])
  return guide === undefined ? "没有指南" : (Resource.text(guide) ?? "指南不是文本")
}

/** 若大厅存在且当前不在大厅，则导航回大厅。 */
function returnToHall(): void {
  if (Story.has("Hall") && Story.current()?.name !== "Hall") Engine.goto("Hall")
}

// 存档、读档、日志与语言都是 Worker ECMAScript 全局（Save/Logger/I18n），
// .twee 表达式由 Core 求值，不能直接调用它们。游戏作者把这些能力封装成
// 普通函数并经 State.global 暴露后，就能在 .twee 里用 <<run/print 函数(...)>> 调用。

/** 请求 Host 把存档导出到指定槽位，并返回提示文本。 */
function saveGame(slot = "manual-1"): string {
  Save.export(slot)
  return `已请求导出存档：${slot}`
}

/** 请求 Host 从指定槽位导入存档，并返回提示文本。 */
function loadGame(slot = "manual-1"): string {
  Save.import(slot)
  return `已请求读取存档：${slot}`
}

/** 向 Logger 写一条 info 日志。 */
function logStory(message: string, target = "story"): void {
  Logger.info(target, message)
}

/** 向 Logger 写一条 warn 日志。 */
function logWarnStory(message: string, target = "story"): void {
  Logger.warn(target, message)
}

/** 读取当前 locale。 */
function currentLocale(): string {
  return I18n.locale
}

/** 读取默认 locale。 */
function defaultLocale(): string {
  return I18n.defaultLocale
}

/** 导出译者模板，并把完整 JSON 留在 Host 日志中供开发阶段复制。 */
function exportI18nTemplate(): string {
  const template = I18n.export()
  Logger.info("i18n.export", template)
  return `I18n 模板已导出（${template.length} 字符）`
}

// 内联宏：输出一个装饰字符，演示最简宏定义。
Macro.add("sparkle", {
  body: "inline",
  arguments: "list",
  execution: "sync",
  handler: () => "✨",
})

// 展开态侧栏演示：填充 Bar 特殊 Passage 的语义文本。
Macro.add("barDemo", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () =>
    Presentation.fragment(
      Presentation.text("大厅状态<br>", { key: "bar-heading", styles: ["strong"] }),
      Presentation.text("天气：小雨 · 17°C<br>", { key: "bar-weather", tone: 34 }),
      Presentation.text("人物：Maple<br>", { key: "bar-character", styles: ["strong"] }),
      Presentation.text("状态：轻微疼痛<br>", { key: "bar-condition", tone: 34 }),
      Presentation.text("提示：藏书室似乎有动静。<br>", { key: "bar-hint", tone: 3 }),
      Presentation.text("管理界面由游戏脚本和 Twee 自行定义。<br>", {
        key: "management-hint",
        tone: 3,
      }),
    ),
})

// 收拢态侧栏演示：用极短文本填充 BarStowed 特殊 Passage。
Macro.add("barStowedDemo", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () =>
    Presentation.fragment(
      Presentation.text("雨", { key: "bar-stowed-weather", tone: 34 }),
      Presentation.text("痛", { key: "bar-stowed-condition", styles: ["strong"], tone: 34 }),
      Presentation.text("!", { key: "bar-stowed-hint", styles: ["strong"], tone: 8 }),
    ),
})

// 综合演示：region、component、image、语义字形与状态色阶。
Macro.add("presentationDemo", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () =>
    Presentation.fragment(
      Presentation.region(
        "header",
        [
          Presentation.text("Presentation V2", {
            key: "demo-title",
            styles: ["strong"],
            // tone 只决定文字颜色；16 是 Host 色阶中的橙色。
            tone: 16,
          }),
        ],
        { key: "demo-header" },
      ),
      Presentation.region(
        "bar",
        [
          Presentation.text("测试工具", { key: "bar-heading", styles: ["strong"] }),
          Presentation.component(
            "meter",
            1,
            {
              label: "探索进度",
              value: 72,
              min: 0,
              max: 100,
            },
            ["探索进度：72 / 100"],
            { key: "exploration-meter" },
          ),
        ],
        { key: "demo-bar" },
      ),
      Presentation.text("强调文本。 ", { styles: ["emphasis"] }),
      Presentation.text("重要文本。 ", { styles: ["strong"] }),
      Presentation.text("const answer = 42<br>", { styles: ["code"], tone: 34 }),
      Presentation.text("新增内容。 ", { styles: ["inserted"], tone: 32 }),
      Presentation.text("删除内容。 ", { styles: ["deleted"], tone: 8 }),
      // marked 自身表示高亮底色，不需要再叠加 tone。
      Presentation.text("需要留意。 ", { styles: ["marked"] }),
      Presentation.text("危险状态。<br>", { styles: ["strong"], tone: 8 }),
      Presentation.image("images/loom.svg", {
        key: "loom-image",
        alt: "由经纬线组成的 Narrava Loom 示意图",
        caption: "图片由 Resource 逻辑路径加载。",
      }),
      Presentation.component(
        "future-card",
        1,
        { title: "未知组件" },
        [Presentation.text("Host 不认识该组件，因此显示这段 fallback。", { tone: 3 })],
        { key: "fallback-demo" },
      ),
      Presentation.region(
        "footer",
        [Presentation.text("当前示例：语义渲染与 Resource", { key: "demo-footer", tone: 3 })],
        { key: "demo-footer-region" },
      ),
    ),
})

// Dialog 演示：页签、动作按钮及其角色（default/primary/secondary/danger）。
Macro.add("dialogDemo", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () =>
    Presentation.fragment(
      Presentation.region(
        "header",
        [Presentation.text("Dialog 与按钮", { key: "dialog-page-title", styles: ["strong"] })],
        { key: "dialog-page-header" },
      ),
      Presentation.text("进入本页时会打开语义 Dialog。关闭后仍可阅读正文并返回大厅。"),
      Presentation.region(
        "dialog",
        [
          Presentation.text("第一页", { key: "dialog-page-one", heading: 2 }),
          Presentation.text("默认显示第一页。点击标题栏中的页签可以切换内容。"),
          Presentation.action("默认按钮", "dismiss", { key: "default-action", role: "default" }),
          Presentation.action("主要按钮", "dismiss", { key: "primary-action", role: "primary" }),
          Presentation.text("第二页", { key: "dialog-page-two", heading: 2 }),
          Presentation.text("第二页继续展示次要与危险动作。"),
          Presentation.action("次要按钮", "dismiss", {
            key: "secondary-action",
            role: "secondary",
          }),
          Presentation.action("危险按钮", "dismiss", { key: "danger-action", role: "danger" }),
        ],
        { key: "demo-dialog" },
      ),
    ),
})

const readyEvents = Event.subscribe({ name: "game:ready" })
Event.emit("game:ready", { locale: I18n.locale, resources: Resource.paths().length })
Logger.info("example.script", `综合示例脚本已加载：${I18n.locale}`)
State.global.extend({
  scriptedGreeting,
  resourceSummary,
  returnToHall,
  saveGame,
  loadGame,
  logStory,
  logWarnStory,
  currentLocale,
  defaultLocale,
  exportI18nTemplate,
  readyEvents,
  difficulty: 3,
})

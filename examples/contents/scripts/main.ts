/// <reference types="@narrava-loom/types" />

function scriptedGreeting(name: string): string {
  State.temporary.set("lastGuest", name)
  return `欢迎阅读，${name}`
}

function resourceSummary(): string {
  const guide = Resource.pick(["data/guide.zh-CN.txt", "data/guide.txt"])
  return guide === undefined ? "没有指南" : (Resource.text(guide) ?? "指南不是文本")
}

function returnToHall(): void {
  if (Story.has("Hall") && Story.current()?.name !== "Hall") Engine.goto("Hall")
}

function exportSave(): void {
  Save.export()
}

Macro.add("sparkle", {
  body: "inline",
  arguments: "list",
  execution: "sync",
  handler: () => "✨",
})

Macro.add("barDemo", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () =>
    Presentation.fragment(
      Presentation.text("大厅状态", { key: "bar-heading", styles: ["heading2"] }),
      Presentation.text("天气：小雨 · 17°C", { key: "bar-weather", tone: "informational" }),
      Presentation.text("人物：Maple", { key: "bar-character", styles: ["strong"] }),
      Presentation.text("状态：轻微疼痛", { key: "bar-condition", tone: "warning" }),
      Presentation.text("提示：藏书室似乎有动静。", { key: "bar-hint", tone: "muted" }),
      Presentation.text("管理界面由游戏脚本和 Twee 自行定义。", {
        key: "management-hint",
        tone: "muted",
      }),
    ),
})

Macro.add("barStowedDemo", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () =>
    Presentation.fragment(
      Presentation.text("雨", { key: "bar-stowed-weather", tone: "informational" }),
      Presentation.text("痛", { key: "bar-stowed-condition", styles: ["strong"], tone: "warning" }),
      Presentation.text("!", { key: "bar-stowed-hint", styles: ["strong"], tone: "critical" }),
    ),
})

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
            styles: ["heading1"],
            tone: "accent",
          }),
        ],
        { key: "demo-header" },
      ),
      Presentation.region(
        "bar",
        [
          Presentation.text("测试工具", { key: "bar-heading", styles: ["heading2"] }),
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
      Presentation.text("强调文本。", { styles: ["emphasis"] }),
      Presentation.text("重要文本。", { styles: ["strong"] }),
      Presentation.text("const answer = 42", { styles: ["code"], tone: "informational" }),
      Presentation.text("新增内容。", { styles: ["inserted"], tone: "positive" }),
      Presentation.text("删除内容。", { styles: ["deleted"], tone: "negative" }),
      Presentation.text("需要留意。", { styles: ["marked"], tone: "warning" }),
      Presentation.text("危险状态。", { styles: ["strong"], tone: "critical" }),
      Presentation.image("images/loom.svg", {
        key: "loom-image",
        alt: "由经纬线组成的 Narrava Loom 示意图",
        caption: "图片由 Resource 逻辑路径加载。",
      }),
      Presentation.component(
        "future-card",
        1,
        { title: "未知组件" },
        [Presentation.text("Host 不认识该组件，因此显示这段 fallback。", { tone: "muted" })],
        { key: "fallback-demo" },
      ),
      Presentation.region(
        "footer",
        [Presentation.text("当前示例：语义渲染与 Resource", { key: "demo-footer", tone: "muted" })],
        { key: "demo-footer-region" },
      ),
    ),
})

Macro.add("dialogDemo", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () =>
    Presentation.fragment(
      Presentation.region(
        "header",
        [Presentation.text("Dialog 与按钮", { key: "dialog-page-title", styles: ["heading1"] })],
        { key: "dialog-page-header" },
      ),
      Presentation.text("进入本页时会打开语义 Dialog。关闭后仍可阅读正文并返回大厅。"),
      Presentation.region(
        "dialog",
        [
          Presentation.text("第一页", { key: "dialog-page-one", styles: ["heading2"] }),
          Presentation.text("默认显示第一页。点击标题栏中的页签可以切换内容。"),
          Presentation.action("默认按钮", "dismiss", { key: "default-action", role: "default" }),
          Presentation.action("主要按钮", "dismiss", { key: "primary-action", role: "primary" }),
          Presentation.text("第二页", { key: "dialog-page-two", styles: ["heading2"] }),
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
  exportSave,
  readyEvents,
  difficulty: 3,
})

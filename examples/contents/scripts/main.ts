/// <reference types="@narrava-loom/types" />
// 综合示例脚本：先定义供 Twee 调用的函数，再注册 Surface Macro，最后统一公开函数。

// setup 是启动配置，不进入存档；脚本模块加载时建立本示例需要的稳定字段。
setup.build = "grand-tour"
V.reaction_enabled = V.reaction_enabled ?? true

/** 记录客人名到 temporary 并返回欢迎语。 */
function scriptedGreeting(name: string): string {
  T.lastGuest = name
  return `欢迎阅读，${name}`
}

/** 演示 V/T/setup 与 Twee 的 $/_/setup 共享同一份活动 Rust State。 */
function inspectState(): string {
  V.scriptChecks = typeof V.scriptChecks === "number" ? V.scriptChecks + 1 : 1
  T.lastTool = "state"
  return `脚本检查 ${V.scriptChecks} 次；build=${String(setup.build)}`
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

/** 产生供 Reaction 示例消费的结构化 Event。 */
function emitQuestCompleted(): void {
  Event.emit("quest:completed", { quest: "old_mine", reward: 500 })
}

/** 产生一个会由 Reaction 导航的 Event。 */
function emitReactionGoto(): void {
  Event.emit("demo:reaction_goto")
}

/** 跨过 State Reaction 示例的阈值。 */
function raiseReputation(): void {
  V.reputation = 50
}

Reaction.add({
  id: "demo.quest.completed",
  event: "quest:completed",
  passage: /^reactiongallery$/i,
  cond: (payload) =>
    typeof payload === "object" &&
    payload !== null &&
    "quest" in payload &&
    payload.quest === "old_mine" &&
    V.reaction_enabled === true,
  widget: '<<highlightCard "Event Reaction：旧矿井任务已结算。">>',
  replace: "reaction-result",
  emit: {
    name: "quest:notice",
    payload: (payload: NarravaData) => ({ source: "reaction", original: payload }),
  },
  limit: 3,
  tags: ["example", "event"],
})

Reaction.add({
  id: "demo.reputation.threshold",
  state: "$reputation",
  cond: ({ before, after }) =>
    typeof before === "number" && typeof after === "number" && before < 50 && after >= 50,
  include: "ReactionReputationNotice",
  once: true,
  tags: ["example", "state"],
})

Reaction.add({
  id: "demo.quest.notice",
  event: "quest:notice",
  passage: "ReactionGallery",
  cond: (payload) =>
    typeof payload === "object" &&
    payload !== null &&
    "source" in payload &&
    payload.source === "reaction",
  widget: '<<highlightCard "动态 emit payload 已进入后续 Event 链。">>',
  tags: ["example", "event-chain"],
})

Reaction.add({
  id: "demo.lifecycle.guard",
  lifecycle: true,
  passage: { match: ["ReactionExitDemo"], tags: { all: ["reaction"] } },
  include: "ReactionLockdown",
  replace: "main",
  exit: true,
  tags: ["example", "lifecycle"],
})

Reaction.add({
  id: "demo.goto",
  event: "demo:reaction_goto",
  goto: "ReactionGotoTarget",
  tags: ["example", "navigation"],
})

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
  handler: () => {
    const english = I18n.locale === "en"
    return Surface.fragment(
      Surface.text(english ? "Hall status" : "大厅状态", {
        key: "bar-heading",
        styles: ["strong"],
      }),
      Surface.hardBreak(),
      Surface.text(english ? "Weather: light rain · 17°C" : "天气：小雨 · 17°C", {
        key: "bar-weather",
        color: 34,
      }),
      Surface.hardBreak(),
      Surface.text(english ? "Character: Author" : "人物：Author", {
        key: "bar-character",
        styles: ["strong"],
      }),
      Surface.hardBreak(),
      Surface.text(english ? "Condition: mild pain" : "状态：轻微疼痛", {
        key: "bar-condition",
        color: 34,
      }),
      Surface.hardBreak(),
      Surface.text(
        english ? "Hint: something is stirring in the library." : "提示：藏书室似乎有动静。",
        {
          key: "bar-hint",
          color: 3,
        },
      ),
      Surface.hardBreak(),
      Surface.text(
        english
          ? "The game script and Twee define the management interface."
          : "管理界面由游戏脚本和 Twee 自行定义。",
        {
          key: "management-hint",
          color: 3,
        },
      ),
      Surface.hardBreak(),
    )
  },
})

// 收拢态侧栏演示：用极短文本填充 BarStowed 特殊 Passage。
Macro.add("barStowedDemo", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () => {
    const english = I18n.locale === "en"
    return Surface.fragment(
      Surface.text(english ? "R" : "雨", { key: "bar-stowed-weather", color: 34 }),
      Surface.text(english ? "P" : "痛", {
        key: "bar-stowed-condition",
        styles: ["strong"],
        color: 34,
      }),
      Surface.text("!", { key: "bar-stowed-hint", styles: ["strong"], color: 8 }),
    )
  },
})

// 综合演示：region、component、image、语义字形与标准调色板。
Macro.add("surfaceDemo", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () =>
    Surface.fragment(
      Surface.region(
        "header",
        [
          Surface.text("Surface V2", {
            key: "demo-title",
            styles: ["strong"],
            // color 只决定文字颜色；16 是 Host 色阶中的橙色。
            color: 16,
          }),
        ],
        { key: "demo-header" },
      ),
      Surface.region(
        "bar",
        [
          Surface.text("测试工具", { key: "bar-heading", styles: ["strong"] }),
          Surface.component(
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
      Surface.text("强调文本。 ", { styles: ["emphasis"] }),
      Surface.text("重要文本。 ", { styles: ["strong"] }),
      Surface.text("const answer = 42", { styles: ["code"], color: 34 }),
      Surface.hardBreak(),
      Surface.text("新增内容。 ", { styles: ["inserted"], color: 32 }),
      Surface.text("删除内容。 ", { styles: ["deleted"], color: 8 }),
      // marked 自身表示高亮底色，不需要再叠加 color。
      Surface.text("需要留意。 ", { styles: ["marked"] }),
      Surface.text("危险状态。", { styles: ["strong"], color: 8 }),
      Surface.hardBreak(),
      Surface.image("images/loom.svg", {
        key: "loom-image",
        alt: "由经纬线组成的 Narrava Loom 示意图",
        caption: "图片由 Resource 逻辑路径加载。",
      }),
      Surface.component(
        "future-card",
        1,
        { title: "未知组件" },
        [Surface.text("Host 不认识该组件，因此显示这段 fallback。", { color: 3 })],
        { key: "fallback-demo" },
      ),
      Surface.region(
        "footer",
        [Surface.text("当前示例：语义渲染与 Resource", { key: "demo-footer", color: 3 })],
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
    Surface.fragment(
      Surface.region(
        "header",
        [Surface.text("Dialog 与按钮", { key: "dialog-page-title", styles: ["strong"] })],
        { key: "dialog-page-header" },
      ),
      Surface.text("进入本页时会打开语义 Dialog。关闭后仍可阅读正文并返回大厅。"),
      Surface.region(
        "dialog",
        [
          Surface.text("第一页", { key: "dialog-page-one", heading: 2 }),
          Surface.text("WebView 用页签切换；TUI 则把这一页单独成框，并把操作归到第一页。"),
          Surface.action("默认按钮", "dismiss", { key: "default-action", role: "default" }),
          Surface.action("主要按钮", "dismiss", { key: "primary-action", role: "primary" }),
          Surface.text("第二页", { key: "dialog-page-two", heading: 2 }),
          Surface.text("第二页独立展示次要与危险动作；TUI 操作编号仍跨页连续。"),
          Surface.action("次要按钮", "dismiss", {
            key: "secondary-action",
            role: "secondary",
          }),
          Surface.action("危险按钮", "dismiss", { key: "danger-action", role: "danger" }),
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
  inspectState,
  resourceSummary,
  returnToHall,
  emitQuestCompleted,
  emitReactionGoto,
  raiseReputation,
  readyEvents,
  difficulty: 3,
})

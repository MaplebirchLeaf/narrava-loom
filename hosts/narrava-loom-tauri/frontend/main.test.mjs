import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"

const source = await readFile(new URL("main.js", import.meta.url), "utf8")
const styles = await readFile(new URL("main.css", import.meta.url), "utf8")
const paletteFunctionStart = source.indexOf("function colorForPaletteIndex(index) {")
const paletteFunctionEnd = source.indexOf("\n}\n", paletteFunctionStart) + 2
assert.notEqual(paletteFunctionStart, -1, "Renderer 应定义 colorForPaletteIndex")
assert.notEqual(paletteFunctionEnd, 1, "colorForPaletteIndex 应保持可独立验证的函数边界")

const colorForPaletteIndex = Function(
  `"use strict"; return (${source.slice(paletteFunctionStart, paletteFunctionEnd)})`,
)()
const colors = Array.from({ length: 64 }, (_, color) => colorForPaletteIndex(color))

assert.equal(colors[0], "", "color 0 应继承正文颜色")
for (let color = 1; color <= 63; color += 1) {
  assert.match(colors[color], /^rgb\(\d+, \d+, \d+\)$/, `color ${color} 应映射为 RGB`)
}
assert.equal(new Set(colors).size, 64, "0..63 应产生 64 个互不混淆的结果")
assert.equal(colors[8], "rgb(255, 90, 90)")
assert.equal(colors[16], "rgb(255, 158, 69)")
assert.equal(colors[32], "rgb(82, 200, 120)")
assert.equal(colors[63], "rgb(88, 28, 135)")

assert.match(
  source,
  /node\.presentation === "panel"\s*\? `surface-container surface-panel\$\{flowClass\}`/,
  "Renderer 应把 Protocol panel 容器映射为稳定的 Host class",
)
assert.match(source, /node\.flow === "row"/, "Renderer 应只把显式 row 容器映射为同行布局")
assert.match(
  source,
  /unwrapPanelRows\(passage\)[\s\S]*reconcileSurfaceNodes\(passage,/,
  "正文 reconcile 前应先拆回旧 row 分组",
)
assert.match(
  source,
  /applySurfaceReplacements\(\)[\s\S]*wrapPanelRows\(passage\)/,
  "replace 完成后应把连续 row panel 建成独立块级组",
)
assert.match(
  source,
  /element\.matches\("\[data-surface-replace\]"\)/,
  "已经执行的隐藏 replace 命令不应打断相邻 row panel",
)
assert.match(
  styles,
  /\.surface-row\s*{[^}]*display:\s*flex[^}]*flex-wrap:\s*wrap[^}]*gap:\s*0\.5rem/s,
  "row panel 组应块级起行、紧凑排列并自然换行",
)
assert.match(
  styles,
  /\.passage-main\s*{[^}]*white-space:\s*normal/s,
  "Twee 源码排版换行不应打断相邻 row panel",
)
assert.match(
  styles,
  /\.surface-panel\s*{[^}]*margin-block:\s*0\.5rem/s,
  "panel 的上下留白应保持紧凑",
)

console.log("Narrava Tauri palette and panel mapping verified")

// 执行现有 Renderer 的 Image 分支，验证 Resource URL 与可变文字字段。
const updateStart = source.indexOf("function updateSurfaceElement(element, node) {")
const updateEnd = source.indexOf("\n}\n", updateStart) + 2
const resourcePaths = []
const updateImage = Function(
  "urlForResourcePath",
  `return (${source.slice(updateStart, updateEnd)})`,
)((path) => {
  resourcePaths.push(path)
  return `narrava-resource://localhost/${path}`
})
const imageElement = {}
const figure = {
  querySelector: () => imageElement,
}
updateImage(figure, {
  type: "image",
  resource: "images/forest.png",
  alt: "森林",
})
assert.deepEqual(resourcePaths, ["images/forest.png"])
assert.equal(imageElement.src, "narrava-resource://localhost/images/forest.png")
assert.equal(imageElement.alt, "森林")
updateImage(figure, { type: "image", resource: "images/forest.png", alt: "" })
assert.equal(imageElement.alt, "")

const numberStart = source.indexOf("function finiteNumberOrFallback(")
const numberEnd = source.indexOf("\n}", numberStart) + 2
const updateMeter = Function(
  "finiteNumberOrFallback",
  `return (${source.slice(updateStart, updateEnd)})`,
)(Function(`return (${source.slice(numberStart, numberEnd)})`)())
const meterLabel = {}
const meterElement = {
  setAttribute(name, value) {
    this[name] = value
  },
}
const meterContainer = {
  dataset: {},
  querySelector: (selector) => (selector === "span" ? meterLabel : meterElement),
}
updateMeter(meterContainer, {
  type: "component",
  capability: "meter",
  version: 1,
  properties: { label: "体力", value: 72, min: 0, max: 100 },
})
assert.equal(meterLabel.textContent, "体力")
assert.equal(meterElement.value, 72)
assert.equal(meterElement.min, 0)
assert.equal(meterElement.max, 100)
assert.equal(meterElement["aria-label"], "体力")

// 执行 Dialog 的 Host 状态转换；页面内容继续交给已有 keyed renderer。
class DialogElement {
  dataset = {}
  children = []
  attributes = new Map()
  listeners = new Map()
  classList = { toggle() {} }
  setAttribute(name, value) {
    this.attributes.set(name, value)
  }
  addEventListener(name, handler) {
    this.listeners.set(name, handler)
  }
  replaceChildren(...nodes) {
    this.children = nodes
  }
  append(node) {
    this.children.push(node)
  }
  focus() {
    this.focused = true
  }
}
const dialogSurface = new DialogElement()
const dialogTabs = new DialogElement()
const dialogMessage = { hidden: true }
const dialog = {
  open: false,
  showModal() {
    this.open = true
  },
  close() {
    this.open = false
  },
}
const dialogStart = source.indexOf("function renderDialog(node, fallback) {")
const dialogEnd = source.indexOf("/** 重绘前拆回 Host panel", dialogStart)
const renderDialog = Function(
  "document",
  "dialogSurface",
  "dialogTabs",
  "dialogMessage",
  "dialog",
  "reconcileSurfaceNodes",
  `${source.slice(dialogStart, dialogEnd)}; return renderDialog`,
)(
  { createElement: () => new DialogElement() },
  dialogSurface,
  dialogTabs,
  dialogMessage,
  dialog,
  (panel, nodes) => {
    panel.nodes = nodes
  },
)
const dialogNode = {
  key: "open:1",
  initial: "装备",
  pages: ["属性", "装备", "经历", "说明"].map((title) => ({
    title,
    nodes: [{ type: "styledText", heading: 2, text: "普通标题" }],
  })),
}
renderDialog(dialogNode, [])
assert.equal(dialog.open, true)
assert.equal(dialogTabs.children.length, 4, "普通标题不增加页面")
assert.equal(dialogSurface.dataset.selectedPage, "装备")
assert.equal(dialogSurface.children[1].hidden, false)
dialogTabs.children[2].listeners.get("click")()
const retainedPanel = dialogSurface.children[2]
renderDialog(dialogNode, [])
assert.equal(dialogSurface.dataset.selectedPage, "经历", "同一弹窗刷新保留选中页")
assert.equal(dialogSurface.children[2], retainedPanel, "同页复用内容容器")
dialogTabs.children[2].listeners.get("keydown")({ key: "End", preventDefault() {} })
assert.equal(dialogSurface.dataset.selectedPage, "说明")
assert.equal(dialogTabs.children[3].focused, true)
dialog.close()
renderDialog(dialogNode, [])
assert.equal(dialog.open, false, "普通重绘不能重新打开已关闭弹窗")
renderDialog({ ...dialogNode, key: "open:2" }, [])
assert.equal(dialog.open, true)
assert.equal(dialogSurface.dataset.selectedPage, "装备", "重新打开恢复作者默认页")
renderDialog({ key: "single", initial: "提示", pages: [{ title: "提示", nodes: [] }] }, [])
assert.equal(dialogTabs.children.length, 1)
renderDialog(null, [])
assert.equal(dialog.open, false)
console.log("Narrava Tauri explicit dialog pages, keyboard, refresh and reopen verified")

function rendererFunction(name) {
  const start = source.indexOf(`function ${name}(`)
  assert.notEqual(start, -1, `Renderer 应定义 ${name}`)
  const asyncStart = source.slice(start - 6, start) === "async " ? start - 6 : start
  return source.slice(asyncStart, source.indexOf("\n}", start) + 2)
}

const createRuntimeDispatcher = Function(
  `return (${rendererFunction("createRuntimeDispatcher")})`,
)()

// 执行真实队列：后续操作等待前一条完成，错误也不能阻塞未来命令。
const calls = []
const busy = []
const frames = []
let finishFirst
const dispatch = createRuntimeDispatcher(
  (command) => {
    calls.push(command)
    if (command === "input")
      return new Promise((resolve) => {
        finishFirst = resolve
      })
    if (command === "bad") return Promise.reject(new Error("rejected"))
    return Promise.resolve(command === "save_game" ? null : { current: command })
  },
  (frame) => frames.push(frame),
  (active) => busy.push(active),
)
const first = dispatch("input")
const next = dispatch("select_language")
await Promise.resolve()
assert.deepEqual(calls, ["input"], "输入未完成时不能交错另一个 Runtime 命令")
finishFirst({ current: "input-result" })
await Promise.all([first, next])
assert.deepEqual(
  frames.map((frame) => frame.current),
  ["input-result", "select_language"],
)
assert.deepEqual(busy, [true, true, true, false], "队列清空前保持忙碌")
const failed = dispatch("bad")
const recovered = dispatch("save_game")
await assert.rejects(failed, /rejected/)
assert.equal(await recovered, null, "Applied 的空帧合法且不重绘")
assert.equal(frames.length, 2)

// Renderer 失败发生在 Runtime 已提交之后，不能当作拒绝输入回滚控件。
let rolledBack = false
const renderFailure = createRuntimeDispatcher(
  () => Promise.resolve({ current: "committed" }),
  () => {
    throw new Error("render failed")
  },
  () => {},
)
await assert.rejects(
  renderFailure(
    "input",
    {},
    {
      rollback: () => {
        rolledBack = true
      },
    },
  ),
  /render failed/,
)
assert.equal(rolledBack, false)

class InputElement {
  dataset = {}
  listeners = new Map()
  isConnected = true
  checked = false
  value = ""
  addEventListener(name, handler) {
    this.listeners.set(name, handler)
  }
}

function inputHarness(invokeCommand, render) {
  const controls = []
  const errors = []
  const rendered = []
  const commands = []
  const run = createRuntimeDispatcher(
    (command, args) => {
      commands.push([command, args])
      return invokeCommand(command, args)
    },
    (frame) => {
      rendered.push(frame)
      render?.(frame)
    },
    () => {},
  )
  const create = Function(
    "document",
    "story",
    "CSS",
    "runRuntimeCommand",
    "showHostError",
    `${rendererFunction("createSurfaceElement")}\n${rendererFunction("surfaceElementType")}\n${rendererFunction("submitInputValue")}\nreturn createSurfaceElement`,
  )(
    { createElement: () => new InputElement() },
    { querySelectorAll: () => controls.filter((control) => control.type === "radio") },
    { escape: (value) => value },
    run,
    (error) => errors.push(error),
  )
  return {
    errors,
    rendered,
    commands,
    add(node) {
      const element = create(node)
      updateImage(element, node)
      controls.push(element)
      return element
    },
  }
}

// 新帧能在同一 DOM 控件上改变状态；不能再用旧提交覆盖其 committed 值。
let radio
const radioHarness = inputHarness(
  () => Promise.resolve({ current: "Start" }),
  () => {
    radio.checked = false
    radio.dataset.committedChecked = "false"
  },
)
radio = radioHarness.add({
  type: "radiobutton",
  key: "radio",
  id: "radio:1",
  group: "mode",
  value: "one",
  selected: false,
})
radio.checked = true
await radio.listeners.get("change")()
assert.equal(radioHarness.rendered.length, 1)
assert.equal(radio.checked, false)
assert.equal(radio.dataset.committedChecked, "false")

// 导航帧移除原控件后，不能向旧节点写回本地提交状态。
let textbox
const navigationHarness = inputHarness(
  () => Promise.resolve({ current: "Target" }),
  () => {
    textbox.isConnected = false
  },
)
textbox = navigationHarness.add({ type: "textbox", key: "name", id: "name:1", value: "before" })
textbox.value = "after"
await textbox.listeners.get("change")()
assert.equal(navigationHarness.rendered[0].current, "Target")
assert.equal(textbox.dataset.committedValue, "before")

// 无帧提交保留输入，拒绝后恢复上次成功值，随后仍然能提交。
let reject = false
const localHarness = inputHarness(() =>
  reject ? Promise.reject(new Error("invalid")) : Promise.resolve(null),
)
const localInput = localHarness.add({
  type: "textbox",
  key: "local",
  id: "local:1",
  value: "before",
})
localInput.value = "saved"
await localInput.listeners.get("change")()
assert.equal(localInput.dataset.committedValue, "saved")
reject = true
localInput.value = "invalid"
await localInput.listeners.get("change")()
assert.equal(localInput.value, "saved")
assert.equal(localHarness.errors.length, 1)
reject = false
localInput.value = "retry"
await localInput.listeners.get("change")()
assert.equal(localInput.dataset.committedValue, "retry")

// 排队期间被导航移除的第二个控件不应再把旧交互身份提交给 Worker。
let completeNavigation
let staleInput
const staleHarness = inputHarness(
  () =>
    new Promise((resolve) => {
      completeNavigation = resolve
    }),
  () => {
    staleInput.isConnected = false
  },
)
const trigger = staleHarness.add({
  type: "checkbox",
  key: "go",
  id: "go:1",
  checked: true,
  unchecked: false,
  selected: false,
})
staleInput = staleHarness.add({ type: "textbox", key: "stale", id: "stale:1", value: "before" })
trigger.checked = true
const pendingNavigation = trigger.listeners.get("change")()
staleInput.value = "too late"
const pendingInput = staleInput.listeners.get("change")()
await Promise.resolve()
completeNavigation({ current: "Target" })
await Promise.all([pendingNavigation, pendingInput])
assert.equal(staleHarness.commands.length, 1)
assert.equal(staleHarness.errors.length, 0)
console.log(
  "Narrava Tauri input frames, serial commands, stale controls and failure recovery verified",
)

const errorStory = {
  setAttribute(name, value) {
    this[name] = value
  },
}
const showHostError = Function(
  "document",
  "dialogSurface",
  "dialogTabs",
  "dialogMessage",
  "dialog",
  "status",
  "story",
  "runtimeBusy",
  `return (${rendererFunction("showHostError")})`,
)(
  { createElement: () => new DialogElement() },
  dialogSurface,
  dialogTabs,
  dialogMessage,
  dialog,
  {},
  errorStory,
  true,
)
showHostError({
  code: "script.error",
  message: "失败",
  location: { source: "scripts/main.ts", line: 12, column: 8, generated: true },
})
assert.equal(dialogMessage.textContent, "scripts/main.ts:12:8（生成位置）\nscript.error：失败")
assert.equal(errorStory["aria-busy"], "true", "错误提示不能解除其他排队命令的忙碌状态")
showHostError({ code: "input.invalid", message: "值不合法" })
assert.equal(dialogMessage.textContent, "input.invalid：值不合法")

// 真实控制台模块：命令进入主队列，显示纯文本，支持历史并保留启动来源。
const consoleSource = (await readFile(new URL("console.js", import.meta.url), "utf8")).replace(
  /^import .*\n/gm,
  "",
)
const viewSource = await readFile(new URL("console-view.js", import.meta.url), "utf8")
const completionSource = await readFile(new URL("console-completion.js", import.meta.url), "utf8")
const renderConsoleValue = Function(
  `${viewSource.replace("export function", "function")}; return renderConsoleValue`,
)()
const createConsoleCompletion = Function(
  `${completionSource.replace("export function", "function")}; return createConsoleCompletion`,
)()
class ConsoleElement extends DialogElement {
  append(...nodes) {
    for (const node of nodes) {
      if (node.parentElement)
        node.parentElement.children = node.parentElement.children.filter((child) => child !== node)
      super.append(node)
      node.parentElement = this
    }
  }
  removeAttribute(name) {
    this.attributes.delete(name)
  }
  scrollIntoView() {}
  hidden = true
  value = ""
  setSelectionRange(start, end) {
    this.selectionStart = start
    this.selectionEnd = end
  }
}
const consoleElements = new Map(
  [
    "debug-open",
    "debug-console",
    "debug-status",
    "debug-content",
    "debug-input",
    "debug-refresh",
    "debug-clear",
    "debug-close",
    "debug-cancel",
    "debug-help",
    "debug-candidates",
    "debug-examples",
  ].map((id) => [id, new ConsoleElement()]),
)
const consoleHome = new ConsoleElement()
consoleHome.append(consoleElements.get("debug-console"))
const consoleDialog = new ConsoleElement()
consoleElements.set("nv-dialog", consoleDialog)
let observeDialog
const consoleCommands = []
const executed = []
let consoleFailure = false
let releaseCommand
const developerConsole = Function(
  "window",
  "document",
  "MutationObserver",
  "renderConsoleValue",
  "createConsoleCompletion",
  `${consoleSource}; return {refresh, execute, setOpen, clear}`,
)(
  {
    __TAURI__: {
      core: {
        invoke: async (command) => {
          consoleCommands.push(command)
          if (command === "developer_enabled") return true
          assert.equal(command, "debug_snapshot")
          if (consoleFailure)
            throw {
              code: "script.startup",
              message: "broken",
              location: { source: "scripts/main.ts", line: 8, column: 3, generated: true },
            }
          return {
            current: "Start",
            logs: [{ sequence: 1, level: "info", target: "test", message: "<img onerror=bad>" }],
          }
        },
      },
    },
    narravaDebug: {
      execute: (command) => {
        executed.push(command)
        return new Promise((resolve) => {
          releaseCommand = resolve
        })
      },
    },
    addEventListener() {},
  },
  {
    getElementById: (id) => consoleElements.get(id),
    createElement: () => new ConsoleElement(),
    addEventListener() {},
  },
  class {
    constructor(callback) {
      observeDialog = callback
    }
    observe() {}
  },
  renderConsoleValue,
  createConsoleCompletion,
)
await Promise.resolve()
assert.equal(consoleElements.get("debug-open").hidden, false)
await developerConsole.refresh()
const consoleOutput = consoleElements.get("debug-content")
assert.equal(consoleOutput.children[0].textContent, "[info test] <img onerror=bad>")
await developerConsole.refresh()
assert.equal(consoleOutput.children.length, 1, "读取不重复已有日志")
developerConsole.setOpen(true)
consoleDialog.open = true
observeDialog()
assert.equal(
  consoleElements.get("debug-console").parentElement,
  consoleDialog,
  "模态框内仍可使用控制台",
)
consoleDialog.open = false
observeDialog()
assert.equal(consoleElements.get("debug-console").parentElement, consoleHome)
const consoleInput = consoleElements.get("debug-input")
assert.equal(consoleInput.focused, true)
consoleInput.value = "V.name = 'changed'"
const command = developerConsole.execute()
assert.equal(consoleInput.readOnly, true)
await developerConsole.execute()
assert.equal(executed.length, 1, "不能重复提交忙碌命令")
releaseCommand()
await command
assert.equal(consoleInput.readOnly, false)
const consoleKey = (key) => consoleInput.listeners.get("keydown")({ key, preventDefault() {} })
consoleInput.value = "draft"
consoleKey("ArrowUp")
assert.equal(consoleInput.value, "V.name = 'changed'")
consoleKey("ArrowDown")
assert.equal(consoleInput.value, "draft")
developerConsole.clear()
await developerConsole.refresh()
assert.equal(consoleOutput.children.length, 0, "清屏后不重新展示旧日志")
consoleFailure = true
await developerConsole.refresh()
assert.match(consoleOutput.children[0].textContent, /scripts\/main.ts:8:3.*Generated/)
assert.deepEqual(executed, ["V.name = 'changed'"])
console.log(
  "Narrava compact script console, history, log deduplication and startup diagnostics verified",
)

// 对象树保留类型和结构，帮助、成员路径与危险字符串均按文本显示。
const treeDocument = {
  createElement(tag) {
    const element = new ConsoleElement()
    element.tagName = tag.toUpperCase()
    return element
  },
}
let usedPath
const valueTree = renderConsoleValue(
  treeDocument,
  {
    name: "Save",
    kind: "object",
    preview: "Save {1}",
    signature: "NarravaSave",
    help: "保存 / Save",
    truncated: false,
    children: [
      {
        name: "export",
        kind: "function",
        preview: "ƒ export",
        signature: "(target?: string) => void",
        help: "导出 / Export",
        children: [],
        truncated: false,
      },
    ],
  },
  (path) => {
    usedPath = path
  },
)
assert.equal(valueTree.tagName, "DETAILS")
assert.equal(valueTree.open, true)
const exportTree = valueTree.children.at(-1)
exportTree.children.at(-1).listeners.get("click")()
assert.equal(usedPath, "Save.export")

// 执行真实补全模块，模拟乱序响应；只发送路径，不发送可执行脚本。
const completionInput = new ConsoleElement()
const candidateList = new ConsoleElement()
candidateList.ownerDocument = treeDocument
const completionHelp = new ConsoleElement()
const pendingCompletions = []
const completer = createConsoleCompletion(
  completionInput,
  candidateList,
  completionHelp,
  (ipcCommand, args) => {
    assert.equal(ipcCommand, "debug_complete")
    return new Promise((resolve) => pendingCompletions.push({ path: args.path, resolve }))
  },
)
completionInput.value = "Save.e"
const outdated = completer.update()
completionInput.value = "Engine.g"
const current = completer.update()
pendingCompletions[1].resolve([
  {
    name: "goto",
    kind: "function",
    signature: "(target: string) => void",
    help: "前往 / Navigate",
  },
])
await current
pendingCompletions[0].resolve([{ name: "export", kind: "function", signature: "", help: "" }])
await outdated
assert.equal(candidateList.children[0].textContent, "goto(…)")
assert.equal(completer.key({ key: "Tab", preventDefault() {} }), true)
assert.equal(completionInput.value, "Engine.goto(")
pendingCompletions[2].resolve([
  {
    name: "goto",
    kind: "function",
    signature: "(target: string) => void",
    help: "前往 / Navigate",
  },
])
await Promise.resolve()
assert.match(completionHelp.textContent, /target: string/)
assert.deepEqual(
  pendingCompletions.map((request) => request.path),
  ["Save", "Engine", "Engine"],
)
console.log(
  "Narrava object trees, member paths, stale completion responses and parameter hints verified",
)

const consoleHtml = await readFile(new URL("index.html", import.meta.url), "utf8")
for (const id of ["debug-help", "debug-candidates", "debug-examples", "debug-cancel"])
  assert.ok(consoleHtml.includes(`id="${id}"`), `Console DOM missing ${id}`)

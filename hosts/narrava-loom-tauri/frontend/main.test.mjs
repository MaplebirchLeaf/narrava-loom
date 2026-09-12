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

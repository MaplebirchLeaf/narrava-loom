import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"

const source = await readFile(new URL("main.js", import.meta.url), "utf8")
const styles = await readFile(new URL("main.css", import.meta.url), "utf8")
const paletteFunctionStart = source.indexOf("function colorForPaletteIndex(index) {")
const paletteFunctionEnd = source.indexOf("\n}\n\n/** 重绘前", paletteFunctionStart) + 2
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

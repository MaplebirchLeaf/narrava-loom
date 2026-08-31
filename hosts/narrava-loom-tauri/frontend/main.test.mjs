import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"

const source = await readFile(new URL("main.js", import.meta.url), "utf8")
const styles = await readFile(new URL("main.css", import.meta.url), "utf8")
const start = source.indexOf("function paletteColor(index) {")
const end = source.indexOf("\n}\n\n/** 重绘前", start) + 2
assert.notEqual(start, -1, "Renderer 应定义 paletteColor")
assert.notEqual(end, 1, "paletteColor 应保持可独立验证的函数边界")

const paletteColor = Function(`"use strict"; return (${source.slice(start, end)})`)()
const colors = Array.from({ length: 64 }, (_, color) => paletteColor(color))

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
  styles,
  /\.surface-panel\.surface-flow-row\s*{[^}]*display:\s*inline-block/s,
  "row panel 应成为可连续同行并自然换行的 inline block",
)
assert.match(
  styles,
  /\.surface-panel\.surface-flow-row\s*\+\s*\.surface-panel\.surface-flow-row\s*{[^}]*margin-inline-start:/s,
  "相邻 row panel 之间应保留清晰间距",
)
assert.match(
  styles,
  /\.surface-panel\.surface-flow-row\s*\+\s*:not\(\.surface-panel\.surface-flow-row\)\s*{[^}]*display:\s*block/s,
  "row panel 组后的普通正文应从下一行开始",
)

console.log("Narrava Tauri palette and panel mapping verified")

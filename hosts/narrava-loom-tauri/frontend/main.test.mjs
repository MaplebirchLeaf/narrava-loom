import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"

const source = await readFile(new URL("main.js", import.meta.url), "utf8")
const start = source.indexOf("function toneColor(index) {")
const end = source.indexOf("\n}\n\n/** 重绘前", start) + 2
assert.notEqual(start, -1, "Renderer 应定义 toneColor")
assert.notEqual(end, 1, "toneColor 应保持可独立验证的函数边界")

const toneColor = Function(`"use strict"; return (${source.slice(start, end)})`)()
const colors = Array.from({ length: 64 }, (_, tone) => toneColor(tone))

assert.equal(colors[0], "", "tone 0 应继承正文颜色")
for (let tone = 1; tone <= 63; tone += 1) {
  assert.match(colors[tone], /^rgb\(\d+, \d+, \d+\)$/, `tone ${tone} 应映射为 RGB`)
}
assert.equal(new Set(colors).size, 64, "0..63 应产生 64 个互不混淆的结果")
assert.equal(colors[8], "rgb(255, 90, 90)")
assert.equal(colors[16], "rgb(255, 158, 69)")
assert.equal(colors[32], "rgb(82, 200, 120)")
assert.equal(colors[63], "rgb(88, 28, 135)")

console.log("Narrava Tauri 64-tone palette verified")

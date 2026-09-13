import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import { runInNewContext } from "node:vm"

const require = createRequire(import.meta.url)
const { macroKinds } = require("../src/catalog")
const listeners = {}
const context = { subscriptions: [] }
let enabled = true
let style
function Range(start, end) {
  this.start = start
  this.end = end
}
function listen(name) {
  return (callback) => {
    listeners[name] = callback
    return { dispose: () => delete listeners[name] }
  }
}
let text = ":: Start\n<<if $outer>>\n<<if $inner>>内部<</if>>\n<<else>>外部<</if>>\n结束"
let scans = 0
const document = {
  languageId: "narrava-twee",
  uri: { scheme: "file" },
  version: 1,
  getText: () => {
    scans += 1
    return text
  },
  offsetAt: (position) => position,
  positionAt: (offset) => offset,
}
const editor = {
  document,
  selections: [{ active: text.indexOf("内部") }],
  setDecorations: (_type, ranges) => {
    editor.ranges = Array.from(ranges)
  },
}
const vscode = {
  Range,
  DecorationRangeBehavior: { ClosedClosed: 1 },
  window: {
    visibleTextEditors: [editor],
    createTextEditorDecorationType: (options) => {
      style = options
      return { dispose() {} }
    },
    onDidChangeTextEditorSelection: listen("selection"),
    onDidChangeActiveTextEditor: listen("active"),
    onDidChangeVisibleTextEditors: listen("visible"),
  },
  workspace: {
    getConfiguration: () => ({ get: () => enabled }),
    onDidChangeTextDocument: listen("text"),
    onDidChangeConfiguration: listen("configuration"),
  },
}
const workspace = { kinds: macroKinds([]), emitter: { event: listen("definitions") } }
const module = { exports: {} }
runInNewContext(readFileSync(new URL("../src/macro-highlights.js", import.meta.url), "utf8"), {
  require: (name) => (name === "vscode" ? vscode : require(`../src/${name}`)),
  module,
})
module.exports.registerMacroHighlights(context, workspace)
const highlighted = () => editor.ranges.map(({ start, end }) => text.slice(start, end))
assert.equal(style.fontWeight, "bold")
assert.equal(style.textDecoration, "underline")
assert.equal(style.color, undefined)
assert.deepEqual(highlighted(), ["<<if", ">>", "<</if>>"])
assert.equal(editor.ranges[0].start, text.indexOf("<<if $inner"))
editor.selections = [{ active: text.indexOf("外部") }]
listeners.selection({ textEditor: editor })
assert.equal(editor.ranges[0].start, text.indexOf("<<if $outer"))
assert.equal(scans, 1, "cursor movement must reuse the document scan")
editor.selections.push({ active: text.indexOf("内部") }, { active: text.indexOf("内部") })
listeners.selection({ textEditor: editor })
assert.equal(editor.ranges.length, 6, "multi-cursor matches are deduplicated")
editor.selections = [{ active: text.indexOf("结束") }]
listeners.selection({ textEditor: editor })
assert.deepEqual(highlighted(), [])
editor.selections = [{ active: text.indexOf("内部") }]
listeners.active(editor)
assert.equal(editor.ranges.length, 3)
enabled = false
listeners.configuration({ affectsConfiguration: () => true })
assert.deepEqual(highlighted(), [])
enabled = true
listeners.configuration({ affectsConfiguration: () => true })
assert.equal(editor.ranges.length, 3)
document.version += 1
listeners.text({ document })
assert.equal(scans, 2)
workspace.kinds = macroKinds([])
listeners.definitions()
assert.equal(scans, 3, "new macro definitions invalidate the pair cache")
text = text.replaceAll("<</if>>", "")
document.version += 1
listeners.text({ document })
assert.deepEqual(highlighted(), [], "deleting closing tags clears stale decorations")
text = "<<if true>>新内容<</if>>"
document.version += 1
editor.selections = [{ active: text.indexOf("新内容") }]
listeners.visible([editor])
assert.deepEqual(highlighted(), ["<<if", ">>", "<</if>>"])
document.languageId = "javascript"
listeners.active(editor)
assert.deepEqual(highlighted(), [])
for (const subscription of context.subscriptions) subscription.dispose()
assert.deepEqual(Object.keys(listeners), [])
console.log(
  "Active macro decorations, nested pairs, multi-cursor updates, cache and cleanup verified",
)

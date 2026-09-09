import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { createRequire } from "node:module"
import { fileURLToPath } from "node:url"
import { runInNewContext } from "node:vm"

const require = createRequire(new URL("../src/providers.js", import.meta.url))
const { BUILTIN_MACROS, knownNames } = require("./catalog")
const { MACRO_APIS } = require("./macro-api")
class MarkdownString {
  value = ""
  appendCodeblock(code) {
    this.value += code + "\n"
  }
  appendMarkdown(text) {
    this.value += text
  }
}
function Hover(contents, range) {
  this.contents = contents
  this.range = range
}
function Range(start, end) {
  this.start = start
  this.end = end
}
function CompletionItem(label) {
  this.label = label
}
const vscode = {
  MarkdownString,
  Hover,
  Range,
  CompletionItem,
  CompletionItemKind: { Function: 1 },
  SemanticTokensLegend: function () {},
  Uri: { file: (path) => path },
}
const module = { exports: {} }
runInNewContext(readFileSync(new URL("../src/providers.js", import.meta.url), "utf8"), {
  require: (name) => (name === "vscode" ? vscode : require(name)),
  module,
  __dirname: fileURLToPath(new URL("../src", import.meta.url)),
})
const { hoverProvider, completionProvider } = module.exports
const document = (text) => ({
  getText: () => text,
  offsetAt: (position) => position,
  positionAt: (offset) => offset,
})
for (const name of BUILTIN_MACROS) {
  const text = `<<${name}>>`
  const hover = hoverProvider().provideHover(document(text), 2)
  assert.ok(hover.contents.value.includes(MACRO_APIS[name].signature), name)
  assert.ok(hover.contents.value.includes(MACRO_APIS[name].description), name)
  assert.equal(hover.range.start, 2)
  assert.equal(hover.range.end, 2 + name.length)
  assert.equal(hoverProvider().provideHover(document(text), 2 + name.length), undefined)
}
assert.ok(BUILTIN_MACROS.includes("image"))
assert.ok(hoverProvider().provideHover(document("<</if>>"), 3))
for (const text of ["<<custom>>", "/% <<print>> %/"]) {
  assert.equal(
    hoverProvider().provideHover(
      document(text),
      text.indexOf("print") >= 0 ? text.indexOf("print") : 2,
    ),
    undefined,
  )
}
const expression = hoverProvider().provideHover(document("<<print abs(-1)>>"), 8)
assert.ok(expression.contents.value.includes("Expression API"))
const items = completionProvider({
  known: new Set([...BUILTIN_MACROS, "custom"]),
}).provideCompletionItems()
assert.ok(items.find((item) => item.label === "image").documentation.value.includes("TUI"))
assert.equal(items.find((item) => item.label === "custom").documentation, undefined)
assert.ok(knownNames([]).has("image"))
console.log("Narrava native macro hover and completion verified")

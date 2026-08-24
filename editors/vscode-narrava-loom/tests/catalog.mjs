import assert from "node:assert/strict"
import { createRequire } from "node:module"

const require = createRequire(import.meta.url)
const { BUILTIN_MACROS, knownNames, macroKinds, scanScript, scanTwee } = require("../src/catalog")

const first = scanTwee(`
/% <<widget "ignored">> <<ignored>> %/
<<widget "crossFileCard">>
<<print _args[0]>><</print>>
<</widget>>
`)
const second = scanTwee(`<<crossFileCard "跨文件">>\n<<missing>>`)
const scripts = scanScript(`
Macro.add("scriptCard", () => {})
Macro.update('updatedCard', {})
`)

assert.deepEqual(first.definitions.map(item => item.name), ["crossFileCard"])
assert.equal(first.definitions[0].bodyKind, "inline")
assert.ok(first.calls.some(call => call.name === "widget" && !call.closing))
assert.ok(first.calls.some(call => call.name === "widget" && call.closing))
assert.ok(second.calls.some(call => call.name === "crossFileCard"))
assert.deepEqual(scripts.map(item => item.name), ["scriptCard", "updatedCard"])

const known = knownNames([...first.definitions, ...scripts])
for (const name of ["if", "link", "widget", "crossFileCard", "scriptCard", "updatedCard"]) {
  assert.ok(known.has(name), `${name} should be known`)
}
assert.equal(known.has("missing"), false)
assert.ok(BUILTIN_MACROS.length > 10)
assert.equal(macroKinds(first.definitions).get("crossFileCard"), "inline")
assert.equal(macroKinds([]).get("print"), "inline")
assert.equal(macroKinds([]).get("link"), "container")
assert.equal(macroKinds([]).get("button"), "container")
assert.equal(macroKinds([]).get("replace"), "container")
assert.equal(macroKinds([]).get("slot"), "container")
assert.equal(macroKinds([]).get("checkbox"), "inline")
assert.equal(macroKinds([]).get("radiobutton"), "inline")
assert.equal(macroKinds([]).get("textbox"), "inline")

console.log("Narrava Twee cross-file Macro catalog verified")

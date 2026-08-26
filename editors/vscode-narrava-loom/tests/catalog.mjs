import assert from "node:assert/strict"
import { createRequire } from "node:module"

const require = createRequire(import.meta.url)
const {
  BUILTIN_MACROS,
  SPECIAL_PASSAGES,
  missingPassageLinks,
  taggedSpecialPassages,
  knownNames,
  macroKinds,
  scanScript,
  scanScriptFunctions,
  scanTwee,
} = require("../src/catalog")

const first = scanTwee(`
/% <<widget "ignored">> <<ignored>> %/
<<widget "crossFileCard">>
<<print _args[0]>><</print>>
<</widget>>
`)
const second = scanTwee(`<<crossFileCard "跨文件">>\n<<missing>>`)
const story = scanTwee(`
:: StoryInit
:: Start [opening]
<<link [[进入大厅|Hall]]>><</link>>
:: Hall [hub]
`)
const scripts = scanScript(`
Macro.add("scriptCard", () => {})
Macro.update('updatedCard', {})
`)
const scriptFunctions = scanScriptFunctions(`
export function scriptedGreeting(name: string): string { return name }
const resourceSummary = (): string => "ok"
async function loadGuide() {}
`)
const expressionCalls = scanTwee(`
<<print scriptedGreeting($hero.profile.name)>>
<<run Object.assign($hero.profile, { ready: true })>>
`)

assert.deepEqual(
  first.definitions.map((item) => item.name),
  ["crossFileCard"],
)
assert.equal(first.definitions[0].bodyKind, "inline")
assert.ok(first.calls.some((call) => call.name === "widget" && !call.closing))
assert.ok(first.calls.some((call) => call.name === "widget" && call.closing))
assert.ok(second.calls.some((call) => call.name === "crossFileCard"))
assert.deepEqual(
  scripts.map((item) => item.name),
  ["scriptCard", "updatedCard"],
)
assert.deepEqual(
  scriptFunctions.map((item) => item.name),
  ["scriptedGreeting", "resourceSummary", "loadGuide"],
)
assert.deepEqual(
  expressionCalls.functionCalls.map((item) => item.name),
  ["scriptedGreeting", "Object.assign"],
)
assert.equal(expressionCalls.functionCalls[0].length, "scriptedGreeting".length)

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
assert.deepEqual(SPECIAL_PASSAGES, ["Start", "StoryInit", "Header", "Footer", "Bar", "BarStowed"])
assert.deepEqual(
  story.passages.map((item) => item.name),
  ["StoryInit", "Start", "Hall"],
)
assert.equal(story.passages[0].special, true)
assert.deepEqual(story.passages[0].tags, [])
assert.deepEqual(story.passages[1].tags, ["opening"])
assert.equal(story.passages[2].special, false)
assert.deepEqual(
  taggedSpecialPassages(story.passages).map((item) => item.name),
  ["Start"],
)
assert.deepEqual(
  story.links.map((item) => item.target),
  ["Hall"],
)
assert.deepEqual(missingPassageLinks(story.links, story.passages), [])
assert.deepEqual(
  missingPassageLinks(scanTwee("<<link [[迷路|Missing]]>><</link>>").links, story.passages).map(
    (item) => item.target,
  ),
  ["Missing"],
)
assert.equal(
  `\n:: StoryInit\n:: Start [opening]\n<<link [[进入大厅|Hall]]>><</link>>\n:: Hall [hub]\n`.slice(
    story.links[0].start,
    story.links[0].start + story.links[0].length,
  ),
  "Hall",
)

console.log("Narrava Twee cross-file Macro and Passage catalog verified")

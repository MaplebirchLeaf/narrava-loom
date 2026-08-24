import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"

const root = new URL("../", import.meta.url)
const repository = new URL("../../../", import.meta.url)
const manifest = JSON.parse(await readFile(new URL("package.json", root), "utf8"))
const grammar = JSON.parse(await readFile(new URL("syntaxes/narrava-twee.tmLanguage.json", root), "utf8"))
const language = JSON.parse(await readFile(new URL("language-configuration.json", root), "utf8"))
const reference = await readFile(new URL("docs/reference/api-and-syntax.md", repository), "utf8")
const hooks = await readFile(new URL("src/macro_runtime/hooks.rs", repository), "utf8")
const evaluator = await readFile(new URL("src/expression/evaluator/chain.rs", repository), "utf8")
const gallery = await readFile(new URL("examples/contents/story/main.twee", repository), "utf8")
const sharedWidgets = await readFile(new URL("examples/contents/story/widgets.twee", repository), "utf8")

assert.equal(manifest.engines.vscode.startsWith("^"), true)
assert.equal(manifest.main, "./extension.js")
assert.deepEqual(manifest.contributes.languages[0].extensions, [".twee"])
assert.equal(manifest.contributes.grammars[0].scopeName, "source.narrava-twee")
assert.equal(grammar.scopeName, "source.narrava-twee")
assert.equal(language.comments.blockComment[0], "/%")
assert.equal(language.comments.blockComment[1], "%/")
assert.equal(JSON.stringify(grammar.repository.comment).includes("<!--"), false)

const serialized = JSON.stringify(grammar)
for (const token of [
  "passage",
  "if",
  "switch",
  "widget",
  "include",
  "goto",
  "print",
  "silently",
  "variable",
  "html",
  "entity.other.attribute-name.html",
  "setup",
  "keyword.operator",
  "string.other.link.label",
  "support.function",
  "keyword.operator.link",
  "entity.name.function.passage",
  "variable.other.readwrite",
  "string.quoted.other.template",
  "meta.interpolation",
  "punctuation.definition.template-expression.begin.narrava-twee",
  "punctuation.definition.template-expression.end.narrava-twee",
  "punctuation.definition.tag.begin.narrava-twee",
  "punctuation.definition.tag.end.narrava-twee",
  "support.type.passage-tag",
]) {
  assert.ok(serialized.includes(token), `grammar should cover ${token}`)
}

for (const sample of [
  "/%",
  ":: HighlightGallery [reference exit]",
  "$hero.profile.name",
  "_counter",
  "@choice.label",
  "setup.build.channel",
  "defined($hero.profile.name)",
  "scriptedGreeting($hero.profile.name)",
  "<<link [[进入大厅|Hall]]>><</link>>",
  "<span class=\"highlight-demo\"",
  "<<widget \"highlightCard\">>",
  "`反引号字符串：${$hero.profile.name} / ${string(_counter)}`",
  "<<highlightCard \"自定义 Widget Macro\">>",
  "<<crossFileCard \"跨文件 Widget Macro\">>",
]) {
  assert.ok(gallery.includes(sample), `highlight gallery should cover ${sample}`)
}

assert.ok(sharedWidgets.includes("<<widget \"crossFileCard\">>"))
assert.equal(gallery.includes("<<widget \"crossFileCard\">>"), false)

assert.equal(gallery.includes("[[Library<-查看藏书室]]"), false)
assert.equal(grammar.patterns.some(pattern => pattern.include === "#link"), false)
assert.equal(grammar.repository.link.match.includes("->"), false)
assert.equal(grammar.repository.link.match.includes("<-"), false)
assert.equal(grammar.repository.macro.beginCaptures["1"].name, "punctuation.definition.tag.begin.narrava-twee")
assert.equal(grammar.repository.macro.beginCaptures["2"].name, "punctuation.definition.tag.begin.narrava-twee")
assert.equal(grammar.repository.macro.beginCaptures["3"].name, "meta.identifier.macro.narrava-twee")
assert.equal(grammar.repository.macro.endCaptures["1"].name, "punctuation.definition.tag.end.narrava-twee")
assert.equal(grammar.repository.interpolation.beginCaptures["1"].name, "punctuation.definition.template-expression.begin.narrava-twee")
assert.equal(grammar.repository.interpolation.endCaptures["1"].name, "punctuation.definition.template-expression.end.narrava-twee")
assert.equal(grammar.repository.macro.name, undefined)
assert.equal(grammar.repository.passage.captures["2"].name, "entity.name.function.narrava-twee")
assert.equal(grammar.repository.passage.captures["4"].name, "support.type.passage-tag.narrava-twee")
assert.equal(grammar.repository.link.captures["1"].name, "punctuation.definition.link.begin.narrava-twee")
assert.equal(grammar.repository.link.captures["4"].name, "entity.name.function.passage.narrava-twee")
assert.equal(grammar.repository.link.captures["5"].name, "punctuation.definition.link.end.narrava-twee")
for (const variable of grammar.repository.variable.patterns) {
  assert.equal(variable.name, "variable.language.narrava-twee")
}

for (const pattern of [
  grammar.repository.comment.begin,
  grammar.repository.comment.end,
  grammar.repository.passage.match,
  grammar.repository.macro.begin,
  grammar.repository.macro.end,
  grammar.repository.html.begin,
  grammar.repository.html.end,
]) {
  assert.doesNotThrow(() => new RegExp(pattern), `invalid grammar regex: ${pattern}`)
}

const compilerOwned = hooks.slice(hooks.indexOf("fn compiler_owns_macro"))
const macroNames = [...compilerOwned.matchAll(/"([a-z]+)"/g)].map(match => match[1])
macroNames.push("link")
const nativeFunctionTable = evaluator.slice(
  evaluator.indexOf("fn native_function"),
  evaluator.indexOf("fn native_namespace"),
)
const functionNames = [...nativeFunctionTable.matchAll(/"([A-Za-z]+)"\s*=>/g)].map(match => match[1])

for (const name of [...new Set([...macroNames, ...functionNames])]) {
  assert.ok(reference.includes(`\`${name}`), `quick reference should list ${name}`)
  assert.ok(serialized.includes(name), `grammar should highlight ${name}`)
}

console.log("Narrava Twee VS Code extension manifest and grammar verified")

import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import textmate from "vscode-textmate"
import oniguruma from "vscode-oniguruma"

const root = new URL("../", import.meta.url)
const repository = new URL("../../../", import.meta.url)
const manifest = JSON.parse(await readFile(new URL("package.json", root), "utf8"))
const grammar = JSON.parse(
  await readFile(new URL("syntaxes/narrava-twee.tmLanguage.json", root), "utf8"),
)
const language = JSON.parse(await readFile(new URL("language-configuration.json", root), "utf8"))
const reference = await readFile(new URL("docs/reference/api-and-syntax.md", repository), "utf8")
const hooks = await readFile(new URL("src/macro_runtime/hooks.rs", repository), "utf8")
const evaluator = await readFile(new URL("src/expression/evaluator/chain.rs", repository), "utf8")
const expressionDts = await readFile(new URL("references/narrava-expression.d.ts", root), "utf8")
const providers = await readFile(new URL("src/providers.js", root), "utf8")
const extension = await readFile(new URL("extension.js", root), "utf8")
const gallery = await readFile(new URL("examples/contents/story/main.twee", repository), "utf8")
const sharedWidgets = await readFile(
  new URL("examples/contents/story/widgets.twee", repository),
  "utf8",
)
const specialPassages = await readFile(new URL("src/story/special.rs", repository), "utf8")
const wasm = await readFile(new URL("node_modules/vscode-oniguruma/release/onig.wasm", root))

await oniguruma.loadWASM(wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength))
const registry = new textmate.Registry({
  onigLib: Promise.resolve({
    createOnigScanner: (sources) => new oniguruma.OnigScanner(sources),
    createOnigString: (source) => new oniguruma.OnigString(source),
  }),
  loadGrammar: async () =>
    textmate.parseRawGrammar(JSON.stringify(grammar), "narrava-twee.tmLanguage.json"),
})
const tokenGrammar = await registry.loadGrammar("source.narrava-twee")

function scopeFor(line, text) {
  const token = tokenGrammar
    .tokenizeLine(line)
    .tokens.find((item) => line.slice(item.startIndex, item.endIndex) === text)
  return token?.scopes.at(-1)
}

function scopesFor(line, text) {
  return tokenGrammar
    .tokenizeLine(line)
    .tokens.filter((item) => line.slice(item.startIndex, item.endIndex) === text)
    .map((item) => item.scopes.at(-1))
}

assert.equal(manifest.engines.vscode.startsWith("^"), true)
assert.equal(manifest.main, "./extension.js")
assert.equal(manifest.version, "0.5.0")
assert.ok(manifest.files.includes("references/**"))
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
  "punctuation.section.embedded.begin.narrava-twee",
  "punctuation.section.embedded.end.narrava-twee",
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
  '<span class="highlight-demo"',
  '<<widget "highlightCard">>',
  "`反引号字符串：${$hero.profile.name} / ${string(_counter)}`",
  '<<highlightCard "自定义 Widget Macro">>',
  '<<crossFileCard "跨文件 Widget Macro">>',
]) {
  assert.ok(gallery.includes(sample), `highlight gallery should cover ${sample}`)
}

assert.ok(sharedWidgets.includes('<<widget "crossFileCard">>'))
assert.equal(gallery.includes('<<widget "crossFileCard">>'), false)

assert.equal(gallery.includes("[[Library<-查看藏书室]]"), false)
assert.equal(
  grammar.patterns.some((pattern) => pattern.include === "#link"),
  false,
)
assert.equal(grammar.repository.link.match.includes("->"), false)
assert.equal(grammar.repository.link.match.includes("<-"), false)
assert.equal(
  grammar.repository.macro.beginCaptures["1"].name,
  "punctuation.definition.tag.begin.narrava-twee",
)
assert.equal(
  grammar.repository.macro.beginCaptures["2"].name,
  "punctuation.definition.tag.begin.narrava-twee",
)
assert.equal(grammar.repository.macro.beginCaptures["3"].name, "meta.identifier.macro.narrava-twee")
assert.equal(
  grammar.repository.macro.endCaptures["1"].name,
  "punctuation.definition.tag.end.narrava-twee",
)
assert.equal(
  grammar.repository.interpolation.beginCaptures["1"].name,
  "punctuation.definition.template-expression.begin.narrava-twee",
)
assert.equal(
  grammar.repository.interpolation.beginCaptures["2"].name,
  "punctuation.section.embedded.begin.narrava-twee",
)
assert.equal(
  grammar.repository.interpolation.endCaptures["1"].name,
  "punctuation.section.embedded.end.narrava-twee",
)
assert.equal(grammar.repository.macro.name, undefined)
assert.equal(grammar.repository.passage.captures["2"].name, "entity.name.function.narrava-twee")
assert.equal(grammar.repository.passage.captures["4"].name, "support.type.passage-tag.narrava-twee")
assert.ok(serialized.includes("support.type.passage.special.narrava-twee"))
for (const name of ["Start", "StoryInit", "Header", "Footer", "Bar", "BarStowed"]) {
  assert.ok(specialPassages.includes(`= "${name}"`), `Core should define special Passage ${name}`)
  assert.ok(grammar.repository["special-passage"].match.includes(name))
}
assert.equal(scopeFor(":: StoryInit", "StoryInit"), "support.type.passage.special.narrava-twee")
assert.equal(scopeFor(":: Hall [hub]", "Hall"), "entity.name.function.narrava-twee")
assert.equal(
  scopeFor("<<link [[进入大厅|Hall]]>><</link>>", "Hall"),
  "entity.name.function.passage.narrava-twee",
)
assert.deepEqual(scopesFor("<<link [[进入大厅|Hall]]>><</link>>", "["), [
  "punctuation.definition.link.outer.narrava-twee",
  "keyword.operator.link.inner.narrava-twee",
])
assert.deepEqual(scopesFor("<<link [[进入大厅|Hall]]>><</link>>", "]"), [
  "keyword.operator.link.inner.narrava-twee",
  "punctuation.definition.link.outer.narrava-twee",
])
const variableChain = "<<set setup.build.channel to $hero.profile.name>>"
assert.equal(scopeFor(variableChain, "setup"), "variable.language.narrava-twee")
assert.equal(scopeFor(variableChain, "$hero"), "variable.language.narrava-twee")
assert.deepEqual(scopesFor(variableChain, "."), [
  "keyword.operator.accessor.narrava-twee",
  "keyword.operator.accessor.narrava-twee",
  "keyword.operator.accessor.narrava-twee",
  "keyword.operator.accessor.narrava-twee",
])
for (const property of ["build", "channel", "profile", "name"]) {
  assert.equal(scopeFor(variableChain, property), "variable.other.property.narrava-twee")
}
const template = "<<print `值：${$hero.profile.name}`>>"
assert.equal(
  scopeFor(template, "$"),
  "punctuation.definition.template-expression.begin.narrava-twee",
)
assert.equal(scopeFor(template, "{"), "punctuation.section.embedded.begin.narrava-twee")
assert.equal(scopeFor(template, "}"), "punctuation.section.embedded.end.narrava-twee")
assert.deepEqual(scopesFor(template, "."), [
  "keyword.operator.accessor.narrava-twee",
  "keyword.operator.accessor.narrava-twee",
])
assert.equal(scopeFor(template, "$hero"), "variable.language.narrava-twee")
assert.equal(scopeFor(template, "profile"), "variable.other.property.narrava-twee")
assert.equal(scopeFor(template, "name"), "variable.other.property.narrava-twee")
assert.equal(
  scopeFor("<<print random()>>", "random"),
  "support.function.builtin.expression.narrava-twee",
)
assert.equal(
  scopeFor("<<run $items.splice(0, 1)>>", "splice"),
  "support.function.builtin.expression.narrava-twee",
)
assert.equal(
  grammar.repository.link.captures["1"].name,
  "punctuation.definition.link.outer.narrava-twee",
)
assert.equal(grammar.repository.link.captures["2"].name, "keyword.operator.link.inner.narrava-twee")
assert.equal(
  grammar.repository.link.captures["5"].name,
  "entity.name.function.passage.narrava-twee",
)
assert.equal(grammar.repository.link.captures["6"].name, "keyword.operator.link.inner.narrava-twee")
assert.equal(
  grammar.repository.link.captures["7"].name,
  "punctuation.definition.link.outer.narrava-twee",
)
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
const macroNames = [...compilerOwned.matchAll(/"([a-z]+)"/g)].map((match) => match[1])
macroNames.push("link")
const nativeFunctionTable = evaluator.slice(
  evaluator.indexOf("fn native_function"),
  evaluator.indexOf("fn native_namespace"),
)
const functionNames = [...nativeFunctionTable.matchAll(/"([A-Za-z]+)"\s*=>/g)].map(
  (match) => match[1],
)

for (const name of new Set([...macroNames, ...functionNames])) {
  assert.ok(reference.includes(`\`${name}`), `quick reference should list ${name}`)
  assert.ok(serialized.includes(name), `grammar should highlight ${name}`)
}
for (const name of functionNames) {
  assert.match(expressionDts, new RegExp(`\\b${name}(?:<[^>]+>)?\\(`))
}

assert.match(expressionDts, /\bclone<[^>]+>\(/)
assert.ok(providers.includes("resolveExpressionApis"))
assert.ok(providers.includes("narrava-expression.d.ts"))
assert.ok(extension.includes("registerHoverProvider"))

console.log("Narrava Twee VS Code extension manifest and grammar verified")

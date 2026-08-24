"use strict"

const BUILTIN_MACRO_KINDS = Object.freeze({
  break: "inline", capture: "container", case: "clause", continue: "inline",
  default: "clause", else: "clause", elseif: "clause", exit: "inline",
  for: "container", goto: "inline", if: "container", include: "inline",
  link: "container", print: "inline", return: "inline", run: "inline",
  set: "inline", silently: "container", switch: "container", unset: "inline",
  while: "container", widget: "container", text: "inline",
  button: "container", replace: "container", slot: "container", checkbox: "inline",
  radiobutton: "inline", textbox: "inline",
})
const BUILTIN_MACROS = Object.freeze(Object.keys(BUILTIN_MACRO_KINDS))

const COMMENT = /\/%[\s\S]*?%\//g
const TWEE_MACRO = /<<(\/)?([A-Za-z_][A-Za-z0-9_-]*)/g
const WIDGET = /<<widget\s+(?:"([A-Za-z_][A-Za-z0-9_]*)"|'([A-Za-z_][A-Za-z0-9_]*)'|([A-Za-z_][A-Za-z0-9_]*))\s*>>/g
const SCRIPT_MACRO = /\bMacro\s*\.\s*(?:add|update)\s*\(\s*(["'])([A-Za-z_][A-Za-z0-9_-]*)\1/g

function withoutComments(text) {
  return text.replace(COMMENT, match => " ".repeat(match.length))
}

function scanTwee(text) {
  const source = withoutComments(text)
  const definitions = []
  const calls = []
  for (const match of source.matchAll(WIDGET)) {
    const name = match[1] ?? match[2] ?? match[3]
    const relative = match[0].indexOf(name)
    definitions.push({ name, start: match.index + relative, length: name.length, bodyKind: "inline" })
  }
  for (const match of source.matchAll(TWEE_MACRO)) {
    const name = match[2]
    const relative = match[0].lastIndexOf(name)
    calls.push({ name, start: match.index + relative, length: name.length, closing: Boolean(match[1]) })
  }
  return { definitions, calls }
}

function scanScript(text) {
  const definitions = []
  const matches = [...text.matchAll(SCRIPT_MACRO)]
  for (const [index, match] of matches.entries()) {
    const end = matches[index + 1]?.index ?? text.length
    const definitionSource = text.slice(match.index, end)
    const body = /\bbody\s*:\s*["'](inline|container)["']/.exec(definitionSource)
    definitions.push({ name: match[2], start: match.index + match[0].indexOf(match[2]), length: match[2].length, bodyKind: body?.[1] })
  }
  return definitions
}

function knownNames(definitions) {
  return new Set([...BUILTIN_MACROS, ...definitions.map(definition => definition.name)])
}

function macroKinds(definitions) {
  const kinds = new Map(Object.entries(BUILTIN_MACRO_KINDS))
  for (const definition of definitions) {
    if (definition.bodyKind) kinds.set(definition.name, definition.bodyKind)
  }
  return kinds
}

module.exports = { BUILTIN_MACROS, BUILTIN_MACRO_KINDS, knownNames, macroKinds, scanScript, scanTwee }

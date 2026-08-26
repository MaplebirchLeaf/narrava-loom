"use strict"

const BUILTIN_MACRO_KINDS = Object.freeze({
  break: "inline",
  capture: "container",
  case: "clause",
  continue: "inline",
  default: "clause",
  else: "clause",
  elseif: "clause",
  exit: "inline",
  for: "container",
  goto: "inline",
  if: "container",
  include: "inline",
  link: "container",
  print: "inline",
  return: "inline",
  run: "inline",
  set: "inline",
  silently: "container",
  switch: "container",
  unset: "inline",
  while: "container",
  widget: "container",
  text: "inline",
  button: "container",
  replace: "container",
  slot: "container",
  checkbox: "inline",
  radiobutton: "inline",
  textbox: "inline",
})
const BUILTIN_MACROS = Object.freeze(Object.keys(BUILTIN_MACRO_KINDS))
const SPECIAL_PASSAGES = Object.freeze([
  "Start",
  "StoryInit",
  "Header",
  "Footer",
  "Bar",
  "BarStowed",
])

const COMMENT = /\/%[\s\S]*?%\//g
const TWEE_MACRO = /<<(\/)?([A-Za-z_][A-Za-z0-9_-]*)/g
const WIDGET =
  /<<widget\s+(?:"([A-Za-z_][A-Za-z0-9_]*)"|'([A-Za-z_][A-Za-z0-9_]*)'|([A-Za-z_][A-Za-z0-9_]*))\s*>>/g
const SCRIPT_MACRO = /\bMacro\s*\.\s*(?:add|update)\s*\(\s*(["'])([A-Za-z_][A-Za-z0-9_-]*)\1/g
const SCRIPT_FUNCTION = /\b(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*\(/g
const SCRIPT_FUNCTION_VALUE =
  /\b(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*(?::[^=;\r\n]+)?=\s*(?:async\s*)?(?:function\b|(?:\([^)]*\)|[A-Za-z_$][A-Za-z0-9_$]*)\s*(?::[^=\r\n]+)?=>)/g
const PASSAGE = /^::[ \t]+([^[\r\n]+?)(?:[ \t]+\[([^\]\r\n]*)\])?[ \t]*$/gm
const PASSAGE_LINK = /\[\[([^|\]\r\n]+)\|([^\]\r\n]+)\]\]/g
const EXPRESSION_CALL = /\b([A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z_$][A-Za-z0-9_$]*)*)\s*(?=\()/g

function withoutComments(text) {
  return text.replace(COMMENT, (match) => " ".repeat(match.length))
}

function scanTwee(text) {
  const source = withoutComments(text)
  const definitions = []
  const calls = []
  const passages = []
  const links = []
  const functionCalls = []
  for (const match of source.matchAll(WIDGET)) {
    const name = match[1] ?? match[2] ?? match[3]
    const relative = match[0].indexOf(name)
    definitions.push({
      name,
      start: match.index + relative,
      length: name.length,
      bodyKind: "inline",
    })
  }
  for (const match of source.matchAll(TWEE_MACRO)) {
    const name = match[2]
    const relative = match[0].lastIndexOf(name)
    calls.push({
      name,
      start: match.index + relative,
      length: name.length,
      closing: Boolean(match[1]),
    })
  }
  for (const match of source.matchAll(PASSAGE)) {
    const name = match[1].trim()
    const relative = match[0].indexOf(name)
    const rawTags = match[2]
    const tags = rawTags?.trim() ? rawTags.trim().split(/\s+/) : []
    const bracket = rawTags === undefined ? -1 : match[0].lastIndexOf("[")
    passages.push({
      name,
      start: match.index + relative,
      length: name.length,
      special: SPECIAL_PASSAGES.includes(name),
      tags,
      tagsStart: bracket < 0 ? undefined : match.index + bracket + 1,
      tagsLength: rawTags?.length ?? 0,
    })
  }
  for (const match of source.matchAll(PASSAGE_LINK)) {
    const rawTarget = match[2]
    const target = rawTarget.trim()
    if (!target) continue
    const relative = match[0].lastIndexOf(rawTarget) + rawTarget.indexOf(target)
    links.push({ target, start: match.index + relative, length: target.length })
  }
  for (const match of source.matchAll(EXPRESSION_CALL)) {
    const name = match[1]
    functionCalls.push({ name, start: match.index, length: name.length })
  }
  return { definitions, calls, passages, links, functionCalls }
}

function scanScript(text) {
  const definitions = []
  const matches = [...text.matchAll(SCRIPT_MACRO)]
  for (const [index, match] of matches.entries()) {
    const end = matches[index + 1]?.index ?? text.length
    const definitionSource = text.slice(match.index, end)
    const body = /\bbody\s*:\s*["'](inline|container)["']/.exec(definitionSource)
    definitions.push({
      name: match[2],
      start: match.index + match[0].indexOf(match[2]),
      length: match[2].length,
      bodyKind: body?.[1],
    })
  }
  return definitions
}

function scanScriptFunctions(text) {
  const definitions = []
  for (const pattern of [SCRIPT_FUNCTION, SCRIPT_FUNCTION_VALUE]) {
    pattern.lastIndex = 0
    for (const match of text.matchAll(pattern)) {
      const name = match[1]
      definitions.push({ name, start: match.index + match[0].indexOf(name), length: name.length })
    }
  }
  return definitions.toSorted((left, right) => left.start - right.start)
}

function knownNames(definitions) {
  return new Set([...BUILTIN_MACROS, ...definitions.map((definition) => definition.name)])
}

function macroKinds(definitions) {
  const kinds = new Map(Object.entries(BUILTIN_MACRO_KINDS))
  for (const definition of definitions) {
    if (definition.bodyKind) kinds.set(definition.name, definition.bodyKind)
  }
  return kinds
}

function missingPassageLinks(links, passages) {
  const names = new Set(passages.map((passage) => passage.name))
  return links.filter((link) => !names.has(link.target))
}

function taggedSpecialPassages(passages) {
  return passages.filter((passage) => passage.special && passage.tags.length > 0)
}

module.exports = {
  BUILTIN_MACROS,
  BUILTIN_MACRO_KINDS,
  SPECIAL_PASSAGES,
  knownNames,
  macroKinds,
  missingPassageLinks,
  scanScript,
  scanScriptFunctions,
  scanTwee,
  taggedSpecialPassages,
}

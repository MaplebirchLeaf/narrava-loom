"use strict"

// 把 .twee 与脚本源码扫描为结构化目录（宏定义、Passage、链接与函数调用），
// 供语义着色、跳转、补全与诊断使用。所有位置都相对源码起始偏移。

// 内置宏名 → 形态：inline 原地展开 / container 包裹正文 / clause 分支子句。
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
  button: "container",
  replace: "container",
  slot: "container",
  checkbox: "inline",
  radiobutton: "inline",
  textbox: "inline",
})
// 内置宏名列表（BUILTIN_MACRO_KINDS 的键）。
const BUILTIN_MACROS = Object.freeze(Object.keys(BUILTIN_MACRO_KINDS))
// 特殊 Passage 名：正文进入固定区域，且不得带有 Tag。
const SPECIAL_PASSAGES = Object.freeze([
  "Start",
  "StoryInit",
  "Header",
  "Footer",
  "Bar",
  "BarStowed",
])

// 块注释（/% ... %/）；扫描前先剔除，避免把注释里的内容当成代码。
const COMMENT = /\/%[\s\S]*?%\//g
// 宏调用 <<name 与闭合 <</name>>。
const TWEE_MACRO = /<<(\/)?([A-Za-z_][A-Za-z0-9_-]*)/g
// widget 定义：widget 名可以作为宏调用。
const WIDGET =
  /<<widget\s+(?:"([A-Za-z_][A-Za-z0-9_]*)"|'([A-Za-z_][A-Za-z0-9_]*)'|([A-Za-z_][A-Za-z0-9_]*))\s*>>/g
// 脚本中 Macro.add/update 注册的宏名。
const SCRIPT_MACRO = /\bMacro\s*\.\s*(?:add|update)\s*\(\s*(["'])([A-Za-z_][A-Za-z0-9_-]*)\1/g
// 脚本顶层 function 声明。
const SCRIPT_FUNCTION = /\b(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*\(/g
// 脚本顶层 const/let/var 赋值为函数（箭头函数或 function 表达式）。
const SCRIPT_FUNCTION_VALUE =
  /\b(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*(?::[^=;\r\n]+)?=\s*(?:async\s*)?(?:function\b|(?:\([^)]*\)|[A-Za-z_$][A-Za-z0-9_$]*)\s*(?::[^=\r\n]+)?=>)/g
// Passage 标题行：:: 名称 [tags]。
const PASSAGE = /^::[ \t]+([^[\r\n]+?)(?:[ \t]+\[([^\]\r\n]*)\])?[ \t]*$/gm
// Twee 链接 [[显示文本|目标]]。
const PASSAGE_LINK = /\[\[([^|\]\r\n]+)\|([^\]\r\n]+)\]\]/g
// 表达式里的函数调用（含 a.b.c 链式形式）。
const EXPRESSION_CALL = /\b([A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z_$][A-Za-z0-9_$]*)*)\s*(?=\()/g

/** 用等长空白替换块注释，保持其余文本的行列位置不变。 */
function withoutComments(text) {
  return text.replace(COMMENT, (match) => " ".repeat(match.length))
}

/** 扫描 Twee 源码，返回宏定义、宏调用、Passage、链接与函数调用的位置清单。 */
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

/** 扫描脚本源码，收集 Macro.add/update 定义的宏（含 body 形态）。 */
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

/** 扫描脚本源码，收集顶层函数定义并按起始位置排序。 */
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

/** 已知宏名集合 = 内置宏 ∪ 扫描到的全部定义。 */
function knownNames(definitions) {
  return new Set([...BUILTIN_MACROS, ...definitions.map((definition) => definition.name)])
}

/** 宏名 → 形态映射；脚本定义覆盖内置默认形态。 */
function macroKinds(definitions) {
  const kinds = new Map(Object.entries(BUILTIN_MACRO_KINDS))
  for (const definition of definitions) {
    if (definition.bodyKind) kinds.set(definition.name, definition.bodyKind)
  }
  return kinds
}

/** 指向不存在 Passage 的链接（用于诊断）。 */
function missingPassageLinks(links, passages) {
  const names = new Set(passages.map((passage) => passage.name))
  return links.filter((link) => !names.has(link.target))
}

/** 带 Tag 的特殊 Passage；按规则应为空列表。 */
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

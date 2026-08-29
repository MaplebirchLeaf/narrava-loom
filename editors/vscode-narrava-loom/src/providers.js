"use strict"

// VSCode Language Provider：语义着色（已知宏、特殊 Passage）、跳转
// （链接→Passage、函数调用→函数定义、宏调用→宏定义）与宏名补全。

const vscode = require("vscode")
const { scanTwee } = require("./catalog")
const { readFileSync } = require("node:fs")
const path = require("node:path")
const { resolveExpressionApis } = require("./expression-api")

const expressionReferencePath = path.join(__dirname, "../references/narrava-expression.d.ts")
const expressionReference = readFileSync(expressionReferencePath, "utf8")
const expressionReferenceUri = vscode.Uri.file(expressionReferencePath)

// 语义 token 图例：0 = keyword（宏），1 = type（特殊 Passage）。
const legend = new vscode.SemanticTokensLegend(["keyword", "type"])

/** 返回 position 命中的宏调用记录；无则 undefined。 */
function callAt(document, position) {
  const offset = document.offsetAt(position)
  return scanTwee(document.getText()).calls.find(
    (call) => offset >= call.start && offset <= call.start + call.length,
  )
}

/** 返回 position 命中的 Twee 链接记录；无则 undefined。 */
function passageLinkAt(document, position) {
  const offset = document.offsetAt(position)
  return scanTwee(document.getText()).links.find(
    (link) => offset >= link.start && offset <= link.start + link.length,
  )
}

/** 返回 position 命中的函数调用记录；无则 undefined。 */
function functionCallAt(document, position) {
  const offset = document.offsetAt(position)
  return scanTwee(document.getText()).functionCalls.find(
    (call) => offset >= call.start && offset <= call.start + call.length,
  )
}

/** 找到原生 Expression API 在随扩展发布的 DTS 参考中的精确位置。 */
function expressionDefinitionOffset(api) {
  const sections = {
    global: "interface Globals",
    namespace: "interface ObjectNamespace",
    array: "interface ArrayValue",
    string: "interface StringValue",
  }
  const section = expressionReference.indexOf(sections[api.kind])
  const method = api.name.split(".").at(-1)
  const relative = new RegExp(`\\b${method}(?:<[^>]+>)?\\(`).exec(
    expressionReference.slice(section),
  )?.index
  return relative === undefined ? -1 : section + relative
}

/** 为 Twee Expression 原生函数/方法提供签名与用途说明。 */
function hoverProvider() {
  return {
    provideHover(document, position) {
      const call = functionCallAt(document, position)
      if (!call) return undefined
      const apis = resolveExpressionApis(call.name)
      if (apis.length === 0) return undefined
      const markdown = new vscode.MarkdownString()
      for (const api of apis) {
        markdown.appendCodeblock(api.signature, "typescript")
        markdown.appendMarkdown(`${api.description}\n\n`)
      }
      markdown.appendMarkdown("_Narrava Twee Expression API；不是 JavaScript 全局值。_")
      return new vscode.Hover(markdown)
    },
  }
}

/** 为已知宏名与特殊 Passage 提供语义着色。 */
function semanticProvider(workspace) {
  return {
    onDidChangeSemanticTokens: workspace.emitter.event,
    provideDocumentSemanticTokens(document) {
      const builder = new vscode.SemanticTokensBuilder(legend)
      for (const call of scanTwee(document.getText()).calls) {
        if (!workspace.known.has(call.name)) continue
        const start = document.positionAt(call.start)
        builder.push(start.line, start.character, call.length, 0, 0)
      }
      for (const passage of scanTwee(document.getText()).passages) {
        if (!passage.special) continue
        const start = document.positionAt(passage.start)
        builder.push(start.line, start.character, passage.length, 1, 0)
      }
      return builder.build()
    },
  }
}

/** 跳转：链接→Passage 定义，函数调用→函数定义，宏调用→宏定义。 */
function definitionProvider(workspace) {
  return {
    async provideDefinition(document, position) {
      const link = passageLinkAt(document, position)
      if (link) {
        const locations = await Promise.all(
          workspace.passages
            .filter((item) => item.name === link.target)
            .map(async (passage) => {
              const target = await vscode.workspace.openTextDocument(passage.uri)
              return new vscode.Location(passage.uri, target.positionAt(passage.start))
            }),
        )
        return locations
      }
      const functionCall = functionCallAt(document, position)
      if (functionCall) {
        const locations = await Promise.all(
          workspace.functions
            .filter((item) => item.name === functionCall.name)
            .map(async (definition) => {
              const target = await vscode.workspace.openTextDocument(definition.uri)
              return new vscode.Location(definition.uri, target.positionAt(definition.start))
            }),
        )
        if (locations.length > 0) return locations
        const builtinLocations = resolveExpressionApis(functionCall.name)
          .map((api) => expressionDefinitionOffset(api))
          .filter((offset) => offset >= 0)
          .map(
            (offset) =>
              new vscode.Location(
                expressionReferenceUri,
                new vscode.Position(
                  expressionReference.slice(0, offset).split("\n").length - 1,
                  offset - (expressionReference.lastIndexOf("\n", offset) + 1),
                ),
              ),
          )
        if (builtinLocations.length > 0) return builtinLocations
      }
      const call = callAt(document, position)
      if (!call) return undefined
      const locations = await Promise.all(
        workspace.definitions
          .filter((item) => item.name === call.name)
          .map(async (definition) => {
            const target = await vscode.workspace.openTextDocument(definition.uri)
            return new vscode.Location(definition.uri, target.positionAt(definition.start))
          }),
      )
      return locations
    },
  }
}

/** 补全所有已知宏名（内置 + 脚本定义）。 */
function completionProvider(workspace) {
  return {
    provideCompletionItems() {
      return [...workspace.known]
        .toSorted()
        .map((name) => new vscode.CompletionItem(name, vscode.CompletionItemKind.Function))
    },
  }
}

module.exports = {
  completionProvider,
  definitionProvider,
  hoverProvider,
  legend,
  semanticProvider,
}

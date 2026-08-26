"use strict"

// VSCode Language Provider：语义着色（已知宏、特殊 Passage）、跳转
// （链接→Passage、函数调用→函数定义、宏调用→宏定义）与宏名补全。

const vscode = require("vscode")
const { scanTwee } = require("./catalog")

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

module.exports = { completionProvider, definitionProvider, legend, semanticProvider }

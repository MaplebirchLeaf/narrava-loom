"use strict"

const vscode = require("vscode")
const { scanTwee } = require("./catalog")

const legend = new vscode.SemanticTokensLegend(["keyword", "type"])

function callAt(document, position) {
  const offset = document.offsetAt(position)
  return scanTwee(document.getText()).calls.find(
    (call) => offset >= call.start && offset <= call.start + call.length,
  )
}

function passageLinkAt(document, position) {
  const offset = document.offsetAt(position)
  return scanTwee(document.getText()).links.find(
    (link) => offset >= link.start && offset <= link.start + link.length,
  )
}

function functionCallAt(document, position) {
  const offset = document.offsetAt(position)
  return scanTwee(document.getText()).functionCalls.find(
    (call) => offset >= call.start && offset <= call.start + call.length,
  )
}

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

"use strict"

const vscode = require("vscode")
const { scanTwee } = require("./catalog")

const legend = new vscode.SemanticTokensLegend(["keyword"])

function callAt(document, position) {
  const offset = document.offsetAt(position)
  return scanTwee(document.getText()).calls.find(call => offset >= call.start && offset <= call.start + call.length)
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
      return builder.build()
    },
  }
}

function definitionProvider(workspace) {
  return {
    async provideDefinition(document, position) {
      const call = callAt(document, position)
      if (!call) return undefined
      const locations = []
      for (const definition of workspace.definitions.filter(item => item.name === call.name)) {
        const target = await vscode.workspace.openTextDocument(definition.uri)
        locations.push(new vscode.Location(definition.uri, target.positionAt(definition.start)))
      }
      return locations
    },
  }
}

function completionProvider(workspace) {
  return {
    provideCompletionItems() {
      return [...workspace.known].sort().map(name => new vscode.CompletionItem(name, vscode.CompletionItemKind.Function))
    },
  }
}

module.exports = { completionProvider, definitionProvider, legend, semanticProvider }

"use strict"

const vscode = require("vscode")
const { scanTwee } = require("./catalog")

const legend = new vscode.SemanticTokensLegend(["keyword", "type"])

function callAt(document, position) {
  const offset = document.offsetAt(position)
  return scanTwee(document.getText()).calls.find(call => offset >= call.start && offset <= call.start + call.length)
}

function passageLinkAt(document, position) {
  const offset = document.offsetAt(position)
  return scanTwee(document.getText()).links.find(link => offset >= link.start && offset <= link.start + link.length)
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
        const locations = []
        for (const passage of workspace.passages.filter(item => item.name === link.target)) {
          const target = await vscode.workspace.openTextDocument(passage.uri)
          locations.push(new vscode.Location(passage.uri, target.positionAt(passage.start)))
        }
        return locations
      }
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

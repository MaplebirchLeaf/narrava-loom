"use strict"

const vscode = require("vscode")
const { MacroWorkspace } = require("./src/workspace")
const { completionProvider, definitionProvider, legend, semanticProvider } = require("./src/providers")

async function activate(context) {
  const workspace = new MacroWorkspace()
  const selector = { language: "narrava-twee", scheme: "file" }
  let refreshTimer
  const scheduleRefresh = () => {
    clearTimeout(refreshTimer)
    refreshTimer = setTimeout(() => workspace.refresh(), 120)
  }
  context.subscriptions.push(
    workspace,
    vscode.languages.registerDocumentSemanticTokensProvider(selector, semanticProvider(workspace), legend),
    vscode.languages.registerDefinitionProvider(selector, definitionProvider(workspace)),
    vscode.languages.registerCompletionItemProvider(selector, completionProvider(workspace), "<"),
    vscode.workspace.onDidChangeTextDocument(event => {
      if (/\.(?:twee|ts|js)$/.test(event.document.uri.path)) scheduleRefresh()
    }),
    vscode.workspace.onDidOpenTextDocument(document => {
      if (document.languageId === "narrava-twee") workspace.validate(document)
    }),
    vscode.workspace.onDidCreateFiles(scheduleRefresh),
    vscode.workspace.onDidDeleteFiles(scheduleRefresh),
    vscode.workspace.onDidRenameFiles(scheduleRefresh),
  )
  await workspace.refresh()
}

function deactivate() {}

module.exports = { activate, deactivate }

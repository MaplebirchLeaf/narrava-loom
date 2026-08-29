"use strict"

// Narrava Loom Twee 扩展入口：注册语义着色、跳转、补全与诊断 Provider，
// 并在 Twee/脚本文档变更时防抖刷新工作区索引。

const vscode = require("vscode")
const { MacroWorkspace } = require("./src/workspace")
const {
  completionProvider,
  definitionProvider,
  hoverProvider,
  legend,
  semanticProvider,
} = require("./src/providers")

/** 激活扩展：创建工作区索引、注册 Provider 与文件事件，并做首次刷新。 */
async function activate(context) {
  const workspace = new MacroWorkspace()
  const selector = { language: "narrava-twee", scheme: "file" }
  let refreshTimer
  /** 防抖 120ms 后刷新工作区索引，合并连续编辑触发。 */
  const scheduleRefresh = () => {
    clearTimeout(refreshTimer)
    refreshTimer = setTimeout(() => workspace.refresh(), 120)
  }
  context.subscriptions.push(
    workspace,
    vscode.languages.registerDocumentSemanticTokensProvider(
      selector,
      semanticProvider(workspace),
      legend,
    ),
    vscode.languages.registerDefinitionProvider(selector, definitionProvider(workspace)),
    vscode.languages.registerHoverProvider(selector, hoverProvider()),
    vscode.languages.registerCompletionItemProvider(selector, completionProvider(workspace), "<"),
    vscode.workspace.onDidChangeTextDocument((event) => {
      if (/\.(?:twee|ts|js)$/.test(event.document.uri.path)) scheduleRefresh()
    }),
    vscode.workspace.onDidOpenTextDocument((document) => {
      if (document.languageId === "narrava-twee") workspace.validate(document)
    }),
    vscode.workspace.onDidCreateFiles(scheduleRefresh),
    vscode.workspace.onDidDeleteFiles(scheduleRefresh),
    vscode.workspace.onDidRenameFiles(scheduleRefresh),
  )
  await workspace.refresh()
}

/** 扩展停用；当前没有需要显式清理的资源。 */
function deactivate() {}

module.exports = { activate, deactivate }

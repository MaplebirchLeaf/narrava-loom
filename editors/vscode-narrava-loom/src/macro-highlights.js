"use strict"

const vscode = require("vscode")
const { macroTagPairs } = require("./catalog")

/** 光标所在的最近一层容器：强调首尾标签，保留参数的语义颜色。 */
function registerMacroHighlights(context, workspace) {
  const decoration = vscode.window.createTextEditorDecorationType({
    fontWeight: "bold",
    textDecoration: "underline",
    rangeBehavior: vscode.DecorationRangeBehavior.ClosedClosed,
  })
  const cache = new WeakMap()
  const update = (editor) => {
    if (!editor) return
    const { document } = editor
    const enabled = vscode.workspace
      .getConfiguration("narravaTwee", document.uri)
      .get("macroTagMatching", true)
    if (document.languageId !== "narrava-twee" || !enabled) {
      editor.setDecorations(decoration, [])
      return
    }
    let entry = cache.get(document)
    if (entry?.version !== document.version || entry?.kinds !== workspace.kinds) {
      entry = {
        version: document.version,
        kinds: workspace.kinds,
        pairs: macroTagPairs(document.getText(), workspace.kinds),
      }
      cache.set(document, entry)
    }
    const selected = new Set()
    for (const selection of editor.selections) {
      const offset = document.offsetAt(selection.active)
      const pair = entry.pairs.find(
        ({ opening, closing }) => opening.start <= offset && offset <= closing.end,
      )
      if (pair) selected.add(pair)
    }
    const range = (start, end) =>
      new vscode.Range(document.positionAt(start), document.positionAt(end))
    const ranges = []
    for (const { opening, closing } of selected) {
      ranges.push(
        range(opening.start, opening.nameEnd),
        range(opening.end - 2, opening.end),
        range(closing.start, closing.end),
      )
    }
    editor.setDecorations(decoration, ranges)
  }
  const updateVisible = () => vscode.window.visibleTextEditors.forEach(update)
  context.subscriptions.push(
    decoration,
    vscode.window.onDidChangeTextEditorSelection((event) => update(event.textEditor)),
    vscode.window.onDidChangeActiveTextEditor(update),
    vscode.window.onDidChangeVisibleTextEditors(updateVisible),
    vscode.workspace.onDidChangeTextDocument((event) => {
      for (const editor of vscode.window.visibleTextEditors) {
        if (editor.document === event.document) update(editor)
      }
    }),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("narravaTwee.macroTagMatching")) updateVisible()
    }),
    workspace.emitter.event(updateVisible),
  )
  updateVisible()
}

module.exports = { registerMacroHighlights }

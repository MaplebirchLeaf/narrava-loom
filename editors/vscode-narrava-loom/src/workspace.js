"use strict"

const vscode = require("vscode")
const { BUILTIN_MACROS, knownNames, macroKinds, scanScript, scanTwee } = require("./catalog")

class MacroWorkspace {
  constructor() {
    this.definitions = []
    this.known = new Set(BUILTIN_MACROS)
    this.kinds = macroKinds([])
    this.emitter = new vscode.EventEmitter()
    this.diagnostics = vscode.languages.createDiagnosticCollection("narrava-twee")
  }

  async refresh() {
    const files = await vscode.workspace.findFiles("**/*.{twee,ts,js}", "**/{target,node_modules,.git}/**")
    const definitions = []
    for (const uri of files) {
      const document = await vscode.workspace.openTextDocument(uri)
      const items = uri.path.endsWith(".twee") ? scanTwee(document.getText()).definitions : scanScript(document.getText())
      for (const item of items) definitions.push({ ...item, uri })
    }
    this.definitions = definitions
    this.known = knownNames(definitions)
    this.kinds = macroKinds(definitions)
    for (const document of vscode.workspace.textDocuments) this.validate(document)
    this.emitter.fire()
  }

  validate(document) {
    if (document.languageId !== "narrava-twee") return
    const calls = scanTwee(document.getText()).calls
    const errors = calls
      .filter(call => !call.closing && !this.known.has(call.name))
      .map(call => {
        const range = new vscode.Range(document.positionAt(call.start), document.positionAt(call.start + call.length))
        const diagnostic = new vscode.Diagnostic(range, `Macro \`${call.name}\` 未定义`, vscode.DiagnosticSeverity.Error)
        diagnostic.code = "narrava.macro.undefined"
        diagnostic.source = "Narrava Loom"
        return diagnostic
      })
    const stack = []
    for (const call of calls) {
      const kind = this.kinds.get(call.name)
      if (!this.known.has(call.name) || kind === "clause") continue
      if (!call.closing) {
        if (kind === "container") stack.push(call)
        continue
      }
      if (kind === "inline") {
        errors.push(this.bodyDiagnostic(document, call, `Inline Macro \`${call.name}\` 不能使用闭合标签`, "narrava.macro.inline_closed"))
        continue
      }
      const opening = stack.pop()
      if (!opening || opening.name !== call.name) {
        errors.push(this.bodyDiagnostic(document, call, `Macro 闭合不匹配：\`${call.name}\``, "narrava.macro.closing_mismatch"))
      }
    }
    for (const call of stack) {
      errors.push(this.bodyDiagnostic(document, call, `Container Macro \`${call.name}\` 缺少 <</${call.name}>>`, "narrava.macro.container_unclosed"))
    }
    this.diagnostics.set(document.uri, errors)
  }

  bodyDiagnostic(document, call, message, code) {
    const range = new vscode.Range(document.positionAt(call.start), document.positionAt(call.start + call.length))
    const diagnostic = new vscode.Diagnostic(range, message, vscode.DiagnosticSeverity.Error)
    diagnostic.code = code
    diagnostic.source = "Narrava Loom"
    return diagnostic
  }

  dispose() {
    this.diagnostics.dispose()
    this.emitter.dispose()
  }
}

module.exports = { MacroWorkspace }

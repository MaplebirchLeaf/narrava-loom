"use strict"

// 工作区索引：扫描 *.twee / *.ts / *.js 收集宏、Passage 与函数定义，
// 并为 Twee 文档产出诊断（未定义宏、缺失 Passage、特殊 Passage 带 Tag、闭合不匹配）。

const vscode = require("vscode")
const {
  BUILTIN_MACROS,
  knownNames,
  macroKinds,
  missingPassageLinks,
  scanScript,
  scanScriptFunctions,
  scanTwee,
  taggedSpecialPassages,
} = require("./catalog")

/** 工作区级目录：持有全部定义索引与诊断集合，供各 Provider 共享。 */
class MacroWorkspace {
  constructor() {
    this.definitions = []
    this.passages = []
    this.functions = []
    this.known = new Set(BUILTIN_MACROS)
    this.kinds = macroKinds([])
    this.emitter = new vscode.EventEmitter()
    this.diagnostics = vscode.languages.createDiagnosticCollection("narrava-twee")
  }

  /** 重新扫描整个工作区：重建定义索引与 known/kinds，并重新校验已打开的文档。 */
  async refresh() {
    const files = await vscode.workspace.findFiles(
      "**/*.{twee,ts,js}",
      "**/{target,node_modules,.git}/**",
    )
    const definitions = []
    const passages = []
    const functions = []
    const documents = await Promise.all(files.map((uri) => vscode.workspace.openTextDocument(uri)))
    for (const document of documents) {
      const uri = document.uri
      const twee = uri.path.endsWith(".twee") ? scanTwee(document.getText()) : undefined
      const items = twee ? twee.definitions : scanScript(document.getText())
      for (const item of items) definitions.push({ ...item, uri })
      for (const passage of twee?.passages ?? []) passages.push({ ...passage, uri })
      if (!twee) {
        for (const definition of scanScriptFunctions(document.getText()))
          functions.push({ ...definition, uri })
      }
    }
    this.definitions = definitions
    this.passages = passages
    this.functions = functions
    this.known = knownNames(definitions)
    this.kinds = macroKinds(definitions)
    for (const document of vscode.workspace.textDocuments) this.validate(document)
    this.emitter.fire()
  }

  /** 校验单个 Twee 文档：未定义宏、缺失 Passage、特殊 Passage 带 Tag、宏闭合不匹配。 */
  validate(document) {
    if (document.languageId !== "narrava-twee") return
    const twee = scanTwee(document.getText())
    const calls = twee.calls
    const errors = calls
      .filter((call) => !call.closing && !this.known.has(call.name))
      .map((call) => {
        const range = new vscode.Range(
          document.positionAt(call.start),
          document.positionAt(call.start + call.length),
        )
        const diagnostic = new vscode.Diagnostic(
          range,
          `Macro \`${call.name}\` 未定义`,
          vscode.DiagnosticSeverity.Error,
        )
        diagnostic.code = "narrava.macro.undefined"
        diagnostic.source = "Narrava Loom"
        return diagnostic
      })
    for (const link of missingPassageLinks(twee.links, this.passages)) {
      const range = new vscode.Range(
        document.positionAt(link.start),
        document.positionAt(link.start + link.length),
      )
      const diagnostic = new vscode.Diagnostic(
        range,
        `Passage \`${link.target}\` 未定义`,
        vscode.DiagnosticSeverity.Error,
      )
      diagnostic.code = "narrava.passage.undefined"
      diagnostic.source = "Narrava Loom"
      errors.push(diagnostic)
    }
    for (const passage of taggedSpecialPassages(twee.passages)) {
      const start = passage.tagsStart ?? passage.start
      const length = passage.tagsLength || passage.length
      const range = new vscode.Range(
        document.positionAt(start),
        document.positionAt(start + length),
      )
      const diagnostic = new vscode.Diagnostic(
        range,
        `特殊 Passage \`${passage.name}\` 不能带有 Tag`,
        vscode.DiagnosticSeverity.Error,
      )
      diagnostic.code = "narrava.passage.special_tags"
      diagnostic.source = "Narrava Loom"
      errors.push(diagnostic)
    }
    const stack = []
    for (const call of calls) {
      const kind = this.kinds.get(call.name)
      if (!this.known.has(call.name) || kind === "clause") continue
      if (!call.closing) {
        if (kind === "container") stack.push(call)
        continue
      }
      if (kind === "inline") {
        errors.push(
          this.bodyDiagnostic(
            document,
            call,
            `Inline Macro \`${call.name}\` 不能使用闭合标签`,
            "narrava.macro.inline_closed",
          ),
        )
        continue
      }
      const opening = stack.pop()
      if (!opening || opening.name !== call.name) {
        errors.push(
          this.bodyDiagnostic(
            document,
            call,
            `Macro 闭合不匹配：\`${call.name}\``,
            "narrava.macro.closing_mismatch",
          ),
        )
      }
    }
    for (const call of stack) {
      errors.push(
        this.bodyDiagnostic(
          document,
          call,
          `Container Macro \`${call.name}\` 缺少 <</${call.name}>>`,
          "narrava.macro.container_unclosed",
        ),
      )
    }
    this.diagnostics.set(document.uri, errors)
  }

  /** 按调用位置构造一条带稳定 code/source 的诊断。 */
  bodyDiagnostic(document, call, message, code) {
    const range = new vscode.Range(
      document.positionAt(call.start),
      document.positionAt(call.start + call.length),
    )
    const diagnostic = new vscode.Diagnostic(range, message, vscode.DiagnosticSeverity.Error)
    diagnostic.code = code
    diagnostic.source = "Narrava Loom"
    return diagnostic
  }

  /** 释放诊断集合与事件发射器。 */
  dispose() {
    this.diagnostics.dispose()
    this.emitter.dispose()
  }
}

module.exports = { MacroWorkspace }

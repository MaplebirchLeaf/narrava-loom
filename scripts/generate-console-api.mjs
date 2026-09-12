import { readFileSync, writeFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { API } from "typescript/unstable/sync"

// 作者声明同时服务 VS Code 和游戏控制台，保留泛型实例化后的参数类型。
const root = new URL("../", import.meta.url)
const file = fileURLToPath(new URL("bindings/typescript/narrava.d.ts", root))
const source = readFileSync(file, "utf8")
const contract = JSON.parse(readFileSync(new URL("bindings/script-contract.json", root), "utf8"))
const api = new API({ cwd: fileURLToPath(root) })
const output = {}
try {
  const snapshot = api.updateSnapshot({ openFiles: [file] })
  const checker = snapshot.getDefaultProjectForFile(file).checker
  function visit(symbol, path, depth) {
    const type = checker.getTypeOfSymbol(symbol)
    if (!type) throw new Error(`Missing API type: ${path}`)
    const signature = checker.typeToString(type)
    const help =
      symbol.getDocumentationComment(checker) ||
      type.getSymbol()?.getDocumentationComment(checker) ||
      ""
    const examples = symbol
      .getJsDocTags(checker)
      .filter((tag) => tag.name === "example")
      .map((tag) => tag.text)
      .filter(Boolean)
    output[path] = {
      signature,
      help: [help, ...examples.map((example) => `示例 / Example: ${example}`)]
        .filter(Boolean)
        .join("\n"),
    }
    if (depth < 3 && !checker.getSignaturesOfType(type, 0).length && type.isObjectType()) {
      for (const member of checker.getPropertiesOfType(type))
        visit(member, `${path}.${member.name}`, depth + 1)
    }
  }
  for (const name of contract.globals) {
    const position = source.indexOf(`const ${name}:`)
    if (position < 0) throw new Error(`Missing global: ${name}`)
    const symbol = checker.getSymbolAtPosition(file, position + 6)
    if (!symbol) throw new Error(`Unresolved global: ${name}`)
    visit(symbol, name, 0)
  }
  snapshot.dispose()
} finally {
  api.close()
}
const path = new URL("crates/narrava-loom-script/bootstrap/console-api.generated.json", root)
const content = `${JSON.stringify(output, null, 2)}\n`
if (process.argv.includes("--check")) {
  if (readFileSync(path, "utf8") !== content)
    throw new Error("Console API metadata is stale; run bun run console:generate")
} else writeFileSync(path, content)

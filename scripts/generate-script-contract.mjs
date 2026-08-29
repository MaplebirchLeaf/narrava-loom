import { readFileSync, writeFileSync } from "node:fs"

const root = new URL("../", import.meta.url)
const contract = JSON.parse(readFileSync(new URL("bindings/script-contract.json", root), "utf8"))

const union = (values) =>
  values.length <= 3
    ? values.map((value) => JSON.stringify(value)).join(" | ")
    : `\n${values.map((value) => `    | ${JSON.stringify(value)}`).join("\n")}`
const rustSlice = (name, values) =>
  `pub const ${name}: &[&str] = &[${values.map((value) => JSON.stringify(value)).join(", ")}];\n`
const runtimeTypes = Object.entries(contract.runtimeTypes)
  .map(([name, definition]) => `  type ${name} = ${definition}`)
  .join("\n")

const typescript = `/** Generated from bindings/script-contract.json. Do not edit by hand. */
declare global {
  type NarravaScriptGlobalName =${union(contract.globals)}
  type NarravaBuiltinEventName =${union(contract.builtinEvents)}
  type NarravaSurfaceBuilderName =${union(contract.surfaceBuilders)}
  type NarravaRuntimeCommandType =${union(contract.runtimeProtocol.commands)}
  type NarravaRuntimeUpdateType = ${union(contract.runtimeProtocol.updates)}
  type NarravaPendingOperationType = ${union(contract.runtimeProtocol.pendingOperations)}
  type NarravaSurfaceNodeType =${union(contract.surfaceNodes)}
${runtimeTypes}
}

export {}
`

const rust = `// Generated from bindings/script-contract.json. Do not edit by hand.

pub const RUNTIME_PROTOCOL_VERSION: u16 = ${contract.version};
${rustSlice("GLOBALS", contract.globals)}${rustSlice("BUILTIN_EVENTS", contract.builtinEvents)}${rustSlice("SURFACE_BUILDERS", contract.surfaceBuilders)}${rustSlice("RUNTIME_COMMANDS", contract.runtimeProtocol.commands)}${rustSlice("RUNTIME_UPDATES", contract.runtimeProtocol.updates)}${rustSlice("PENDING_OPERATIONS", contract.runtimeProtocol.pendingOperations)}${rustSlice("SURFACE_NODES", contract.surfaceNodes)}`

const outputs = [
  [new URL("bindings/typescript/narrava-contract.generated.d.ts", root), typescript],
  [new URL("crates/narrava-loom-protocol/src/contract_generated.rs", root), rust],
]

if (process.argv.includes("--check")) {
  for (const [path, expected] of outputs) {
    if (readFileSync(path, "utf8") !== expected) {
      throw new Error(`${path.pathname} 未同步；请运行 bun run contract:generate`)
    }
  }
} else {
  for (const [path, content] of outputs) writeFileSync(path, content)
}

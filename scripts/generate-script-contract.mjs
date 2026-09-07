import { readFileSync, writeFileSync } from "node:fs"

const root = new URL("../", import.meta.url)
const contract = JSON.parse(readFileSync(new URL("bindings/script-contract.json", root), "utf8"))

const union = (values) =>
  values.length <= 3
    ? values.map((value) => JSON.stringify(value)).join(" | ")
    : `\n${values.map((value) => `    | ${JSON.stringify(value)}`).join("\n")}`
const rustSlice = (name, values) =>
  `pub const ${name}: &[&str] = &[${values.map((value) => JSON.stringify(value)).join(", ")}];\n`
// Rust Protocol declarations own wire fields; this generator accepts their small DTO subset.
const protocol = readFileSync(
  new URL("crates/narrava-loom-protocol/src/lib.rs", root),
  "utf8",
).replace(/\/\/[^\n]*/g, "")
const declarations = new Map()
const declarationPattern = /((?:#\[[^\]]*\]\s*)+)pub (struct|enum) (\w+)\s*\{/g
for (const match of protocol.matchAll(declarationPattern)) {
  let end = match.index + match[0].length
  let depth = 1
  const start = end
  while (depth > 0 && end < protocol.length) {
    if (protocol[end] === "{") depth++
    if (protocol[end] === "}") depth--
    end++
  }
  if (depth !== 0) throw new Error(`Unclosed Protocol declaration: ${match[3]}`)
  declarations.set(match[3], {
    kind: match[2],
    attributes: match[1],
    body: protocol.slice(start, end - 1),
  })
}
const camelCase = (name) => name[0].toLowerCase() + name.slice(1)
const typeName = (name) =>
  name === "SaveOperation" ? "NarravaRuntimeSaveOperation" : `Narrava${name}`
const tsType = (rustType) => {
  if (rustType === "String" || rustType === "RuntimeSessionId") return "string"
  if (rustType === "bool") return "boolean"
  if (/^u(8|16|32|64)$/.test(rustType)) return "number"
  if (rustType === "serde_json::Value") return "unknown"
  const vector = /^Vec<(.+)>$/.exec(rustType)
  if (vector) return `readonly ${tsType(vector[1])}[]`
  const optional = /^Option<(.+)>$/.exec(rustType)
  if (optional) return `${tsType(optional[1])} | null`
  if (declarations.has(rustType)) return typeName(rustType)
  throw new Error(`Unsupported Protocol type: ${rustType}`)
}
function fields(body) {
  const members = []
  const rest = body
    .replace(
      /((?:#\[[^\]]*\]\s*)*)(?:pub\s+)?(\w+):\s*([\w:]+(?:<[^,]+>)?)\s*(?:,|$)/g,
      (_match, attributes, name, type) => {
        const rename = /rename\s*=\s*"([^"]+)"/.exec(attributes)
        const optional = /^Option<(.+)>$/.exec(type)
        const omitted = optional && attributes.includes('skip_serializing_if = "Option::is_none"')
        const fieldType =
          name === "protocol_version"
            ? String(contract.version)
            : tsType(omitted ? optional[1] : type)
        members.push(`readonly ${rename?.[1] ?? name}${omitted ? "?" : ""}: ${fieldType}`)
        return ""
      },
    )
    .trim()
  if (rest) throw new Error(`Unsupported Protocol fields: ${rest}`)
  return members
}
function variants(name) {
  const declaration = declarations.get(name)
  if (
    !declaration ||
    declaration.kind !== "enum" ||
    !declaration.attributes.includes('rename_all = "camelCase"')
  )
    throw new Error(`Unsupported Protocol enum: ${name}`)
  const values = []
  const rest = declaration.body
    .replace(
      /(?:#\[default\]\s*)?(\w+)(?:\s*\{([^}]*)\}|\(([^)]+)\))?\s*,/g,
      (_match, variant, body, tuple) => {
        values.push({
          tag: camelCase(variant),
          fields: body === undefined ? [] : fields(body),
          tuple,
        })
        return ""
      },
    )
    .trim()
  if (rest) throw new Error(`Unsupported Protocol variants: ${rest}`)
  return values
}
const runtimeProtocol = {
  commands: variants("RuntimeCommand").map(({ tag }) => tag),
  updates: variants("RuntimeUpdate").map(({ tag }) => tag),
  pendingOperations: variants("PendingOperation").map(({ tag }) => tag),
}
const surfaceNodes = variants("HostNodeDto").map(({ tag }) => tag)
const runtimeTypes = [...declarations]
  .map(([name, declaration]) => {
    if (declaration.kind === "struct")
      return `  type ${typeName(name)} = { ${fields(declaration.body).join("; ")} }`
    const tagField = /tag\s*=\s*"([^"]+)"/.exec(declaration.attributes)?.[1]
    const contentField = /content\s*=\s*"([^"]+)"/.exec(declaration.attributes)?.[1]
    const definition = variants(name)
      .map(({ tag, fields: members, tuple }) => {
        if (tuple) {
          if (!contentField || !tagField) throw new Error(`Unsupported tuple variant: ${name}`)
          members.push(`readonly ${contentField}: ${tsType(tuple)}`)
        }
        return tagField
          ? `{ ${[`readonly ${tagField}: ${JSON.stringify(tag)}`, ...members].join("; ")} }`
          : JSON.stringify(tag)
      })
      .join(" | ")
    return `  type ${typeName(name)} = ${definition}`
  })
  .join("\n")

const typescript = `/** Generated from Protocol Rust declarations and bindings/script-contract.json. Do not edit by hand. */
declare global {
  type NarravaScriptGlobalName =${union(contract.globals)}
  type NarravaBuiltinEventName =${union(contract.builtinEvents)}
  type NarravaSurfaceBuilderName =${union(contract.surfaceBuilders)}
  type NarravaRuntimeCommandType =${union(runtimeProtocol.commands)}
  type NarravaRuntimeUpdateType = ${union(runtimeProtocol.updates)}
  type NarravaPendingOperationType = ${union(runtimeProtocol.pendingOperations)}
  type NarravaSurfaceNodeType =${union(surfaceNodes)}
${runtimeTypes}
}

export {}
`

const rust = `// Generated from Protocol Rust declarations and bindings/script-contract.json. Do not edit by hand.

pub const RUNTIME_PROTOCOL_VERSION: u16 = ${contract.version};
${rustSlice("GLOBALS", contract.globals)}${rustSlice("BUILTIN_EVENTS", contract.builtinEvents)}${rustSlice("SURFACE_BUILDERS", contract.surfaceBuilders)}${rustSlice("RUNTIME_COMMANDS", runtimeProtocol.commands)}${rustSlice("RUNTIME_UPDATES", runtimeProtocol.updates)}${rustSlice("PENDING_OPERATIONS", runtimeProtocol.pendingOperations)}${rustSlice("SURFACE_NODES", surfaceNodes)}`

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

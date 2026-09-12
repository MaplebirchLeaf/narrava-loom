import type {} from "../../../bindings/typescript/narrava-contract.generated"
import api from "./console-api.generated.json"

const globals = globalThis
const stateProxies = new WeakSet<object>()
export function registerStateProxy(value: object): void {
  stateProxies.add(value)
}
function opaqueProxy(value: unknown): boolean {
  return (
    typeof __narravaConsoleIsProxy === "function" &&
    __narravaConsoleIsProxy(value) &&
    !stateProxies.has(value as object)
  )
}
const descriptions: Record<string, { signature: string; help: string }> = api

/** 属性描述符预览，不执行普通 getter；循环与大对象保持有界。 */
export function inspectConsoleResult(value: unknown, path = ""): NarravaHostDebugValueDto {
  const seen = new WeakMap<object, string>()
  let remaining = 400
  function inspect(
    item: unknown,
    name: string,
    current: string,
    depth: number,
  ): NarravaHostDebugValueDto {
    const description = descriptions[current]
    if (opaqueProxy(item))
      return {
        name,
        kind: "proxy",
        preview: "[Proxy]",
        signature: "",
        help: "未执行代理 trap / Proxy traps are not evaluated",
        children: [],
        truncated: false,
      }
    const kind = item === null ? "null" : Array.isArray(item) ? "array" : typeof item
    const node = {
      name,
      kind,
      preview: "",
      signature: description?.signature ?? "",
      help: description?.help ?? "",
      children: [] as NarravaHostDebugValueDto[],
      truncated: false,
    }
    if (item instanceof Promise) throw new TypeError("Promise 尚未结算 / Promise has not settled")
    if (kind === "function") {
      node.preview = `ƒ ${name || "anonymous"}${node.signature ? ` ${node.signature}` : "(…)"}`
      return node
    }
    if (item === null || typeof item !== "object") {
      node.preview = typeof item === "string" ? JSON.stringify(item.slice(0, 4096)) : String(item)
      node.truncated = typeof item === "string" && item.length > 4096
      return node
    }
    if (seen.has(item)) {
      node.kind = "reference"
      node.preview = `↩ ${seen.get(item)}`
      return node
    }
    seen.set(item, current || "result")
    const keys = Object.keys(item)
    node.preview = Array.isArray(item)
      ? `Array(${item.length})`
      : `${current || "Object"} {${keys.length}}`
    if (depth >= 5 || remaining <= 0) {
      node.truncated = keys.length > 0
      return node
    }
    for (const key of keys.slice(0, 80)) {
      if (--remaining < 0) break
      const descriptor = Object.getOwnPropertyDescriptor(item, key)
      const childPath = current ? `${current}.${key}` : key
      node.children.push(
        descriptor && "value" in descriptor
          ? inspect(descriptor.value, key, childPath, depth + 1)
          : {
              name: key,
              kind: "accessor",
              preview: "[Getter]",
              signature: "",
              help: "未执行访问器 / Getter was not invoked",
              children: [],
              truncated: false,
            },
      )
    }
    node.truncated = node.children.length < keys.length
    return node
  }
  // 按实际对象身份识别 API；变量别名同样可获得说明。
  if (!path)
    for (const name of Object.keys(descriptions).filter((candidate) => !candidate.includes("."))) {
      if (Object.getOwnPropertyDescriptor(globals, name)?.value === value) {
        path = name
        break
      }
    }
  return inspect(value, path, path, 0)
}

/** 仅解析成员路径，绝不通过 eval 计算补全目标。 */
export function completeConsole(path: string): NarravaHostDebugValueDto[] {
  if (path.length > 512 || (path && !/^[\w$]+(?:\.[\w$]+)*$/.test(path))) return []
  let value: unknown = globals
  for (const name of path ? path.split(".") : []) {
    if ((typeof value !== "object" && typeof value !== "function") || value === null) return []
    if (opaqueProxy(value)) return []
    const descriptor = Object.getOwnPropertyDescriptor(value, name)
    if (!descriptor || !("value" in descriptor)) return []
    value = descriptor.value
  }
  if ((typeof value !== "object" && typeof value !== "function") || value === null) return []
  if (opaqueProxy(value)) return []
  return Object.getOwnPropertyNames(value)
    .filter((name) => !name.startsWith("__") && /^[\w$]+$/.test(name))
    .slice(0, 200)
    .map((name) => {
      const descriptor = Object.getOwnPropertyDescriptor(value, name)
      const key = path ? `${path}.${name}` : name
      const description = descriptions[key]
      return {
        name,
        kind: descriptor && "value" in descriptor ? typeof descriptor.value : "accessor",
        preview: "",
        signature: description?.signature ?? "",
        help:
          description?.help ??
          (descriptor && "value" in descriptor
            ? "运行时成员 / Runtime member"
            : "访问器，补全不会执行 / Getter is not evaluated"),
        children: [],
        truncated: false,
      }
    })
}

import { decodeScriptValue, encodeScriptValue, scriptGlobals } from "./internal"

function namespaceAccess(namespace: string) {
  return {
    get: (key: string) => decodeScriptValue(__narravaStateGet(namespace, key)),
    has: (key: string) => __narravaStateHas(namespace, key),
    set: (key: string, value: unknown) =>
      decodeScriptValue(__narravaStateSet(namespace, key, encodeScriptValue(key, value))),
    del: (key: string) => decodeScriptValue(__narravaStateDel(namespace, key)),
    extend: (values: Record<string, unknown>) => {
      let inserted = 0
      let replaced = 0
      for (const [key, value] of Object.entries(values)) {
        if (__narravaStateHas(namespace, key)) replaced++
        else inserted++
        __narravaStateSet(namespace, key, encodeScriptValue(key, value))
      }
      return { inserted, replaced }
    },
  }
}

function stateProxy(namespace: string): Record<string, unknown> {
  return new Proxy(Object.create(null) as Record<string, unknown>, {
    get: (_target, key) =>
      typeof key === "string" ? decodeScriptValue(__narravaStateGet(namespace, key)) : undefined,
    set: (_target, key, value) => {
      if (typeof key !== "string") return false
      __narravaStateSet(namespace, key, encodeScriptValue(key, value))
      return true
    },
    deleteProperty: (_target, key) => {
      if (typeof key !== "string") return false
      __narravaStateDel(namespace, key)
      return true
    },
    has: (_target, key) => typeof key === "string" && __narravaStateHas(namespace, key),
    ownKeys: () => Object.keys(__narravaStateSnapshot(namespace)),
    getOwnPropertyDescriptor: (_target, key) => {
      if (typeof key !== "string" || !__narravaStateHas(namespace, key)) return undefined
      return {
        configurable: true,
        enumerable: true,
        writable: true,
        value: decodeScriptValue(__narravaStateGet(namespace, key)),
      }
    },
  })
}

export default function state(): void {
  scriptGlobals.State = Object.seal({
    global: namespaceAccess("global"),
    variables: namespaceAccess("variables"),
    temporary: namespaceAccess("temporary"),
    setup: {
      get: () => decodeScriptValue(__narravaStateGet("setup", "")),
      set: (value: unknown) =>
        decodeScriptValue(__narravaStateSet("setup", "", encodeScriptValue("setup", value))),
    },
  })
  scriptGlobals.V = stateProxy("variables")
  scriptGlobals.T = stateProxy("temporary")
  scriptGlobals.setup = stateProxy("setup-properties")
}

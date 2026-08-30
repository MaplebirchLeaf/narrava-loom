import { fromHostValue, globals, toHostValue } from "./internal"

function namespaceApi(namespace: string) {
  return {
    get: (key: string) => fromHostValue(__narravaStateGet(namespace, key)),
    has: (key: string) => __narravaStateHas(namespace, key),
    set: (key: string, value: unknown) =>
      fromHostValue(__narravaStateSet(namespace, key, toHostValue(key, value))),
    del: (key: string) => fromHostValue(__narravaStateDel(namespace, key)),
    extend: (values: Record<string, unknown>) => {
      let inserted = 0
      let replaced = 0
      for (const [key, value] of Object.entries(values)) {
        if (__narravaStateHas(namespace, key)) replaced++
        else inserted++
        __narravaStateSet(namespace, key, toHostValue(key, value))
      }
      return { inserted, replaced }
    },
  }
}

function propertyApi(namespace: string): Record<string, unknown> {
  return new Proxy(Object.create(null) as Record<string, unknown>, {
    get: (_target, key) =>
      typeof key === "string" ? fromHostValue(__narravaStateGet(namespace, key)) : undefined,
    set: (_target, key, value) => {
      if (typeof key !== "string") return false
      __narravaStateSet(namespace, key, toHostValue(key, value))
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
        value: fromHostValue(__narravaStateGet(namespace, key)),
      }
    },
  })
}

export function installState(): void {
  globals.State = Object.seal({
    global: namespaceApi("global"),
    variables: namespaceApi("variables"),
    temporary: namespaceApi("temporary"),
    setup: {
      get: () => fromHostValue(__narravaStateGet("setup", "")),
      set: (value: unknown) =>
        fromHostValue(__narravaStateSet("setup", "", toHostValue("setup", value))),
    },
  })
  globals.V = propertyApi("variables")
  globals.T = propertyApi("temporary")
  globals.setup = propertyApi("setup-properties")
}

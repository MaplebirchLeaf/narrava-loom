import { allocateSubscription, globals, macros, subscriptions } from "./internal"

export function installMacro(): void {
  globals.Macro = Object.seal({
    add: (name: string, value: { handler: (call: unknown) => unknown }) => {
      const old = macros.get(name)
      macros.set(name, value)
      return old
    },
    update: (name: string, value: { handler: (call: unknown) => unknown }) => {
      if (!macros.has(name)) throw new Error(`Macro 不存在：${name}`)
      const old = macros.get(name)
      macros.set(name, value)
      return old
    },
    del: (name: string) => {
      const old = macros.get(name)
      macros.delete(name)
      return old
    },
    get: (name: string) => macros.get(name),
    has: (name: string) => macros.has(name),
    before: (name: string, hook: unknown) => subscribe("before", name, hook),
    after: (name: string, hook: unknown) => subscribe("after", name, hook),
    off: (id: number) => subscriptions.delete(id),
  })
}

function subscribe(kind: "before" | "after", name: string, hook: unknown): number {
  const id = allocateSubscription()
  subscriptions.set(id, { kind, name, hook })
  return id
}

import { macroDefinitions, macroHooks, scriptGlobals, subscriptionId } from "./internal"

export default function macro(): void {
  scriptGlobals.Macro = Object.seal({
    add: (name: string, value: { handler: (call: unknown) => unknown }) => {
      const old = macroDefinitions.get(name)
      macroDefinitions.set(name, value)
      return old
    },
    update: (name: string, value: { handler: (call: unknown) => unknown }) => {
      if (!macroDefinitions.has(name)) throw new Error(`Macro 不存在：${name}`)
      const old = macroDefinitions.get(name)
      macroDefinitions.set(name, value)
      return old
    },
    del: (name: string) => {
      const old = macroDefinitions.get(name)
      macroDefinitions.delete(name)
      return old
    },
    get: (name: string) => macroDefinitions.get(name),
    has: (name: string) => macroDefinitions.has(name),
    before: (name: string, hook: unknown) => registerHook("before", name, hook),
    after: (name: string, hook: unknown) => registerHook("after", name, hook),
    off: (id: number) => macroHooks.delete(id),
  })
}

function registerHook(kind: "before" | "after", name: string, hook: unknown): number {
  const id = subscriptionId()
  macroHooks.set(id, { kind, name, hook })
  return id
}

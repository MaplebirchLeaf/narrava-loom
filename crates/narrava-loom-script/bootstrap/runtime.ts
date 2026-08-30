import { emitBuiltin, emitReaction, takeAuthorEvents } from "./event"
import { resolveHostOperation, takeHostOperation } from "./host"
import {
  configuration,
  events,
  functions,
  globals,
  logs,
  macros,
  type BootstrapContract,
} from "./internal"
import { saveAfter } from "./save"

interface RuntimeInternal extends Record<string, unknown> {
  engine: unknown
  save: unknown
  configure: (value: Partial<typeof configuration>) => void
  emitBuiltin: (name: string, payload: unknown) => number
  emitReaction: (name: string, payload: unknown) => number
  takeAuthorEvents: typeof takeAuthorEvents
  completeSave: (completion: Parameters<typeof saveAfter>[0]) => void
  takeSave: () => unknown
  hasMacro: (name: string) => boolean
  invokeMacro: (name: string, call: unknown) => unknown
  takeHostOperation: typeof takeHostOperation
  resolveHostOperation: typeof resolveHostOperation
  call: (id: number, arguments_: unknown[]) => unknown
}

export function installRuntime(contract: BootstrapContract): void {
  const runtime: RuntimeInternal = {
    engine: null,
    save: null,
    events,
    logs,
    macros,
    configure(value) {
      Object.assign(configuration, value)
    },
    emitBuiltin,
    emitReaction,
    takeAuthorEvents,
    completeSave: saveAfter,
    takeSave() {
      const request = this.save
      this.save = null
      return request
    },
    hasMacro: (name) => macros.has(name),
    invokeMacro: (name, call) => macros.get(name)!.handler(call),
    takeHostOperation,
    resolveHostOperation,
    call: (id, arguments_) => functions.get(id)!(...arguments_),
  }
  globals.__narrava = runtime

  for (const name of contract.globals) {
    if (globals[name as keyof typeof globals] === undefined) {
      throw new Error(`Script Contract 缺少全局：${name}`)
    }
  }
  for (const name of contract.surfaceBuilders) {
    if (typeof globals.Surface?.[name] !== "function") {
      throw new Error(`Script Contract 缺少 Surface builder：${name}`)
    }
  }
}

import { drainAuthorEvents, publishBuiltin, publishReaction } from "./event"
import { claimHostOperation, completeHostOperation } from "./host"
import {
  eventRecords,
  logRecords,
  macroDefinitions,
  runtimeConfiguration,
  scriptFunctions,
  scriptGlobals,
  type BootstrapContract,
} from "./internal"
import { finishSave } from "./save"

interface NativeBridge extends Record<string, unknown> {
  engine: unknown
  save: unknown
  configure: (value: Partial<typeof runtimeConfiguration>) => void
  emitBuiltin: (name: string, payload: unknown) => number
  emitReaction: (name: string, payload: unknown) => number
  takeAuthorEvents: typeof drainAuthorEvents
  completeSave: (completion: Parameters<typeof finishSave>[0]) => void
  takeSave: () => unknown
  hasMacro: (name: string) => boolean
  invokeMacro: (name: string, call: unknown) => unknown
  takeHostOperation: typeof claimHostOperation
  resolveHostOperation: typeof completeHostOperation
  call: (id: number, arguments_: unknown[]) => unknown
}

export default function runtime(contract: BootstrapContract): void {
  const bridge: NativeBridge = {
    engine: null,
    save: null,
    events: eventRecords,
    logs: logRecords,
    macros: macroDefinitions,
    configure(value) {
      Object.assign(runtimeConfiguration, value)
    },
    emitBuiltin: publishBuiltin,
    emitReaction: publishReaction,
    takeAuthorEvents: drainAuthorEvents,
    completeSave: finishSave,
    takeSave() {
      const request = this.save
      this.save = null
      return request
    },
    takeLanguage() {
      const request = this.language
      this.language = null
      return request
    },
    hasMacro: (name) => macroDefinitions.has(name),
    invokeMacro: (name, call) => macroDefinitions.get(name)!.handler(call),
    takeHostOperation: claimHostOperation,
    resolveHostOperation: completeHostOperation,
    call: (id, arguments_) => scriptFunctions.get(id)!(...arguments_),
  }
  scriptGlobals.__narrava = bridge

  for (const name of contract.globals) {
    if (scriptGlobals[name as keyof typeof scriptGlobals] === undefined) {
      throw new Error(`Script Contract 缺少全局：${name}`)
    }
  }
  for (const name of contract.surfaceBuilders) {
    if (typeof scriptGlobals.Surface?.[name] !== "function") {
      throw new Error(`Script Contract 缺少 Surface builder：${name}`)
    }
  }
}

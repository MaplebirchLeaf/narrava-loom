export interface CallableReference {
  readonly __narravaCallable: number
  readonly name: string
}

export interface RuntimeConfiguration {
  story: {
    passages: Array<{ name: string; tags?: string[] }>
    current: string | null
    visits: Record<string, number>
  }
  defaultLocale: string
  locale: string
  i18nExport: string
}

export interface BootstrapContract {
  globals: string[]
  builtinEvents: string[]
  surfaceBuilders: string[]
}

export interface ScriptGlobalRegistry {
  State?: unknown
  V?: unknown
  T?: unknown
  setup?: unknown
  Reaction?: unknown
  Macro?: unknown
  Logger?: unknown
  Event?: unknown
  Host?: unknown
  Engine?: unknown
  Story?: unknown
  Save?: unknown
  Resource?: unknown
  I18n?: unknown
  Surface?: Record<string, unknown>
  __narrava?: Record<string, unknown>
}

export const scriptGlobals = globalThis as unknown as ScriptGlobalRegistry
export const scriptFunctions = new Map<number, (...arguments_: unknown[]) => unknown>()
export const eventRecords: EventRecord[] = []
export const authorEventQueue: EventRecord[] = []
export const logRecords: unknown[] = []
export const macroDefinitions = new Map<string, { handler: (call: unknown) => unknown }>()
export const macroHooks = new Map<number, unknown>()
export const saveHooks = new Map<number, SaveHook>()
export const hostOperationQueue = new Map<number, HostOperation>()
export const eventSubscriptions = new Map<number, EventSubscription>()
export const runtimeConfiguration: RuntimeConfiguration = {
  story: { passages: [], current: null, visits: {} },
  defaultLocale: "und",
  locale: "und",
  i18nExport: "{}",
}

export interface EventRecord {
  sequence: number
  name: string
  payload: unknown
}

export interface EventSubscription {
  name?: string
  pending: EventRecord[]
}

export interface SaveHook {
  stage: "before" | "after"
  operation: string
  callback: (value: unknown) => unknown
}

export interface HostOperation {
  id: number
  kind: "delay"
  milliseconds: number
  resolve: () => void
  taken: boolean
}

let nextFunctionId = 1
let nextSubscriptionId = 1
let nextHostOperationId = 1
let nextEventSequence = 1

export function subscriptionId(): number {
  return nextSubscriptionId++
}

export function hostOperationId(): number {
  return nextHostOperationId++
}

export function eventSequence(): number {
  return nextEventSequence++
}

export function encodeScriptValue(name: string, value: unknown): unknown {
  if (typeof value !== "function") return value
  const id = nextFunctionId++
  scriptFunctions.set(id, value as (...arguments_: unknown[]) => unknown)
  return { __narravaCallable: id, name } satisfies CallableReference
}

export function decodeScriptValue(value: unknown): unknown {
  const reference = value as Partial<CallableReference> | null | undefined
  return reference?.__narravaCallable === undefined
    ? value
    : scriptFunctions.get(reference.__narravaCallable)
}

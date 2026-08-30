export interface CallableMarker {
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

export interface BootstrapGlobals {
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

export const globals = globalThis as unknown as BootstrapGlobals
export const functions = new Map<number, (...arguments_: unknown[]) => unknown>()
export const events: unknown[] = []
export const logs: unknown[] = []
export const macros = new Map<string, { handler: (call: unknown) => unknown }>()
export const subscriptions = new Map<number, unknown>()
export const saveHooks = new Map<number, SaveHook>()
export const hostOperations = new Map<number, HostOperation>()
export const eventSubscriptions = new Map<number, EventSubscription>()
export const configuration: RuntimeConfiguration = {
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

let nextFunction = 1
let nextSubscription = 1
let nextHostOperation = 1
let nextEventSequence = 1

export function allocateSubscription(): number {
  return nextSubscription++
}

export function allocateHostOperation(): number {
  return nextHostOperation++
}

export function allocateEventSequence(): number {
  return nextEventSequence++
}

export function toHostValue(name: string, value: unknown): unknown {
  if (typeof value !== "function") return value
  const id = nextFunction++
  functions.set(id, value as (...arguments_: unknown[]) => unknown)
  return { __narravaCallable: id, name } satisfies CallableMarker
}

export function fromHostValue(value: unknown): unknown {
  const marker = value as Partial<CallableMarker> | null | undefined
  return marker?.__narravaCallable === undefined ? value : functions.get(marker.__narravaCallable)
}

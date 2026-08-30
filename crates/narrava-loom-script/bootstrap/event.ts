import {
  allocateEventSequence,
  allocateSubscription,
  eventSubscriptions,
  events,
  globals,
  type EventRecord,
} from "./internal"

let builtinEvents = new Set<string>()

export function installEvent(names: string[]): void {
  builtinEvents = new Set(names)
  globals.Event = Object.seal({
    emit: (name: string, payload: unknown = undefined) => {
      if (typeof name !== "string" || name.length === 0 || /\s/u.test(name)) {
        throw new TypeError("Event 名称不能为空或包含空白")
      }
      if (builtinEvents.has(name)) throw new TypeError(`Event 内置名称只能由 Engine 发出：${name}`)
      return emitEvent(name, payload)
    },
    subscribe: (filter: { name?: string } = {}) => {
      const id = allocateSubscription()
      eventSubscriptions.set(id, { name: filter.name, pending: [] })
      return id
    },
    take: (id: number) => {
      const subscription = eventSubscriptions.get(id)
      return subscription?.pending.splice(0)
    },
    unsubscribe: (id: number) => eventSubscriptions.delete(id),
  })
}

export function emitEvent(name: string, payload: unknown): number {
  const record: EventRecord = { sequence: allocateEventSequence(), name, payload }
  events.push(record)
  for (const subscription of eventSubscriptions.values()) {
    if (subscription.name === undefined || subscription.name === name) {
      subscription.pending.push(record)
    }
  }
  return record.sequence
}

export function emitBuiltin(name: string, payload: unknown): number {
  if (!builtinEvents.has(name)) throw new TypeError(`未知 Event 内置名称：${name}`)
  return emitEvent(name, payload)
}

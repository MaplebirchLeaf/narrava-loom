import {
  authorEventQueue,
  eventSequence,
  eventSubscriptions,
  eventRecords,
  scriptGlobals,
  subscriptionId,
  type EventRecord,
} from "./internal"

let builtinEvents = new Set<string>()

export default function events(names: string[]): void {
  builtinEvents = new Set(names)
  scriptGlobals.Event = Object.seal({
    emit: (name: string, payload: unknown = undefined) => {
      if (typeof name !== "string" || name.length === 0 || /\s/u.test(name)) {
        throw new TypeError("Event 名称不能为空或包含空白")
      }
      if (builtinEvents.has(name)) throw new TypeError(`Event 内置名称只能由 Engine 发出：${name}`)
      const sequence = publishEvent(name, payload)
      authorEventQueue.push(eventRecords.at(-1)!)
      return sequence
    },
    subscribe: (filter: { name?: string } = {}) => {
      const id = subscriptionId()
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

function publishEvent(name: string, payload: unknown): number {
  const record: EventRecord = { sequence: eventSequence(), name, payload }
  eventRecords.push(record)
  for (const subscription of eventSubscriptions.values()) {
    if (subscription.name === undefined || subscription.name === name) {
      subscription.pending.push(record)
    }
  }
  return record.sequence
}

export function publishBuiltin(name: string, payload: unknown): number {
  if (!builtinEvents.has(name)) throw new TypeError(`未知 Event 内置名称：${name}`)
  return publishEvent(name, payload)
}

export function publishReaction(name: string, payload: unknown): number {
  return publishEvent(name, payload)
}

export function drainAuthorEvents(): EventRecord[] {
  return authorEventQueue.splice(0)
}

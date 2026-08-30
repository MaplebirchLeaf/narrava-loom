import {
  allocateHostOperation,
  configuration,
  globals,
  hostOperations,
  type HostOperation,
} from "./internal"

export function installHost(): void {
  globals.Host = Object.freeze({
    delay: (milliseconds: number) => {
      if (!Number.isFinite(milliseconds) || milliseconds < 0 || milliseconds > 86_400_000) {
        throw new RangeError("Host.delay 毫秒数必须在 0 到 86400000 之间")
      }
      const id = allocateHostOperation()
      return new Promise<void>((resolve) =>
        hostOperations.set(id, {
          id,
          kind: "delay",
          milliseconds: Math.trunc(milliseconds),
          resolve,
          taken: false,
        }),
      )
    },
  })
  globals.Engine = Object.seal({
    started: false,
    goto: (target: unknown) => setEngine({ kind: "goto", target }),
    back: () => setEngine({ kind: "back" }),
    forward: () => setEngine({ kind: "forward" }),
    restart: () => setEngine({ kind: "restart" }),
  })
  globals.Story = Object.seal({
    has: (name: string) => configuration.story.passages.some((passage) => passage.name === name),
    current: () => configuration.story.current ?? undefined,
    get: (name: string) => configuration.story.passages.find((passage) => passage.name === name),
    visits: (name: string) => configuration.story.visits[name] ?? 0,
  })
}

function setEngine(request: unknown): void {
  if (globals.__narrava !== undefined) globals.__narrava.engine = request
}

export function takeHostOperation():
  | Pick<HostOperation, "id" | "kind" | "milliseconds">
  | { kind: "invalid-count"; count: number }
  | null {
  const pending = [...hostOperations.values()].filter((operation) => !operation.taken)
  if (pending.length !== 1) {
    return pending.length === 0 ? null : { kind: "invalid-count", count: pending.length }
  }
  pending[0].taken = true
  const { id, kind, milliseconds } = pending[0]
  return { id, kind, milliseconds }
}

export function resolveHostOperation(id: number): void {
  const operation = hostOperations.get(id)
  if (operation === undefined) throw new Error(`Host 异步操作不存在：${id}`)
  hostOperations.delete(id)
  operation.resolve()
}

import {
  hostOperationId,
  hostOperationQueue,
  runtimeConfiguration,
  scriptGlobals,
  type HostOperation,
} from "./internal"

export default function host(): void {
  scriptGlobals.Host = Object.freeze({
    delay: (milliseconds: number) => {
      if (!Number.isFinite(milliseconds) || milliseconds < 0 || milliseconds > 86_400_000) {
        throw new RangeError("Host.delay 毫秒数必须在 0 到 86400000 之间")
      }
      const id = hostOperationId()
      return new Promise<void>((resolve) =>
        hostOperationQueue.set(id, {
          id,
          kind: "delay",
          milliseconds: Math.trunc(milliseconds),
          resolve,
          taken: false,
        }),
      )
    },
  })
  scriptGlobals.Engine = Object.seal({
    started: false,
    goto: (target: unknown) => requestEngineCommand({ kind: "goto", target }),
    back: () => requestEngineCommand({ kind: "back" }),
    forward: () => requestEngineCommand({ kind: "forward" }),
    restart: () => requestEngineCommand({ kind: "restart" }),
  })
  scriptGlobals.Story = Object.seal({
    has: (name: string) =>
      runtimeConfiguration.story.passages.some((passage) => passage.name === name),
    current: () => runtimeConfiguration.story.current ?? undefined,
    get: (name: string) =>
      runtimeConfiguration.story.passages.find((passage) => passage.name === name),
    visits: (name: string) => runtimeConfiguration.story.visits[name] ?? 0,
  })
}

function requestEngineCommand(request: unknown): void {
  if (scriptGlobals.__narrava !== undefined) scriptGlobals.__narrava.engine = request
}

export function claimHostOperation():
  | Pick<HostOperation, "id" | "kind" | "milliseconds">
  | { kind: "invalid-count"; count: number }
  | null {
  const pending = [...hostOperationQueue.values()].filter((operation) => !operation.taken)
  if (pending.length !== 1) {
    return pending.length === 0 ? null : { kind: "invalid-count", count: pending.length }
  }
  pending[0].taken = true
  const { id, kind, milliseconds } = pending[0]
  return { id, kind, milliseconds }
}

export function completeHostOperation(id: number): void {
  const operation = hostOperationQueue.get(id)
  if (operation === undefined) throw new Error(`Host 异步操作不存在：${id}`)
  hostOperationQueue.delete(id)
  operation.resolve()
}

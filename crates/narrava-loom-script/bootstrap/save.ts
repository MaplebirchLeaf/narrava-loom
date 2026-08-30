import { allocateSubscription, globals, saveHooks } from "./internal"

interface SaveCompletion {
  operation: string
  target?: string
  succeeded: boolean
  error?: string
}

export function installSave(): void {
  globals.Save = Object.seal({
    capture: () => {
      saveBefore("capture", undefined)
      try {
        const json = JSON.stringify({ variables: __narravaStateSnapshot("variables") })
        saveAfter({ operation: "capture", succeeded: true })
        return json
      } catch (error) {
        saveAfter({ operation: "capture", succeeded: false, error: String(error) })
        throw error
      }
    },
    restore: (json: string) => {
      saveBefore("restore", undefined)
      try {
        __narravaStateReplace("variables", JSON.parse(json).variables ?? {})
        saveAfter({ operation: "restore", succeeded: true })
      } catch (error) {
        saveAfter({ operation: "restore", succeeded: false, error: String(error) })
        throw error
      }
    },
    export: (target = "manual") => requestSave("export", target),
    import: (target = "manual") => requestSave("import", target),
    before: (operation: string, hook: (value: unknown) => unknown) =>
      subscribe("before", operation, hook),
    after: (operation: string, hook: (value: unknown) => unknown) =>
      subscribe("after", operation, hook),
    off: (id: number) => saveHooks.delete(id),
  })
}

export function saveAfter(completion: SaveCompletion): void {
  const frozen = Object.freeze({ ...completion })
  for (const hook of saveHooks.values()) {
    if (hook.stage === "after" && hook.operation === completion.operation) hook.callback(frozen)
  }
}

function saveBefore(operation: string, target: string | undefined): string | undefined {
  let nextTarget = target
  for (const hook of saveHooks.values()) {
    if (hook.stage !== "before" || hook.operation !== operation) continue
    const rewritten = hook.callback(Object.freeze({ operation, target: nextTarget }))
    if (typeof rewritten === "string") nextTarget = rewritten
  }
  return nextTarget
}

function subscribe(
  stage: "before" | "after",
  operation: string,
  callback: (value: unknown) => unknown,
): number {
  if (!["capture", "restore", "export", "import"].includes(operation)) {
    throw new TypeError(`未知 Save 操作：${operation}`)
  }
  if (typeof callback !== "function") throw new TypeError("Save Hook 必须是函数")
  const id = allocateSubscription()
  saveHooks.set(id, { stage, operation, callback })
  return id
}

function requestSave(operation: "export" | "import", target: string): void {
  const rewritten = saveBefore(operation, target)
  if (globals.__narrava !== undefined) {
    globals.__narrava.save = { operation, target: rewritten }
  }
}

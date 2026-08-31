import { saveHooks, scriptGlobals, subscriptionId } from "./internal"

interface SaveCompletion {
  operation: string
  target?: string
  succeeded: boolean
  error?: string
}

export default function save(): void {
  scriptGlobals.Save = Object.seal({
    capture: () => {
      applyBeforeHooks("capture", undefined)
      try {
        const json = JSON.stringify({ variables: __narravaStateSnapshot("variables") })
        finishSave({ operation: "capture", succeeded: true })
        return json
      } catch (error) {
        finishSave({ operation: "capture", succeeded: false, error: String(error) })
        throw error
      }
    },
    restore: (json: string) => {
      applyBeforeHooks("restore", undefined)
      try {
        __narravaStateReplace("variables", JSON.parse(json).variables ?? {})
        finishSave({ operation: "restore", succeeded: true })
      } catch (error) {
        finishSave({ operation: "restore", succeeded: false, error: String(error) })
        throw error
      }
    },
    export: (target = "manual") => requestSave("export", target),
    import: (target = "manual") => requestSave("import", target),
    before: (operation: string, hook: (value: unknown) => unknown) =>
      registerHook("before", operation, hook),
    after: (operation: string, hook: (value: unknown) => unknown) =>
      registerHook("after", operation, hook),
    off: (id: number) => saveHooks.delete(id),
  })
}

export function finishSave(completion: SaveCompletion): void {
  const frozen = Object.freeze({ ...completion })
  for (const hook of saveHooks.values()) {
    if (hook.stage === "after" && hook.operation === completion.operation) hook.callback(frozen)
  }
}

function applyBeforeHooks(operation: string, target: string | undefined): string | undefined {
  let nextTarget = target
  for (const hook of saveHooks.values()) {
    if (hook.stage !== "before" || hook.operation !== operation) continue
    const rewritten = hook.callback(Object.freeze({ operation, target: nextTarget }))
    if (typeof rewritten === "string") nextTarget = rewritten
  }
  return nextTarget
}

function registerHook(
  stage: "before" | "after",
  operation: string,
  callback: (value: unknown) => unknown,
): number {
  if (!["capture", "restore", "export", "import"].includes(operation)) {
    throw new TypeError(`未知 Save 操作：${operation}`)
  }
  if (typeof callback !== "function") throw new TypeError("Save Hook 必须是函数")
  const id = subscriptionId()
  saveHooks.set(id, { stage, operation, callback })
  return id
}

function requestSave(operation: "export" | "import", target: string): void {
  const rewritten = applyBeforeHooks(operation, target)
  if (scriptGlobals.__narrava !== undefined) {
    scriptGlobals.__narrava.save = { operation, target: rewritten }
  }
}

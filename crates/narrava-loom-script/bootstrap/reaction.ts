import { encodeScriptValue, scriptGlobals } from "./internal"

type Matcher = string | RegExp

function encodePassageMatcher(
  value: Matcher,
): { exact: string } | { regex: string; flags: string } {
  return value instanceof RegExp
    ? { regex: value.source, flags: value.flags }
    : { exact: String(value) }
}

function encodePassageSelector(value: unknown): unknown {
  if (value === undefined) return undefined
  if (typeof value === "string" || value instanceof RegExp) {
    return { match: [encodePassageMatcher(value)] }
  }
  if (Array.isArray(value)) return { match: (value as Matcher[]).map(encodePassageMatcher) }
  const selector = value as {
    match?: Matcher[]
    exclude?: Matcher[]
    tags?: unknown
  }
  return {
    match: (selector.match ?? []).map(encodePassageMatcher),
    exclude: (selector.exclude ?? []).map(encodePassageMatcher),
    tags: selector.tags,
  }
}

function encodeEmit(reactionId: unknown, value: unknown): unknown {
  if (value === undefined) return undefined
  const emit = value as { name: unknown; payload?: unknown }
  return {
    name: emit.name,
    payload: encodeScriptValue(`${String(reactionId ?? "<unknown>")}.emit.payload`, emit.payload),
  }
}

export default function reaction(): void {
  scriptGlobals.Reaction = Object.freeze({
    add: (definition: Record<string, unknown>) => {
      if (definition === null || typeof definition !== "object") {
        throw new TypeError("Reaction.add 需要配置对象")
      }
      return Object.freeze(
        JSON.parse(
          __narravaReactionAdd(
            JSON.stringify({
              ...definition,
              passage: encodePassageSelector(definition.passage),
              emit: encodeEmit(definition.id, definition.emit),
              cond:
                definition.cond === undefined
                  ? undefined
                  : encodeScriptValue(
                      `${String(definition.id ?? "<unknown>")}.cond`,
                      definition.cond,
                    ),
            }),
          ),
        ),
      )
    },
    get: (id: unknown) => {
      const value = __narravaReactionGet(String(id))
      return value === undefined ? undefined : Object.freeze(JSON.parse(value))
    },
    enable: (id: unknown) => __narravaReactionEnable(String(id)),
    disable: (id: unknown) => __narravaReactionDisable(String(id)),
    reset: (id: unknown) => __narravaReactionReset(String(id)),
  })
}

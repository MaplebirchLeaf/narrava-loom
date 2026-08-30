import { globals, toHostValue } from "./internal"

type Matcher = string | RegExp

function normalizeMatcher(value: Matcher): { exact: string } | { regex: string } {
  return value instanceof RegExp ? { regex: value.source } : { exact: String(value) }
}

function normalizePassage(value: unknown): unknown {
  if (value === undefined) return undefined
  if (typeof value === "string" || value instanceof RegExp) {
    return { match: [normalizeMatcher(value)] }
  }
  if (Array.isArray(value)) return { match: (value as Matcher[]).map(normalizeMatcher) }
  const selector = value as {
    match?: Matcher[]
    exclude?: Matcher[]
    tags?: unknown
  }
  return {
    match: (selector.match ?? []).map(normalizeMatcher),
    exclude: (selector.exclude ?? []).map(normalizeMatcher),
    tags: selector.tags,
  }
}

export function installReaction(): void {
  globals.Reaction = Object.freeze({
    add: (definition: Record<string, unknown>) => {
      if (definition === null || typeof definition !== "object") {
        throw new TypeError("Reaction.add 需要配置对象")
      }
      return Object.freeze(
        JSON.parse(
          __narravaReactionAdd(
            JSON.stringify({
              ...definition,
              passage: normalizePassage(definition.passage),
              cond:
                definition.cond === undefined
                  ? undefined
                  : toHostValue(`${String(definition.id ?? "<unknown>")}.cond`, definition.cond),
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

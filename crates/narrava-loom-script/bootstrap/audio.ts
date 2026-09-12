import type {} from "../../../bindings/typescript/narrava-contract.generated"
import { scriptGlobals } from "./internal"

type Play = Extract<NarravaAudioEffect, { type: "play" }>
type Declaration = { effect: NarravaAudioEffect; tags: readonly string[] }
type Options = { channel?: string; loop?: boolean; volume?: number; tags?: readonly string[] }
let scopes = new Map<string, Declaration[]>([["global", []]])
let scope = "global"
let tags: readonly string[] = []
let active = new Map<string, Play>()
let checkpoint:
  | { scopes: Map<string, Declaration[]>; scope: string; tags: readonly string[] }
  | undefined

function name(value: unknown, field: string): string {
  if (typeof value !== "string" || value.trim().length === 0)
    throw new TypeError(`Audio ${field} 必须是非空字符串`)
  return value
}

export function beginAudio(): void {
  checkpoint ??= {
    scopes: new Map([...scopes].map(([key, values]) => [key, [...values]])),
    scope,
    tags,
  }
}

export function rollbackAudio(): void {
  if (checkpoint === undefined) return
  ;({ scopes, scope, tags } = checkpoint)
  checkpoint = undefined
}

export function audioPassage(currentTags: readonly string[]): void {
  scopes = new Map([["global", scopes.get("global") ?? []]])
  tags = [...currentTags]
  audioScope("passage")
}

export function audioScope(next: string, reset = true): void {
  scope = next
  if (reset || !scopes.has(scope)) scopes.set(scope, [])
}

function declare(effect: NarravaAudioEffect, selected: readonly string[]): void {
  const declarations = scopes.get(scope)!
  const index = declarations.findIndex(
    (entry) =>
      entry.effect.channel === effect.channel &&
      entry.tags.length === selected.length &&
      entry.tags.every((tag, position) => tag === selected[position]),
  )
  const declaration = { effect, tags: [...selected] }
  if (index >= 0) declarations.splice(index, 1)
  declarations.push(declaration)
}

function play(resource: string, options: Options = {}): void {
  if (
    typeof resource !== "string" ||
    resource.length === 0 ||
    resource.startsWith("/") ||
    /[\\:]/.test(resource) ||
    resource.includes("\0") ||
    resource.split("/").some((part) => part === ".." || part === "." || part === "")
  )
    throw new TypeError("Audio resource 必须是 Resource 逻辑路径")
  if (options === null || typeof options !== "object" || Array.isArray(options))
    throw new TypeError("Audio options 必须是对象")
  if (Object.keys(options).some((key) => !["channel", "loop", "volume", "tags"].includes(key)))
    throw new TypeError("Audio options 只支持 channel、loop、volume、tags")
  const channel = name(options.channel === undefined ? "bgm" : options.channel, "channel")
  const loop = options.loop === undefined ? true : options.loop
  const volume = options.volume === undefined ? 1 : options.volume
  const selected = options.tags === undefined ? [] : options.tags
  if (typeof loop !== "boolean" || !Number.isFinite(volume) || volume < 0 || volume > 1)
    throw new TypeError("Audio loop 必须是布尔值，volume 必须在 0..1 内")
  if (!Array.isArray(selected)) throw new TypeError("Audio tags 必须是字符串数组")
  selected.forEach((tag) => name(tag, "tag"))
  declare(
    {
      type: "play",
      resource: `audio/${resource.startsWith("audio/") ? resource.slice(6) : resource}`,
      channel,
      loop,
      volume,
    },
    selected,
  )
}

export function audioMacro(resource: string, channel = "bgm", tag?: string): void {
  play(resource, { channel, tags: tag === undefined ? [] : [name(tag, "tag")] })
}

/** 只向 Host 交付成功状态之间的差异；相同声明不会重复启动播放器。 */
export function takeAudio(): NarravaAudioEffect[] {
  const desired = new Map<string, Play>()
  for (const key of ["global", "Header", "Footer", "Bar", "BarStowed", "passage"]) {
    for (const { effect, tags: selected } of scopes.get(key) ?? []) {
      if (selected.length > 0 && !selected.some((tag) => tags.includes(tag))) continue
      if (effect.type === "stop") desired.delete(effect.channel)
      else desired.set(effect.channel, effect)
    }
  }
  const effects: NarravaAudioEffect[] = []
  for (const channel of active.keys()) {
    if (!desired.has(channel)) effects.push({ type: "stop", channel })
  }
  for (const [channel, next] of desired) {
    const previous = active.get(channel)
    if (
      previous?.resource !== next.resource ||
      previous.loop !== next.loop ||
      previous.volume !== next.volume
    )
      effects.push(next)
  }
  active = desired
  checkpoint = undefined
  return effects
}

export default function audio(): void {
  scriptGlobals.Audio = Object.freeze({
    play,
    stop(channel: string) {
      declare({ type: "stop", channel: name(channel, "channel") }, [])
    },
  })
}

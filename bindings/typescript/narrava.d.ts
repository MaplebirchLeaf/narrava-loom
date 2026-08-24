/**
 * Narrava Loom Core globals supplied by the Rust ECMAScript Script Binding.
 * Game scripts are not WebView scripts: `window`, `document` and Tauri APIs are unavailable.
 */
export {}

declare global {
  type NarravaPrimitive = undefined | null | boolean | number | string
  type NarravaData = NarravaPrimitive | readonly NarravaData[] | { readonly [key: string]: NarravaData }
  type NarravaCallable = (...arguments_: never[]) => NarravaData | void
  type NarravaGlobal = NarravaData | NarravaCallable

  type NarravaTextStyle =
    | "emphasis" | "strong" | "code" | "deleted" | "inserted" | "marked" | "small"
    | "subscript" | "superscript" | "quote"
    | "heading1" | "heading2" | "heading3" | "heading4" | "heading5" | "heading6"
  type NarravaTextTone =
    | "default" | "muted" | "accent" | "informational"
    | "positive" | "warning" | "negative" | "critical"
  type NarravaPresentationRegion = "header" | "main" | "footer" | "bar" | "dialog"
  interface NarravaPresentationNode { readonly __narravaPresentation: string; readonly key?: string }
  interface NarravaPresentation {
    text(text: string, options?: {
      readonly key?: string
      readonly styles?: readonly NarravaTextStyle[]
      readonly tone?: NarravaTextTone
    }): NarravaPresentationNode
    image(resource: string, options?: {
      readonly key?: string
      readonly alt?: string
      readonly caption?: string
    }): NarravaPresentationNode
    region(
      region: NarravaPresentationRegion,
      children: readonly (string | NarravaPresentationNode)[],
      options?: { readonly key?: string },
    ): NarravaPresentationNode
    component(
      capability: string,
      version: number,
      properties: Readonly<Record<string, NarravaData>>,
      fallback: readonly (string | NarravaPresentationNode)[],
      options?: { readonly key?: string },
    ): NarravaPresentationNode
    action(
      label: string,
      action: "dismiss",
      options?: {
        readonly key?: string
        readonly role?: "default" | "primary" | "secondary" | "danger"
      },
    ): NarravaPresentationNode
    fragment(...children: readonly (string | NarravaPresentationNode)[]): NarravaPresentationNode
  }
  const Presentation: NarravaPresentation

  interface NarravaImportReport { readonly inserted: number; readonly replaced: number }
  interface NarravaStateNamespace<T> {
    get(name: string): T | undefined
    has(name: string): boolean
    set(name: string, value: T): T | undefined
    del(name: string): T | undefined
    extend(values: Readonly<Record<string, T>>): NarravaImportReport
  }
  interface NarravaState {
    readonly global: NarravaStateNamespace<NarravaGlobal>
    readonly variables: NarravaStateNamespace<NarravaData>
    readonly temporary: NarravaStateNamespace<NarravaData>
    readonly setup: { get(): NarravaData; set(value: NarravaData): NarravaData }
  }
  const State: NarravaState

  type NarravaMacroSubscription = number & { readonly __macroSubscription: unique symbol }
  interface NarravaMacroCall {
    readonly name: string
    readonly arguments: readonly NarravaData[] | string
    readonly body?: string
  }
  interface NarravaMacroDefinition {
    readonly body: "inline" | "container"
    readonly arguments: "raw" | "list"
    /**
     * `async` 允许返回 Promise；需要等待时间时使用 `Host.delay()`。
     */
    readonly execution: "sync" | "async"
    readonly handler: (call: NarravaMacroCall) =>
      NarravaData | NarravaPresentationNode | Promise<NarravaData | NarravaPresentationNode>
  }
  interface NarravaMacro {
    add(name: string, definition: NarravaMacroDefinition): NarravaMacroDefinition | undefined
    update(name: string, definition: NarravaMacroDefinition): NarravaMacroDefinition
    del(name: string): NarravaMacroDefinition | undefined
    get(name: string): NarravaMacroDefinition | undefined
    has(name: string): boolean
    before(name: string, hook: (call: NarravaMacroCall) => void): NarravaMacroSubscription
    after(name: string, hook: (output: NarravaData, call: NarravaMacroCall) => NarravaData): NarravaMacroSubscription
    off(subscription: NarravaMacroSubscription): boolean
  }
  const Macro: NarravaMacro

  interface NarravaEngine {
    readonly started: boolean
    goto(target: string): void
    back(): void
    forward(): void
    restart(): void
  }
  const Engine: NarravaEngine

  interface NarravaPassageInfo { readonly name: string; readonly tags: readonly string[] }
  interface NarravaStory {
    has(name: string): boolean
    current(): NarravaPassageInfo | undefined
    get(name: string): NarravaPassageInfo | undefined
    visits(name: string): number
  }
  const Story: NarravaStory

  type NarravaLogLevel = "trace" | "debug" | "info" | "warn" | "error"
  type NarravaLogSubscription = number & { readonly __logSubscription: unique symbol }
  interface NarravaLogRecord {
    readonly sequence: number
    readonly level: NarravaLogLevel
    readonly target: string
    readonly message: string
  }
  interface NarravaLogger {
    trace(target: string, message: string): void
    debug(target: string, message: string): void
    info(target: string, message: string): void
    warn(target: string, message: string): void
    error(target: string, message: string): void
    subscribe(filter?: { minimumLevel?: NarravaLogLevel; target?: string }): NarravaLogSubscription
    take(subscription: NarravaLogSubscription): NarravaLogRecord[] | undefined
    unsubscribe(subscription: NarravaLogSubscription): boolean
  }
  const Logger: NarravaLogger

  type NarravaEventSubscription = number & { readonly __eventSubscription: unique symbol }
  type NarravaPassageEventName =
    | "passage:init"
    | "passage:start"
    | "passage:render"
    | "passage:display"
    | "passage:end"
  interface NarravaPassageEventPayload {
    readonly passage: string
    readonly tags: readonly string[]
  }
  interface NarravaEventRecord {
    /** Monotonically increasing sequence within the current game runtime. */
    readonly sequence: number
    /** Exact, case-sensitive author-defined event name. */
    readonly name: string
    /** Data snapshot supplied to Event.emit. */
    readonly payload: NarravaData
  }
  interface NarravaEvent {
    /** Emit an author-defined event. The five `passage:*` names are Engine-reserved. */
    emit(name: string, payload?: NarravaData): number
    /** Subscribe to future events only; omit name to receive every author event. */
    subscribe(filter?: { name?: string }): NarravaEventSubscription
    /** Drain pending records. Returns undefined only when the subscription does not exist. */
    take(subscription: NarravaEventSubscription): NarravaEventRecord[] | undefined
    unsubscribe(subscription: NarravaEventSubscription): boolean
  }
  const Event: NarravaEvent

  interface NarravaHost {
    /** Suspend the current Engine transaction and resume it after the delay. */
    delay(milliseconds: number): Promise<void>
  }
  const Host: NarravaHost

  interface NarravaResourceInfo { readonly path: string; readonly mediaType: string; readonly size: number }
  interface NarravaResource {
    paths(): readonly string[]
    has(path: string): boolean
    pick(candidates: readonly string[]): string | undefined
    info(path: string): NarravaResourceInfo | undefined
    read(path: string): Uint8Array | undefined
    text(path: string): string | undefined
  }
  const Resource: NarravaResource

  type NarravaSaveOperation = "capture" | "restore" | "export" | "import"
  type NarravaSaveSubscription = number & { readonly __saveSubscription: unique symbol }
  interface NarravaSaveBeforeContext { readonly operation: NarravaSaveOperation; readonly target?: string }
  interface NarravaSaveCompletion {
    readonly operation: NarravaSaveOperation
    readonly target?: string
    readonly succeeded: boolean
    readonly error?: string
  }
  interface NarravaSave {
    capture(): string
    restore(json: string): void
    export(target?: string): void
    import(target?: string): void
    /** Run before an operation. Returning a string rewrites export/import target. */
    before(operation: NarravaSaveOperation, hook: (context: NarravaSaveBeforeContext) => string | void): NarravaSaveSubscription
    /** Run only after an operation has an actual completion result. */
    after(operation: NarravaSaveOperation, hook: (completion: NarravaSaveCompletion) => void): NarravaSaveSubscription
    off(subscription: NarravaSaveSubscription): boolean
  }
  const Save: NarravaSave

  interface NarravaI18n {
    readonly defaultLocale: string
    readonly locale: string
    /** Return the complete translator template as formatted JSON. */
    export(): string
  }
  const I18n: NarravaI18n

  /** Unified, frozen entry point. Uppercase globals remain available as aliases. */
  interface NarravaApi {
    readonly Engine: NarravaEngine
    readonly State: NarravaState
    readonly Macro: NarravaMacro
    readonly Story: NarravaStory
    readonly Logger: NarravaLogger
    readonly Event: NarravaEvent
    readonly Host: NarravaHost
    readonly Save: NarravaSave
    readonly Resource: NarravaResource
    readonly I18n: NarravaI18n
    readonly Presentation: NarravaPresentation
  }
  const narrava: NarravaApi
}

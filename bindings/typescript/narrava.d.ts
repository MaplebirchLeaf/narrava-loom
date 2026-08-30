/**
 * Narrava Loom Core globals supplied by the Rust ECMAScript Script Binding.
 * Game scripts are not WebView scripts: `window`, `document` and Tauri APIs are unavailable.
 */
export {}

declare global {
  /** 脚本层可往返的 JSON 兼容原始值。 */
  type NarravaPrimitive = undefined | null | boolean | number | string
  /** 可存入 State / 事件载荷 / 存档的数据：原始值、只读数组或只读对象。 */
  type NarravaData =
    | NarravaPrimitive
    | readonly NarravaData[]
    | { readonly [key: string]: NarravaData }
  /** Core 可调用的脚本函数；返回值会进入 Core Value 图。 */
  type NarravaCallable = (...arguments_: never[]) => NarravaData | void
  /** State.global 可存放的值：数据或可调用函数。 */
  type NarravaGlobal = NarravaData | NarravaCallable

  /** 语义字形（8 个）：emphasis 强调 / strong 加粗 / code 等宽 / quote 引用 /
   *  marked 高亮 / small 小字 / inserted 新增 / deleted 删除；视觉由 Host 决定。 */
  type NarravaTextStyle =
    | "emphasis"
    | "strong"
    | "code"
    | "quote"
    | "marked"
    | "small"
    | "inserted"
    | "deleted"
  /** 64 级色阶（0..=63）：灰阶 0-7（白1→亮灰2→浅灰3→灰4→深灰5→暗灰6→黑7），
   *  光谱 8-63（红8→橙16→黄24→绿32→蓝40→紫48→深紫56→63）；0 为正文默认。 */
  type NarravaTextColor =
    | 0
    | 1
    | 2
    | 3
    | 4
    | 5
    | 6
    | 7
    | 8
    | 9
    | 10
    | 11
    | 12
    | 13
    | 14
    | 15
    | 16
    | 17
    | 18
    | 19
    | 20
    | 21
    | 22
    | 23
    | 24
    | 25
    | 26
    | 27
    | 28
    | 29
    | 30
    | 31
    | 32
    | 33
    | 34
    | 35
    | 36
    | 37
    | 38
    | 39
    | 40
    | 41
    | 42
    | 43
    | 44
    | 45
    | 46
    | 47
    | 48
    | 49
    | 50
    | 51
    | 52
    | 53
    | 54
    | 55
    | 56
    | 57
    | 58
    | 59
    | 60
    | 61
    | 62
    | 63
  /** 语义展示区域：作者用 Surface.region 把内容放进 Host 的稳定容器。 */
  type NarravaRegionId = string
  /** Surface 树节点；由 Surface.* 工厂创建，Host 只按语义渲染。 */
  interface NarravaSurfaceNode {
    readonly __narravaSurface: string
    readonly key?: string
  }
  /** 作者侧语义展示 API：只表达语义，颜色与字形由 Host 决定。 */
  interface NarravaSurface {
    /** 普通文本；styles 为语义字形，color 为 0..=63 标准调色板索引。 */
    text(
      text: string,
      options?: {
        readonly key?: string
        readonly styles?: readonly NarravaTextStyle[]
        readonly color?: NarravaTextColor
        /** 可见延迟（毫秒）；具体动画由 Host 决定。 */
        readonly delay?: number
        /** 结构性标题级别（1 或 2）：用于页面划分（如弹窗页签的页面标题），不是字形样式。 */
        readonly heading?: 1 | 2
      },
    ): NarravaSurfaceNode
    /** 插入一个结构性硬换行。硬换行没有内容或稳定身份，因此不接受参数。 */
    hardBreak(): NarravaSurfaceNode
    /** 引用 Resource 逻辑路径的图片；alt/caption 可选。 */
    image(
      resource: string,
      options?: {
        readonly key?: string
        readonly alt?: string
        readonly caption?: string
      },
    ): NarravaSurfaceNode
    /** 把子节点放入开放逻辑区域；内建值包括 main/header/footer/bar/bar-stowed/dialog。 */
    region(
      region: NarravaRegionId,
      children: readonly (string | NarravaSurfaceNode)[],
      options?: { readonly key?: string },
    ): NarravaSurfaceNode
    /** 请求 Host 渲染能力组件（capability + version）；Host 不认识时显示 fallback。 */
    component(
      capability: string,
      version: number,
      properties: Readonly<Record<string, NarravaData>>,
      fallback: readonly (string | NarravaSurfaceNode)[],
      options?: { readonly key?: string },
    ): NarravaSurfaceNode
    /** 可交互按钮；action 目前仅支持 dismiss（关闭打开的 Dialog）。 */
    action(
      label: string,
      action: "dismiss",
      options?: {
        readonly key?: string
        readonly role?: "default" | "primary" | "secondary" | "danger"
      },
    ): NarravaSurfaceNode
    /** 组合多个节点为一段分组，常用于宏一次返回多段内容。 */
    fragment(...children: readonly (string | NarravaSurfaceNode)[]): NarravaSurfaceNode
  }
  const Surface: NarravaSurface

  /** State.extend 的批量写入统计：新增与覆盖的键数量。 */
  interface NarravaImportReport {
    readonly inserted: number
    readonly replaced: number
  }
  /** 一个 State 命名空间的读写接口；set/del 返回被替换的旧值（不存在时为 undefined）。 */
  interface NarravaStateNamespace<T> {
    get(name: string): T | undefined
    has(name: string): boolean
    set(name: string, value: T): T | undefined
    del(name: string): T | undefined
    extend(values: Readonly<Record<string, T>>): NarravaImportReport
  }
  /** 作者侧 State 入口：global 存函数与数据、variables 参与存档（capture/restore 只覆盖它）、
   *  temporary 在 restore 时重建、setup 是单个启动值（随启动环境管理）。 */
  interface NarravaState {
    readonly global: NarravaStateNamespace<NarravaGlobal>
    readonly variables: NarravaStateNamespace<NarravaData>
    readonly temporary: NarravaStateNamespace<NarravaData>
    readonly setup: { get(): NarravaData; set(value: NarravaData): NarravaData }
  }
  const State: NarravaState

  /** `$variables` 的属性代理。点语法与动态方括号语法都直接读写活动 Rust State。 */
  const V: { [name: string]: NarravaData }
  /** `_temporary` 的属性代理；恢复存档时会随临时变量一起清空。 */
  const T: { [name: string]: NarravaData }
  /** 启动配置对象的属性代理，与 Twee 中的 `setup.name` 指向同一份数据。 */
  const setup: { [name: string]: NarravaData }

  type NarravaReactionPassageMatcher = string | RegExp
  interface NarravaReactionPassageSelector {
    readonly match?: readonly NarravaReactionPassageMatcher[]
    readonly exclude?: readonly NarravaReactionPassageMatcher[]
    readonly tags?: {
      readonly any?: readonly string[]
      readonly all?: readonly string[]
      readonly none?: readonly string[]
    }
  }
  interface NarravaReactionEffect {
    readonly id: string
    /** 通过 Engine 事务导航；目标 Passage 会正常经历 lifecycle 与 history。 */
    readonly goto?: string
    /** 继续派发结构化 Event；Runtime 会检测后代环并限制执行次数。 */
    readonly emit?: { readonly name: string; readonly payload?: NarravaData }
    /** 仅 lifecycle Reaction 可用；在 Reaction Phase 截断目标 Passage 原正文。 */
    readonly exit?: true
    readonly enabled?: boolean
    readonly once?: boolean
    readonly limit?: number
    readonly tags?: readonly string[]
  }
  type NarravaReactionContent =
    | {
        /** Twee Widget 调用源码；可直接追加，也可替换稳定目标。 */
        readonly widget: string
        readonly include?: never
        readonly replace?: string
      }
    | {
        /** Passage fragment 必须明确替换目标，不能隐式追加到整页末尾。 */
        readonly include: string
        readonly replace: string
        readonly widget?: never
      }
    | { readonly widget?: never; readonly include?: never; readonly replace?: never }
  type NarravaEventReactionDefinition = NarravaReactionEffect &
    NarravaReactionContent & {
      readonly event: string
      readonly state?: never
      readonly lifecycle?: never
      readonly passage?: never
      readonly exit?: never
      readonly cond?: (payload: NarravaData) => boolean
    }
  type NarravaStateReactionDefinition = NarravaReactionEffect &
    NarravaReactionContent & {
      readonly event?: never
      readonly state: `$${string}`
      readonly lifecycle?: never
      readonly passage?: never
      readonly exit?: never
      readonly cond?: (change: {
        readonly before: NarravaData
        readonly after: NarravaData
      }) => boolean
    }
  type NarravaLifecycleReactionDefinition = NarravaReactionEffect &
    NarravaReactionContent & {
      readonly event?: never
      readonly state?: never
      readonly lifecycle: true
      readonly passage?:
        | NarravaReactionPassageMatcher
        | readonly NarravaReactionPassageMatcher[]
        | NarravaReactionPassageSelector
      readonly cond?: () => boolean
    }
  type NarravaReactionDefinition =
    | NarravaEventReactionDefinition
    | NarravaStateReactionDefinition
    | NarravaLifecycleReactionDefinition
  interface NarravaReactionStatus {
    readonly id: string
    readonly enabled: boolean
    readonly triggered: number
    readonly tags: readonly string[]
  }
  /** 声明式叙事反应规则；规则本体与次数状态由 Native Runtime 持有。 */
  const Reaction: {
    add(definition: NarravaReactionDefinition): NarravaReactionStatus
    get(id: string): NarravaReactionStatus | undefined
    enable(id: string): boolean
    disable(id: string): boolean
    reset(id: string): boolean
  }

  /** Macro.before/after 订阅返回的不透明句柄。 */
  type NarravaMacroSubscription = number & { readonly __macroSubscription: unique symbol }
  /** 宏调用上下文：宏名、参数（列表或原始字符串）与容器宏正文。 */
  interface NarravaMacroCall {
    readonly name: string
    readonly arguments: readonly NarravaData[] | string
    readonly body?: string
  }
  /** 宏定义：body 决定原地展开还是包裹正文，arguments 决定参数形态，
   *  execution 决定 handler 是否可返回 Promise。 */
  interface NarravaMacroDefinition {
    readonly body: "inline" | "container"
    readonly arguments: "raw" | "list"
    /**
     * `async` 允许返回 Promise；需要等待时间时使用 `Host.delay()`。
     */
    readonly execution: "sync" | "async"
    readonly handler: (
      call: NarravaMacroCall,
    ) => NarravaData | NarravaSurfaceNode | Promise<NarravaData | NarravaSurfaceNode>
  }
  /** 作者宏注册表：脚本用 Macro.add 定义的新宏可在 .twee 中调用。 */
  interface NarravaMacro {
    /** 注册宏；返回同名旧定义（不存在时为 undefined）。 */
    add(name: string, definition: NarravaMacroDefinition): NarravaMacroDefinition | undefined
    /** 替换已存在的宏；宏不存在时抛错，返回旧定义。 */
    update(name: string, definition: NarravaMacroDefinition): NarravaMacroDefinition
    /** 删除宏；返回被删除的定义（不存在时为 undefined）。 */
    del(name: string): NarravaMacroDefinition | undefined
    get(name: string): NarravaMacroDefinition | undefined
    has(name: string): boolean
    /** 在宏执行前调用 hook，可观察但不能改写输出。 */
    before(name: string, hook: (call: NarravaMacroCall) => void): NarravaMacroSubscription
    /** 在宏输出后调用 hook，返回的值为新的输出。 */
    after(
      name: string,
      hook: (output: NarravaData, call: NarravaMacroCall) => NarravaData,
    ): NarravaMacroSubscription
    /** 注销 before/after 订阅；返回是否成功。 */
    off(subscription: NarravaMacroSubscription): boolean
  }
  const Macro: NarravaMacro

  /** 引擎导航请求：goto/back/forward/restart 在当前事务结束后由 Host 执行。 */
  interface NarravaEngine {
    readonly started: boolean
    goto(target: string): void
    back(): void
    forward(): void
    restart(): void
  }
  const Engine: NarravaEngine

  /** Passage 元数据快照：名称与 Tag 列表。 */
  interface NarravaPassageInfo {
    readonly name: string
    readonly tags: readonly string[]
  }
  /** 只读 Story 查询：has/get/current/visits。 */
  interface NarravaStory {
    has(name: string): boolean
    current(): NarravaPassageInfo | undefined
    get(name: string): NarravaPassageInfo | undefined
    visits(name: string): number
  }
  const Story: NarravaStory

  /** 日志级别，由低到高。 */
  type NarravaLogLevel = "trace" | "debug" | "info" | "warn" | "error"
  /** Logger.subscribe 返回的不透明句柄。 */
  type NarravaLogSubscription = number & { readonly __logSubscription: unique symbol }
  /** 单条日志记录：序号、级别、target 与消息。 */
  interface NarravaLogRecord {
    readonly sequence: number
    readonly level: NarravaLogLevel
    readonly target: string
    readonly message: string
  }
  /** 按 target 记日志；subscribe 后由 take 取走尚未消费的记录。 */
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

  /** Event.subscribe 返回的不透明句柄。 */
  type NarravaEventSubscription = number & { readonly __eventSubscription: unique symbol }
  /** Engine 保留的五个 Passage 生命周期事件名，脚本不可 emit。 */
  type NarravaPassageEventName = NarravaBuiltinEventName
  /** Passage 生命周期事件的载荷：当前 Passage 名与 Tag 列表。 */
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
  /** 作者事件总线：emit 返回记录序号；订阅只接收之后发生的事件。 */
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

  /** Host 能力入口；目前只有 delay（配合 async 宏做时间等待）。 */
  interface NarravaHost {
    /** 挂起当前 Engine 事务并在 delay 毫秒后恢复；取值范围 0..=86400000。 */
    delay(milliseconds: number): Promise<void>
  }
  const Host: NarravaHost

  /** Resource 元数据：逻辑路径、媒体类型与字节数。 */
  interface NarravaResourceInfo {
    readonly path: string
    readonly mediaType: string
    readonly size: number
  }
  /** 只读 Resource 查询：路径列表、存在性、候选选取与读取。 */
  interface NarravaResource {
    paths(): readonly string[]
    has(path: string): boolean
    /** 按顺序返回第一个存在的候选路径。 */
    pick(candidates: readonly string[]): string | undefined
    info(path: string): NarravaResourceInfo | undefined
    /** 读取原始字节；路径不存在时为 undefined。 */
    read(path: string): Uint8Array | undefined
    /** 按 UTF-8 读取为文本；路径不存在或非文本时为 undefined。 */
    text(path: string): string | undefined
  }
  const Resource: NarravaResource

  /** 存档操作：capture/restore 由脚本直接读写 variables，export/import 请求 Host 文件操作。 */
  type NarravaSaveOperation = "capture" | "restore" | "export" | "import"
  /** Save.before/after 订阅返回的不透明句柄。 */
  type NarravaSaveSubscription = number & { readonly __saveSubscription: unique symbol }
  /** before hook 上下文：操作与目标（export/import 时存在）。 */
  interface NarravaSaveBeforeContext {
    readonly operation: NarravaSaveOperation
    readonly target?: string
  }
  /** after hook 的完成结果：操作、目标与成败。 */
  interface NarravaSaveCompletion {
    readonly operation: NarravaSaveOperation
    readonly target?: string
    readonly succeeded: boolean
    readonly error?: string
  }
  /** 存档入口：capture/restore 覆盖 variables 命名空间；export/import 走 Host。 */
  interface NarravaSave {
    /** 生成当前 variables 的存档 JSON 字符串。 */
    capture(): string
    /** 用存档 JSON 整体替换 variables；非法 JSON 抛错。 */
    restore(json: string): void
    /** 请求 Host 把存档导出到 target（默认 manual）。 */
    export(target?: string): void
    /** 请求 Host 从 target 导入存档。 */
    import(target?: string): void
    /** Run before an operation. Returning a string rewrites export/import target. */
    before(
      operation: NarravaSaveOperation,
      hook: (context: NarravaSaveBeforeContext) => string | void,
    ): NarravaSaveSubscription
    /** Run only after an operation has an actual completion result. */
    after(
      operation: NarravaSaveOperation,
      hook: (completion: NarravaSaveCompletion) => void,
    ): NarravaSaveSubscription
    off(subscription: NarravaSaveSubscription): boolean
  }
  const Save: NarravaSave

  /** 只读本地化信息：当前 locale 与翻译模板导出。 */
  interface NarravaI18n {
    readonly defaultLocale: string
    readonly locale: string
    /** Return the complete translator template as formatted JSON. */
    export(): string
  }
  const I18n: NarravaI18n
}

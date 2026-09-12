import type {} from "./narrava-contract.generated"

/**
 * Rust ECMAScript 绑定提供的 Core 全局 API；游戏脚本不提供 window、document 或 Tauri API。
 *
 * Core globals supplied by the Rust ECMAScript binding. Game scripts have no window, document, or Tauri APIs.
 */
export {}

declare global {
  /** 可保存、可回放的共享随机序列。 Shared, saved and replayable random sequence. */
  const Random: {
    /** 生成 [0, 1) 的随机数，与 Math.random 和 Twee random/either 共用序列。
     * Draw from [0, 1), sharing the sequence with Math.random and Twee random/either. */
    next(): number
    /** 以非负安全整数重置序列；默认种子为 0。
     * Reset with a nonnegative safe integer; the default seed is 0. */
    seed(seed: number): void
    /** 只读序列快照；十进制字符串保留完整的 64 位状态。
     * Read-only sequence snapshot; decimal strings preserve all 64 bits. */
    current(): { readonly seed: string; readonly state: string }
  }

  /**
   * 脚本层可往返的 JSON 兼容原始值。
   *
   * JSON-compatible primitive values supported by the script bridge.
   */
  type NarravaPrimitive = undefined | null | boolean | number | string
  /**
   * 可存入 State / 事件载荷 / 存档的数据：原始值、只读数组或只读对象。
   *
   * Data accepted by State, event payloads, and saves: primitives, readonly arrays, or readonly objects.
   */
  type NarravaData =
    | NarravaPrimitive
    | readonly NarravaData[]
    | { readonly [key: string]: NarravaData }
  /**
   * Core 可调用的脚本函数；返回值会进入 Core Value 图。
   *
   * A script function callable by Core; its result enters the Core value graph.
   */
  type NarravaCallable = (...arguments_: never[]) => NarravaData | void
  /**
   * State.global 可存放的值：数据或可调用函数。
   *
   * Values accepted by State.global: data or callable functions.
   */
  type NarravaGlobal = NarravaData | NarravaCallable

  /**
   * 世界坐标，允许负值；两个分量都必须在 ±(2^53 - 1) 的安全整数范围内。
   *
   * Location coordinates may be negative; both components must be safe integers within ±(2^53 - 1).
   */
  type NarravaLocationPoint = readonly [number, number]
  /**
   * 世界地点定义；id 为英文裸 Tag，bounds 为包含边界的简单多边形。
   *
   * A place definition: id is a bare English tag; bounds is a simple polygon including its boundary.
   */
  interface NarravaPlaceDefinition {
    /**
     * 稳定英文地点 ID；也是 Passage 的裸 Tag。
     *
     * Stable English place ID and bare Passage tag.
     */
    readonly id: string
    /**
     * 可选显示名称，支持中文。
     *
     * Optional display name, including Chinese text.
     */
    readonly name?: string
    /**
     * 父地点 ID；允许前向引用。
     *
     * Parent place ID; forward references are allowed.
     */
    readonly parent?: string
    /**
     * 同一世界坐标系中的多边形顶点。
     *
     * Polygon vertices in the shared location coordinate system.
     */
    readonly bounds: readonly NarravaLocationPoint[]
    /**
     * 默认入口；省略时使用首顶点。
     *
     * Entry point; defaults to the first vertex.
     */
    readonly entry?: NarravaLocationPoint
  }
  /**
   * 地点注册后的独立快照。
   *
   * An independent snapshot of a registered place.
   */
  type NarravaPlace = NarravaPlaceDefinition
  /**
   * 当前世界位置；inside/outside 来自 Passage 环境 Tag。
   *
   * The current location position; inside/outside comes from Passage environment tags.
   */
  interface NarravaLocationPosition {
    readonly place: string
    readonly point: NarravaLocationPoint
    readonly environment: "inside" | "outside" | null
  }
  /**
   * 地点定义与移动查询；状态随故事事务、历史和存档恢复。
   *
   * Place definitions and movement queries; state follows story transactions, history, and saves.
   */
  interface NarravaLocation {
    /**
     * 仅初始脚本装载期间可注册；parent 可引用稍后注册的地点。
     *
     * Register only during initial script loading; parent may refer to a place registered later.
     */
    add(place: NarravaPlaceDefinition): void
    /**
     * 按 ID 查询独立快照；不存在时返回 undefined。
     *
     * Get a snapshot by ID, or undefined if absent.
     */
    get(id: string): NarravaPlace | undefined
    /**
     * 按 ID 排序的全部地点快照。
     *
     * Snapshots of all places sorted by ID.
     */
    places(): readonly NarravaPlace[]
    /**
     * 查询包含坐标的所有地点，父地点先于子地点；保留重叠地点。
     *
     * Find all places containing the point, with parents before children; overlapping places are retained.
     */
    locate(point: NarravaLocationPoint): readonly NarravaPlace[]
    /**
     * 当前位置快照；未进入地点时为 null。
     *
     * Current position snapshot, or null before entering a place.
     */
    current(): NarravaLocationPosition | null
    /**
     * 只在当前地点范围内移动；未进入地点或越界会抛出错误。 读档/语言刷新当前页时只校验并返回既有位置，不重复提交移动。
     *
     * Move within the current place; throws if no place is active or the point is outside it. Save/language refreshes validate and return the existing position without moving again.
     */
    move(point: NarravaLocationPoint): NarravaLocationPosition
  }
  const Location: NarravaLocation

  /**
   * 语义字形（8 个）：emphasis 强调 / strong 加粗 / code 等宽 / quote 引用 / marked 高亮 / small 小字 / inserted 新增 / deleted 删除；视觉由 Host 决定。
   *
   * Eight semantic text styles; the host chooses their visual appearance.
   */
  type NarravaTextStyle =
    | "emphasis"
    | "strong"
    | "code"
    | "quote"
    | "marked"
    | "small"
    | "inserted"
    | "deleted"
  /**
   * 64 级色阶（0..=63）：灰阶 0-7（白1→亮灰2→浅灰3→灰4→深灰5→暗灰6→黑7）， 光谱 8-63（红8→橙16→黄24→绿32→蓝40→紫48→深紫56→63）；0 为正文默认。
   *
   * Palette indices 0..63: 0 is the default, 1..7 run from white to black, and 8..63 run through the color spectrum from red to purple.
   */
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
  /**
   * 语义展示区域：作者用 Surface.region 把内容放进 Host 的稳定容器。
   *
   * A semantic display region; Surface.region places content in a stable host container.
   */
  type NarravaRegionId = string
  /**
   * Surface 树节点；由 Surface.* 工厂创建，Host 只按语义渲染。
   *
   * A Surface tree node created by Surface factories and rendered by the host from its semantics.
   */
  interface NarravaSurfaceNode {
    readonly __narravaSurface: string
    readonly key?: string
  }
  /**
   * 作者侧语义展示 API：只表达语义，颜色与字形由 Host 决定。
   *
   * The author presentation API; the host interprets semantic colors and text styles.
   */
  interface NarravaSurface {
    /**
     * 普通文本；styles 为语义字形，color 为 0..=63 标准调色板索引。
     *
     * Text with semantic styles and a standard palette index from 0 to 63.
     */
    text(
      text: string,
      options?: {
        readonly key?: string
        readonly styles?: readonly NarravaTextStyle[]
        readonly color?: NarravaTextColor
        /**
         * 可见延迟（毫秒）；具体动画由 Host 决定。
         *
         * Visibility delay in milliseconds; the host chooses the animation.
         */
        readonly delay?: number
        /**
         * 结构性标题级别（1 或 2）：用于正文标题，不是字形样式。
         *
         * Structural heading level (1 or 2), rather than a text style.
         */
        readonly heading?: 1 | 2
      },
    ): NarravaSurfaceNode
    /**
     * 插入一个结构性硬换行。硬换行没有内容或稳定身份，因此不接受参数。
     *
     * Insert a structural hard break. It has no content or stable identity and accepts no arguments.
     */
    hardBreak(): NarravaSurfaceNode
    /**
     * 引用 Resource 逻辑路径的图片；alt 可选。
     *
     * An image addressed by a Resource logical path, with optional alt text.
     */
    image(
      resource: string,
      options?: {
        readonly key?: string
        readonly alt?: string
      },
    ): NarravaSurfaceNode
    /**
     * 把子节点放入开放逻辑区域；内建值包括 main/header/footer/bar/bar-stowed/dialog。
     *
     * Place children in an open logical region; built-ins include main/header/footer/bar/bar-stowed/dialog.
     */
    region(
      region: NarravaRegionId,
      children: readonly (string | NarravaSurfaceNode)[],
      options?: { readonly key?: string },
    ): NarravaSurfaceNode
    /**
     * 请求 Host 渲染能力组件（capability + version）；Host 不认识时显示 fallback。
     *
     * Request a host component by capability and version; unsupported hosts display the fallback.
     */
    component(
      capability: string,
      version: number,
      properties: Readonly<Record<string, NarravaData>>,
      fallback: readonly (string | NarravaSurfaceNode)[],
      options?: { readonly key?: string },
    ): NarravaSurfaceNode
    /**
     * 可交互按钮；action 目前仅支持 dismiss（关闭打开的 Dialog）。
     *
     * An interactive button; the only supported action is dismiss, which closes the open dialog.
     */
    action(
      label: string,
      action: "dismiss",
      options?: {
        readonly key?: string
        readonly role?: "default" | "primary" | "secondary" | "danger"
      },
    ): NarravaSurfaceNode
    /**
     * 组合多个节点为一段分组，常用于宏一次返回多段内容。
     *
     * Group multiple nodes, often to return several pieces of content from one macro.
     */
    fragment(...children: readonly (string | NarravaSurfaceNode)[]): NarravaSurfaceNode
  }
  const Surface: NarravaSurface

  /**
   * 批量导入统计：新增与覆盖的键数量。
   *
   * Bulk import counts: newly inserted keys and replaced keys.
   */
  interface NarravaImportReport {
    readonly inserted: number
    readonly replaced: number
  }
  /**
   * 一个 State 命名空间的读写接口；set/del 返回被替换的旧值（不存在时为 undefined）。
   *
   * Read and write a State namespace; set/del return the previous value, or undefined if absent.
   */
  interface NarravaStateNamespace<T> {
    /** 读取命名变量，不存在时返回 undefined。 Read a named value; missing keys return undefined. */
    get(name: string): T | undefined
    /** 检查名称是否存在于此接口的集合中。 Check whether this named entry exists. */
    has(name: string): boolean
    /** 写入变量并返回旧值。 Write a value and return its previous value. */
    set(name: string, value: T): T | undefined
    /** 删除变量并返回旧值。 Delete a key and return its previous value. */
    del(name: string): T | undefined
    /** 批量导入变量，返回新增和覆盖数量。 Import values and report inserted/replaced counts. */
    extend(values: Readonly<Record<string, T>>): NarravaImportReport
  }
  /**
   * 作者侧 State 入口：global 存函数与数据、variables 参与存档（capture/restore 只覆盖它）、 temporary 在 restore 时重建、setup 是单个启动值（随启动环境管理）。
   *
   * State namespaces: global holds functions and data; variables is saved; temporary is rebuilt on restore; setup holds startup configuration. Script capture/restore covers variables only.
   */
  interface NarravaState {
    readonly global: NarravaStateNamespace<NarravaGlobal>
    readonly variables: NarravaStateNamespace<NarravaData>
    readonly temporary: NarravaStateNamespace<NarravaData>
    /** 启动配置读写接口。 Startup configuration access. */
    readonly setup: {
      /** 读取启动配置。 Read startup configuration. */
      get(): NarravaData
      /** 整体替换启动配置。 Replace startup configuration. */
      set(value: NarravaData): NarravaData
    }
  }
  const State: NarravaState

  /**
   * `$variables` 的属性代理。点语法与动态方括号语法都直接读写活动 Rust State。
   *
   * A property proxy for $variables; dot and bracket access both read and write the active Rust State.
   */
  const V: { [name: string]: NarravaData }
  /**
   * `_temporary` 的属性代理；恢复存档时会随临时变量一起清空。
   *
   * A property proxy for _temporary, cleared when a save is restored.
   */
  const T: { [name: string]: NarravaData }
  /**
   * 启动配置对象的属性代理，与 Twee 中的 `setup.name` 指向同一份数据。
   *
   * A startup configuration proxy sharing the same data as setup.name in Twee.
   */
  const setup: { [name: string]: NarravaData }

  /**
   * 字符串精确匹配；RegExp 保留 i/m/s/u flags，其他或重复 flags 会被拒绝。
   *
   * Strings match exactly; RegExp accepts i/m/s/u flags and rejects other or duplicate flags.
   */
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
  type NarravaReactionPayloadFactory<Context> = Context extends undefined
    ? () => NarravaData
    : (context: Context) => NarravaData
  interface NarravaReactionEffect<Context> {
    readonly id: string
    /**
     * 以触发时的当前 Passage 过滤规则；适用于 Event、State 与 lifecycle。
     *
     * Filter by the current Passage at trigger time; applies to Event, State, and lifecycle rules.
     */
    readonly passage?:
      | NarravaReactionPassageMatcher
      | readonly NarravaReactionPassageMatcher[]
      | NarravaReactionPassageSelector
    /**
     * 通过 Engine 事务导航；目标 Passage 会正常经历 lifecycle 与 history。
     *
     * Navigate through an Engine transaction, including the target Passage lifecycle and history.
     */
    readonly goto?: string
    /**
     * 继续派发结构化 Event；Runtime 会检测后代环并限制执行次数。
     *
     * Emit another structured event; the Runtime detects descendant cycles and limits executions.
     */
    readonly emit?: {
      readonly name: string
      readonly payload?: NarravaData | NarravaReactionPayloadFactory<Context>
    }
    /**
     * 仅 lifecycle Reaction 可用；在 Reaction Phase 截断目标 Passage 原正文。
     *
     * Available only to lifecycle rules; stop the target Passage body during the Reaction phase.
     */
    readonly exit?: true
    readonly enabled?: boolean
    readonly once?: boolean
    /**
     * 成功次数上限；达到后保持禁用，必须 reset 后才能再次 enable。
     *
     * Maximum successful triggers; after reaching the limit, reset is required before enabling again.
     */
    readonly limit?: number
    readonly tags?: readonly string[]
  }
  type NarravaReactionContent =
    | {
        /**
         * Twee Widget 调用源码；可直接追加，也可替换稳定目标。
         *
         * Twee Widget invocation source; append it or replace a stable target.
         */
        readonly widget: string
        readonly include?: never
        readonly replace?: string
      }
    | {
        /**
         * 原地执行 Passage fragment；无 replace 时追加到当前输出。
         *
         * Execute a Passage fragment in place; append to the current output when replace is absent.
         */
        readonly include: string
        readonly replace?: string
        readonly widget?: never
      }
    | { readonly widget?: never; readonly include?: never; readonly replace?: never }
  type NarravaEventReactionDefinition = NarravaReactionEffect<NarravaData> &
    NarravaReactionContent & {
      readonly event: string
      readonly state?: never
      readonly lifecycle?: never
      readonly exit?: never
      /**
       * 可同时检查 Event payload 与当前活动 State（例如 `V.quest_open === true`）。
       *
       * Inspect both the event payload and active State, for example V.quest_open === true.
       */
      readonly cond?: (payload: NarravaData) => boolean
    }
  interface NarravaReactionStateChange {
    readonly before: NarravaData
    readonly after: NarravaData
  }
  type NarravaStateReactionDefinition = NarravaReactionEffect<NarravaReactionStateChange> &
    NarravaReactionContent & {
      readonly event?: never
      readonly state: `$${string}`
      readonly lifecycle?: never
      readonly exit?: never
      /**
       * 可同时检查本路径变化与当前活动 State 中的其他变量。
       *
       * Inspect this path change together with other active State variables.
       */
      readonly cond?: (change: NarravaReactionStateChange) => boolean
    }
  type NarravaLifecycleReactionDefinition = NarravaReactionEffect<undefined> &
    NarravaReactionContent & {
      readonly event?: never
      readonly state?: never
      readonly lifecycle: true
      /**
       * 可通过 `V` 检查进入 Passage 时的当前活动 State。
       *
       * Inspect active State when entering the Passage, for example through V.
       */
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
  /**
   * 声明式叙事反应规则；cond 与动态 emit.payload 内禁止 add/enable/disable/reset。
   *
   * Declarative narrative reactions; add/enable/disable/reset are forbidden inside cond and dynamic emit.payload callbacks.
   */
  const Reaction: {
    /** 注册声明式反应规则。 Register a declarative reaction. */
    add(definition: NarravaReactionDefinition): NarravaReactionStatus
    /**
     * 回调内读取本轮解析开始时的状态；其他调用读取当前状态。
     *
     * Callbacks read the state at the start of this resolution; other calls read the current state.
     */
    get(id: string): NarravaReactionStatus | undefined
    /** 启用已注册规则。 Enable a registered reaction. */
    enable(id: string): boolean
    /** 停用规则并保留定义。 Disable a reaction while keeping its definition. */
    disable(id: string): boolean
    /** 重置规则的触发次数。 Reset the reaction execution count. */
    reset(id: string): boolean
  }

  /**
   * Macro.before/after 订阅返回的不透明句柄。
   *
   * An opaque subscription handle returned by Macro.before/after.
   */
  type NarravaMacroSubscription = number & { readonly __macroSubscription: unique symbol }
  /**
   * 宏调用上下文：宏名、参数（列表或原始字符串）与容器宏正文。
   *
   * Macro call context: name, arguments (a list or raw string), and optional container body.
   */
  interface NarravaMacroCall {
    readonly name: string
    readonly arguments: readonly NarravaData[] | string
    readonly body?: string
  }
  /**
   * 宏定义：body 决定原地展开还是包裹正文，arguments 决定参数形态， execution 决定 handler 是否可返回 Promise。
   *
   * Macro definition: body selects inline or container form, arguments selects its input shape, and execution controls Promise support.
   */
  interface NarravaMacroDefinition {
    readonly body: "inline" | "container"
    readonly arguments: "raw" | "list"
    /**
     * `async` 允许返回 Promise；需要等待时间时使用 `Host.delay()`。
     *
     * async allows a Promise result; use Host.delay() for timed waits.
     */
    readonly execution: "sync" | "async"
    readonly handler: (
      call: NarravaMacroCall,
    ) => NarravaData | NarravaSurfaceNode | Promise<NarravaData | NarravaSurfaceNode>
  }
  /**
   * 作者宏注册表：脚本用 Macro.add 定义的新宏可在 .twee 中调用。
   *
   * The author macro registry; macros registered with Macro.add are callable from .twee.
   */
  interface NarravaMacro {
    /**
     * 注册宏；返回同名旧定义（不存在时为 undefined）。
     *
     * Register a macro and return its previous definition, or undefined if absent.
     */
    add(name: string, definition: NarravaMacroDefinition): NarravaMacroDefinition | undefined
    /**
     * 替换已存在的宏；宏不存在时抛错，返回旧定义。
     *
     * Replace an existing macro and return its previous definition; throws if absent.
     */
    update(name: string, definition: NarravaMacroDefinition): NarravaMacroDefinition
    /**
     * 删除宏；返回被删除的定义（不存在时为 undefined）。
     *
     * Delete a macro and return its definition, or undefined if absent.
     */
    del(name: string): NarravaMacroDefinition | undefined
    /** 按名称读取宏定义。 Read a macro definition by name. */
    get(name: string): NarravaMacroDefinition | undefined
    /** 检查名称是否存在于此接口的集合中。 Check whether this named entry exists. */
    has(name: string): boolean
    /**
     * 在宏执行前调用 hook，可观察但不能改写输出。
     *
     * Run a hook before execution; it may observe the call but cannot rewrite the output.
     */
    before(name: string, hook: (call: NarravaMacroCall) => void): NarravaMacroSubscription
    /**
     * 在宏输出后调用 hook，返回的值为新的输出。
     *
     * Run a hook after output; its return value becomes the new output.
     */
    after(
      name: string,
      hook: (output: NarravaData, call: NarravaMacroCall) => NarravaData,
    ): NarravaMacroSubscription
    /**
     * 注销 before/after 订阅；返回是否成功。
     *
     * Remove a before/after subscription and report whether it was removed.
     */
    off(subscription: NarravaMacroSubscription): boolean
  }
  const Macro: NarravaMacro

  /**
   * 引擎导航请求：goto/back/forward/restart 在当前事务结束后由 Host 执行。
   *
   * Navigation requests: the host executes goto/back/forward/restart after the current transaction.
   */
  interface NarravaEngine {
    /** 当前游戏是否已启动。 Whether the story has started. */
    readonly started: boolean
    /** 前往指定 Passage，保留当前变量。 Navigate to a passage, keeping current variables.
     * @example Engine.goto("Start") */
    goto(target: string): void
    /** 返回上一条历史记录。 Restore the previous history entry.
     * @example Engine.back() */
    back(): void
    /** 前往下一条历史记录。 Restore the next history entry.
     * @example Engine.forward() */
    forward(): void
    /** 恢复启动状态并重新开始游戏。 Restore startup state and start a new game.
     * @example Engine.restart() */
    restart(): void
  }
  const Engine: NarravaEngine

  /**
   * Passage 元数据快照：名称与 Tag 列表。
   *
   * A Passage metadata snapshot: name and tags.
   */
  interface NarravaPassageInfo {
    readonly name: string
    readonly tags: readonly string[]
  }
  /**
   * 只读 Story 查询：has/get/current/visits。
   *
   * Readonly Story queries: has/get/current/visits.
   */
  interface NarravaStory {
    /** 检查名称是否存在于此接口的集合中。 Check whether this named entry exists. */
    has(name: string): boolean
    /** 取得当前 Passage 的名称与标签。 Get the current passage name and tags. */
    current(): NarravaPassageInfo | undefined
    /** 按名称查询 Passage 元数据。 Look up passage metadata by name. */
    get(name: string): NarravaPassageInfo | undefined
    /** 查询 Passage 的访问次数。 Count visits to a passage. */
    visits(name: string): number
  }
  const Story: NarravaStory

  /**
   * 日志级别，由低到高。
   *
   * Log levels in ascending severity.
   */
  type NarravaLogLevel = "trace" | "debug" | "info" | "warn" | "error"
  /**
   * Logger.subscribe 预留的不透明句柄；订阅投递尚未实现。
   *
   * An opaque handle reserved for Logger.subscribe; subscription delivery is not implemented yet.
   */
  type NarravaLogSubscription = number & { readonly __logSubscription: unique symbol }
  /**
   * 单条日志记录：序号、级别、target 与消息。
   *
   * A log record: sequence, level, target, and message.
   */
  interface NarravaLogRecord {
    readonly sequence: number
    readonly level: NarravaLogLevel
    readonly target: string
    readonly message: string
  }
  /**
   * 按 target 记录有界日志，支持筛选订阅与读取。
   *
   * Record bounded logs by target; subscribe to and consume filtered records.
   */
  interface NarravaLogger {
    /** 记录跟踪日志，target 为来源标签。 Record a trace message under a source target. */
    trace(target: string, message: string): void
    /** 记录调试日志，target 为来源标签。 Record a debug message under a source target. */
    debug(target: string, message: string): void
    /** 记录信息日志，target 为来源标签。 Record a info message under a source target. */
    info(target: string, message: string): void
    /** 记录警告日志，target 为来源标签。 Record a warn message under a source target. */
    warn(target: string, message: string): void
    /** 记录错误日志，target 为来源标签。 Record a error message under a source target. */
    error(target: string, message: string): void
    /**
     * 订阅后续符合条件的日志。
     *
     * Subscribe to subsequent matching log records.
     */
    subscribe(filter?: { minimumLevel?: NarravaLogLevel; target?: string }): NarravaLogSubscription
    /**
     * 取走该订阅的待处理日志；订阅不存在时返回 undefined。
     *
     * Drain pending records; return undefined for an unknown subscription.
     */
    take(subscription: NarravaLogSubscription): NarravaLogRecord[] | undefined
    /**
     * 取消订阅，返回是否成功移除。
     *
     * Unsubscribe and report whether the subscription existed.
     */
    unsubscribe(subscription: NarravaLogSubscription): boolean
  }
  const Logger: NarravaLogger

  /**
   * Event.subscribe 返回的不透明句柄。
   *
   * An opaque subscription handle returned by Event.subscribe.
   */
  type NarravaEventSubscription = number & { readonly __eventSubscription: unique symbol }
  /**
   * Engine 保留的五个 Passage 生命周期事件名，脚本不可 emit。
   *
   * The five Engine-reserved Passage lifecycle names; scripts cannot emit them.
   */
  type NarravaPassageEventName = NarravaBuiltinEventName
  /**
   * Passage 生命周期事件的载荷：当前 Passage 名与 Tag 列表。
   *
   * Passage lifecycle payload: the current Passage name and tags.
   */
  interface NarravaPassageEventPayload {
    readonly passage: string
    readonly tags: readonly string[]
  }
  interface NarravaEventRecord {
    /**
     * 当前游戏 Runtime 内单调递增的事件序号。
     *
     * Monotonically increasing sequence within the current game runtime.
     */
    readonly sequence: number
    /**
     * 作者定义的事件名；精确匹配并区分大小写。
     *
     * Exact, case-sensitive author-defined event name.
     */
    readonly name: string
    /**
     * 发布事件时携带的数据。
     *
     * Data supplied when the event was published.
     */
    readonly payload: NarravaData
  }
  /**
   * 作者事件总线：emit 返回记录序号；订阅只接收之后发生的事件。
   *
   * The author event bus: emit returns a sequence number; subscriptions receive future events only.
   */
  interface NarravaEvent {
    /**
     * 发布作者事件；五个 passage:* 名称由 Engine 保留。
     *
     * Emit an author-defined event. The five passage:* names are Engine-reserved.
     */
    emit(name: string, payload?: NarravaData): number
    /**
     * 只订阅之后发生的事件；省略 name 时接收作者和 Engine 事件。
     *
     * Subscribe to future events only; omit name to receive both author and Engine events.
     */
    subscribe(filter?: { name?: string }): NarravaEventSubscription
    /**
     * 取走待消费记录；仅当订阅不存在时返回 undefined。
     *
     * Drain pending records; returns undefined only if the subscription does not exist.
     */
    take(subscription: NarravaEventSubscription): NarravaEventRecord[] | undefined
    /** 移除事件订阅及其待处理队列。 Remove an event subscription and its pending queue. */
    unsubscribe(subscription: NarravaEventSubscription): boolean
  }
  const Event: NarravaEvent

  /**
   * Host 能力入口；目前只有 delay（配合 async 宏做时间等待）。
   *
   * Host capabilities; currently delay supports timed waits in async macros.
   */
  interface NarravaHost {
    /**
     * 挂起当前 Engine 事务并在 delay 毫秒后恢复；取值范围 0..=86400000。
     *
     * Suspend the current Engine transaction and resume after the given milliseconds; valid range is 0..86400000.
     */
    delay(milliseconds: number): Promise<void>
  }
  const Host: NarravaHost

  /**
   * Resource 元数据：逻辑路径、媒体类型与字节数。
   *
   * Resource metadata: logical path, media type, and byte size.
   */
  interface NarravaResourceInfo {
    readonly path: string
    readonly mediaType: string
    readonly size: number
  }
  /**
   * 只读 Resource 查询：路径列表、存在性、候选选取与读取。
   *
   * Readonly Resource queries: paths, existence, candidate selection, and content.
   */
  interface NarravaResource {
    /** 列出已装载资源路径。 List loaded resource paths. */
    paths(): readonly string[]
    /** 检查资源路径是否存在。 Check whether a resource path exists. */
    has(path: string): boolean
    /**
     * 按顺序返回第一个存在的候选路径。
     *
     * Return the first existing candidate path, in order.
     */
    pick(candidates: readonly string[]): string | undefined
    /** 查询资源类型与大小。 Inspect the resource media type and size. */
    info(path: string): NarravaResourceInfo | undefined
    /**
     * 读取原始字节；路径不存在时为 undefined。
     *
     * Read raw bytes; returns undefined if the path does not exist.
     */
    read(path: string): Uint8Array | undefined
    /**
     * 按 UTF-8 读取为文本；路径不存在或非文本时为 undefined。
     *
     * Read UTF-8 text; returns undefined if the path is missing or not text.
     */
    text(path: string): string | undefined
  }
  const Resource: NarravaResource

  /**
   * 存档操作：capture/restore 由脚本直接读写 variables，export/import 请求 Host 文件操作。
   *
   * Save operations: capture/restore access variables directly; export/import request host file operations.
   */
  type NarravaSaveOperation = "capture" | "restore" | "export" | "import"
  /**
   * Save.before/after 订阅返回的不透明句柄。
   *
   * An opaque subscription handle returned by Save.before/after.
   */
  type NarravaSaveSubscription = number & { readonly __saveSubscription: unique symbol }
  /**
   * before hook 上下文：操作与目标（export/import 时存在）。
   *
   * Before-hook context: operation and optional target for export/import.
   */
  interface NarravaSaveBeforeContext {
    readonly operation: NarravaSaveOperation
    readonly target?: string
  }
  /**
   * after hook 的完成结果：操作、目标与成败。
   *
   * An after-hook result: operation, target, and success or failure.
   */
  interface NarravaSaveCompletion {
    readonly operation: NarravaSaveOperation
    readonly target?: string
    readonly succeeded: boolean
    readonly error?: string
  }
  /**
   * 存档入口：capture/restore 覆盖 variables 命名空间；export/import 走 Host。
   *
   * Save access: capture/restore covers variables; export/import goes through the host.
   */
  interface NarravaSave {
    /**
     * 生成当前 variables 的存档 JSON 字符串。
     *
     * Serialize the current variables namespace as save JSON.
     */
    capture(): string
    /**
     * 用存档 JSON 整体替换 variables；非法 JSON 抛错。
     *
     * Replace variables with save JSON; throws for invalid JSON.
     */
    restore(json: string): void
    /**
     * 请求 Host 把存档导出到 target（默认 manual）。
     *
     * Request a host save export to target (manual by default).
     * @example Save.export("manual")
     */
    export(target?: string): void
    /**
     * 请求 Host 从 target 导入存档。
     *
     * Request a host save import from target.
     * @example Save.import("manual")
     */
    import(target?: string): void
    /**
     * 操作前调用；返回字符串可改写 export/import 的目标。
     *
     * Run before an operation; returning a string rewrites the export/import target.
     */
    before(
      operation: NarravaSaveOperation,
      hook: (context: NarravaSaveBeforeContext) => string | void,
    ): NarravaSaveSubscription
    /**
     * 操作获得实际完成结果后才调用。
     *
     * Run only after an operation has an actual completion result.
     */
    after(
      operation: NarravaSaveOperation,
      hook: (completion: NarravaSaveCompletion) => void,
    ): NarravaSaveSubscription
    /** 移除存档钩子订阅。 Remove a save hook subscription. */
    off(subscription: NarravaSaveSubscription): boolean
  }
  const Save: NarravaSave

  /**
   * 本地化信息、语言切换请求与翻译模板导出。
   *
   * Locale information, language change requests, and translation template export.
   */
  interface NarravaI18n {
    /** 游戏配置的默认语言。 Default language from game configuration. */
    readonly defaultLocale: string
    /** 当前已生效的语言。 Currently active language. */
    readonly locale: string
    /**
     * 请求 Host 切换运行语言；成功后 locale 与后续渲染同步更新。
     *
     * Request a host language change; on success, locale and subsequent rendering update together.
     */
    select(locale: string): void
    /**
     * 以格式化 JSON 返回完整翻译模板。
     *
     * Return the complete translator template as formatted JSON.
     */
    export(): string
  }
  const I18n: NarravaI18n

  /**
   * 声明当前作用域所需的音频；事务成功后由 Host 接收播放差异，不进入 Surface 或存档。
   *
   * Declare desired audio for the current scope. After a successful transaction, the host receives playback changes; audio is not stored in Surface or saves.
   */
  const Audio: {
    /**
     * 声明资源与通道；默认 channel 为 bgm、loop 为 true、volume 为 1（范围 0..1）。
     * tags 非空时至少匹配当前 Passage 的一个 Tag 才生效；相同播放声明不会重新启动音频。
     *
     * Declare a resource and channel; defaults are channel bgm, loop true, and volume 1 (range 0..1).
     * Nonempty tags must match at least one current Passage tag; identical playback declarations do not restart audio.
     */
    play(
      resource: string,
      options?: { channel?: string; loop?: boolean; volume?: number; tags?: readonly string[] },
    ): void
    /**
     * 声明停止指定通道；随当前作用域的音频声明一起生效。
     *
     * Declare that a channel should stop, alongside the current scope's audio declarations.
     */
    stop(channel: string): void
  }
}

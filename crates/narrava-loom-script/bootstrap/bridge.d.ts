/** Rust 安装到 Boa globalThis 的原生 bridge；只在 bootstrap 内部可见。 */
declare function __narravaStateGet(namespace: string, key: string): unknown
declare function __narravaStateHas(namespace: string, key: string): boolean
declare function __narravaStateSet(namespace: string, key: string, value: unknown): unknown
declare function __narravaStateDel(namespace: string, key: string): unknown
declare function __narravaStateSnapshot(namespace: string): Record<string, unknown>
declare function __narravaStateReplace(namespace: string, values: Record<string, unknown>): void

declare function __narravaLocationAdd(place: NarravaPlaceDefinition): void
declare function __narravaLocationGet(id: string): NarravaPlace | undefined
declare function __narravaLocationPlaces(): readonly NarravaPlace[]
declare function __narravaLocationLocate(point: NarravaLocationPoint): readonly NarravaPlace[]
declare function __narravaLocationCurrent(): NarravaLocationPosition | null
declare function __narravaLocationMove(point: NarravaLocationPoint): NarravaLocationPosition

declare function __narravaReactionAdd(definition: string): string
declare function __narravaReactionGet(id: string): string | undefined
declare function __narravaReactionEnable(id: string): boolean
declare function __narravaReactionDisable(id: string): boolean
declare function __narravaReactionReset(id: string): boolean

declare function __narravaResourcePaths(): string[]
declare function __narravaResourceHas(path: string): boolean
declare function __narravaResourceInfo(path: string): unknown
declare function __narravaResourceRead(path: string): number[] | undefined
declare function __narravaResourceText(path: string): string | undefined

/** Core 保存的确定性随机序列。 */
declare function __narravaRandomNext(): number
declare function __narravaRandomSeed(seed: number): void
declare function __narravaRandomCurrent(): { seed: string; state: string }

/** 不调用 Proxy trap 的原生检查。 */
declare function __narravaConsoleIsProxy(value: unknown): boolean

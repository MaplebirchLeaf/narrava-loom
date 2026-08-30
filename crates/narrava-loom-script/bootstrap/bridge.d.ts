/** Rust 安装到 Boa globalThis 的原生 bridge；只在 bootstrap 内部可见。 */
declare function __narravaStateGet(namespace: string, key: string): unknown
declare function __narravaStateHas(namespace: string, key: string): boolean
declare function __narravaStateSet(namespace: string, key: string, value: unknown): unknown
declare function __narravaStateDel(namespace: string, key: string): unknown
declare function __narravaStateSnapshot(namespace: string): Record<string, unknown>
declare function __narravaStateReplace(namespace: string, values: Record<string, unknown>): void

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

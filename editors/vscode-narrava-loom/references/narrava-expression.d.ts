/**
 * Narrava Loom Twee Expression API reference.
 *
 * This file is a navigation/documentation target for `.twee`; these declarations are not
 * JavaScript globals and are deliberately not included by the game-script tsconfig.
 */
declare namespace NarravaExpressionReference {
  interface Globals {
    /** 返回绝对值。 */ abs(value: number): number
    /** 按 Expression 真值规则转换。 */ boolean(value: unknown): boolean
    /** 向上取整。 */ ceil(value: number): number
    /** 限制到闭区间。 */ clamp(value: number, minimum: number, maximum: number): number
    /** 深拷贝值图，断开原 Array/Object 引用并保留拷贝内部共享和循环。 */ clone<T>(value: T): T
    /** 值不为 undefined 时返回 true。 */ defined(value: unknown): boolean
    /** 判断空值、空字符串或空集合。 */ empty(value: unknown): boolean
    /** 返回对象的有序键值对。 */ entries(value: object): [string, unknown][]
    /** 使用 Runtime 随机源选择一个参数。 */ either<T>(first: T, ...rest: T[]): T
    /** 向下取整。 */ floor(value: number): number
    /** 返回对象的有序键。 */ keys(value: object): string[]
    /** 返回最大值。 */ max(first: number, ...rest: number[]): number
    /** 返回最小值。 */ min(first: number, ...rest: number[]): number
    /** 按 Expression 规则转为数值。 */ number(value: unknown): number
    /** 返回 0（含）到 1（不含）的伪随机数。 */ random(): number
    /** 按 Web 语义四舍五入。 */ round(value: number): number
    /** 按 Expression 规则转为字符串。 */ string(value: unknown): string
    /** 返回对象的有序值。 */ values(value: object): unknown[]
  }
  interface ObjectNamespace {
    /** 按顺序合并自有属性到可写目标。 */ assign<T extends object>(
      target: T,
      ...sources: object[]
    ): T
    /** 判断指定自有属性是否存在。 */ hasOwn(value: object, key: unknown): boolean
  }
  interface ArrayValue<T> {
    readonly length: number
    at(index: number): T | undefined
    concat(...values: (T | T[])[]): T[]
    includes(value: T): boolean
    indexOf(value: T, fromIndex?: number): number
    join(separator?: string): string
    pop(): T | undefined
    push(...values: T[]): number
    shift(): T | undefined
    slice(start?: number, end?: number): T[]
    splice(start: number, deleteCount?: number, ...values: T[]): T[]
    unshift(...values: T[]): number
  }
  interface StringValue {
    readonly length: number
    endsWith(search: string): boolean
    includes(search: string): boolean
    slice(start?: number, end?: number): string
    split(separator?: string, limit?: number): string[]
    startsWith(search: string): boolean
    toLowerCase(): string
    toUpperCase(): string
    trim(): string
  }
}

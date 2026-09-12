/**
 * Narrava Twee 表达式的跳转与说明参考；这些声明不属于 JavaScript 全局 API，
 * 不应加入游戏脚本的 tsconfig。
 *
 * Navigation and documentation reference for Narrava Twee expressions. These declarations
 * are not JavaScript globals and must not be included in the game-script tsconfig.
 */
declare namespace NarravaExpressionReference {
  interface Globals {
    /**
     * 返回数值的绝对值。
     *
     * Return the absolute value.
     */
    abs(value: number): number
    /**
     * 按 Narrava Expression 真值规则转为布尔值。
     *
     * Convert using Narrava Expression truthiness rules.
     */
    boolean(value: unknown): boolean
    /**
     * 向上取整。
     *
     * Round up to an integer.
     */
    ceil(value: number): number
    /**
     * 把数值限制在闭区间内。
     *
     * Clamp a number to the inclusive range.
     */
    clamp(value: number, minimum: number, maximum: number): number
    /**
     * 深拷贝完整值图：断开与原 Array/Object 的引用，保留拷贝内部的共享引用和循环。
     *
     * Deep-copy the value graph, detaching original Array/Object references while preserving shared references and cycles within the copy.
     */
    clone<T>(value: T): T
    /**
     * 判断值是否不为 undefined。
     *
     * Return whether the value is not undefined.
     */
    defined(value: unknown): boolean
    /**
     * 判断值是否为空值、空字符串或空集合。
     *
     * Check for a nullish value, empty string, or empty collection.
     */
    empty(value: unknown): boolean
    /**
     * 按属性顺序返回对象键值对。
     *
     * Return object entries in property order.
     */
    entries(value: object): [string, unknown][]
    /**
     * 使用 Runtime 随机源等概率选择一个参数。
     *
     * Choose an argument uniformly using the Runtime random source.
     */
    either<T>(first: T, ...rest: T[]): T
    /**
     * 向下取整。
     *
     * Round down to an integer.
     */
    floor(value: number): number
    /**
     * 按属性顺序返回对象键。
     *
     * Return object keys in property order.
     */
    keys(value: object): string[]
    /**
     * 返回最大数值。
     *
     * Return the largest number.
     */
    max(first: number, ...rest: number[]): number
    /**
     * 返回最小数值。
     *
     * Return the smallest number.
     */
    min(first: number, ...rest: number[]): number
    /**
     * 按 Narrava Expression 规则转为数值。
     *
     * Convert to a number using Narrava Expression rules.
     */
    number(value: unknown): number
    /**
     * 使用 Runtime 随机源返回 0（含）到 1（不含）的数值。
     *
     * Use the Runtime random source to return a number in [0, 1).
     */
    random(): number
    /**
     * 按 Web 数值语义四舍五入。
     *
     * Round to the nearest integer using Web number semantics.
     */
    round(value: number): number
    /**
     * 按 Narrava Expression 规则转为字符串。
     *
     * Convert to a string using Narrava Expression rules.
     */
    string(value: unknown): string
    /**
     * 按属性顺序返回对象值。
     *
     * Return object values in property order.
     */
    values(value: object): unknown[]
  }
  interface ObjectNamespace {
    /**
     * 把源对象的自有属性按顺序写入可变目标。
     *
     * Copy own properties from each source into a mutable target, in order.
     */
    assign<T extends object>(target: T, ...sources: object[]): T
    /**
     * 判断对象是否拥有指定自有属性。
     *
     * Check whether an object has the specified own property.
     */
    hasOwn(value: object, key: unknown): boolean
  }
  interface ArrayValue<T> {
    /**
     * 数组元素数量。
     *
     * Number of array elements.
     */
    readonly length: number
    /**
     * 按 Web 索引规则读取元素，负数从末尾计算。
     *
     * Read an element using Web index rules; negative indices count from the end.
     */
    at(index: number): T | undefined
    /**
     * 返回一层展开后的新数组。
     *
     * Return a new array, flattening array arguments by one level.
     */
    concat(...values: (T | T[])[]): T[]
    /**
     * 判断数组是否包含指定值。
     *
     * Check whether the array contains a value.
     */
    includes(value: T): boolean
    /**
     * 返回指定值的首个索引，未找到时为 -1。
     *
     * Return the first matching index, or -1 if absent.
     */
    indexOf(value: T, fromIndex?: number): number
    /**
     * 把数组元素连接为字符串。
     *
     * Join array elements into a string.
     */
    join(separator?: string): string
    /**
     * 删除并返回末尾元素；需要可写引用。
     *
     * Remove and return the last element; requires a writable reference.
     */
    pop(): T | undefined
    /**
     * 在末尾追加元素并返回新长度；需要可写引用。
     *
     * Append elements and return the new length; requires a writable reference.
     */
    push(...values: T[]): number
    /**
     * 删除并返回首元素；需要可写引用。
     *
     * Remove and return the first element; requires a writable reference.
     */
    shift(): T | undefined
    /**
     * 返回指定区间的新数组。
     *
     * Return a new array for the specified range.
     */
    slice(start?: number, end?: number): T[]
    /**
     * 原地删除或插入元素，返回被删除项；需要可写引用。
     *
     * Remove or insert elements in place and return removed items; requires a writable reference.
     */
    splice(start: number, deleteCount?: number, ...values: T[]): T[]
    /**
     * 在开头插入元素并返回新长度；需要可写引用。
     *
     * Prepend elements and return the new length; requires a writable reference.
     */
    unshift(...values: T[]): number
  }
  interface StringValue {
    /**
     * 字符串的 UTF-16 码元数量。
     *
     * Number of UTF-16 code units in the string.
     */
    readonly length: number
    /**
     * 判断字符串是否以指定文本结尾。
     *
     * Check whether the string ends with the specified text.
     */
    endsWith(search: string): boolean
    /**
     * 判断字符串是否包含指定文本。
     *
     * Check whether the string contains the specified text.
     */
    includes(search: string): boolean
    /**
     * 按 UTF-16 码元返回指定区间。
     *
     * Return the specified range of UTF-16 code units.
     */
    slice(start?: number, end?: number): string
    /**
     * 按分隔符拆分字符串。
     *
     * Split a string by the separator.
     */
    split(separator?: string, limit?: number): string[]
    /**
     * 判断字符串是否以指定文本开头。
     *
     * Check whether the string starts with the specified text.
     */
    startsWith(search: string): boolean
    /**
     * 返回小写字符串。
     *
     * Return a lowercase string.
     */
    toLowerCase(): string
    /**
     * 返回大写字符串。
     *
     * Return an uppercase string.
     */
    toUpperCase(): string
    /**
     * 删除首尾空白。
     *
     * Remove leading and trailing whitespace.
     */
    trim(): string
  }
}

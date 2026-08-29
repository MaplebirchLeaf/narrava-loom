"use strict"

// Twee Expression 原生 API 目录。这些不是 JavaScript 全局值；目录只用于
// .twee 的悬停说明、补全与定义跳转，签名须与 Core evaluator 保持一致。

const globals = [
  ["abs", "abs(value: number): number", "返回数值的绝对值。"],
  ["boolean", "boolean(value: unknown): boolean", "按 Narrava Expression 真值规则转为布尔值。"],
  ["ceil", "ceil(value: number): number", "向上取整。"],
  [
    "clamp",
    "clamp(value: number, minimum: number, maximum: number): number",
    "把数值限制在闭区间内。",
  ],
  [
    "clone",
    "clone<T>(value: T): T",
    "深拷贝完整值图：断开与原 Array/Object 的引用，保留拷贝内部的共享引用和循环。",
  ],
  ["defined", "defined(value: unknown): boolean", "判断值是否不为 undefined。"],
  ["empty", "empty(value: unknown): boolean", "判断值是否为空值、空字符串或空集合。"],
  ["entries", "entries(value: object): [string, unknown][]", "按属性顺序返回对象键值对。"],
  ["either", "either<T>(first: T, ...rest: T[]): T", "使用 Runtime 随机源等概率选择一个参数。"],
  ["floor", "floor(value: number): number", "向下取整。"],
  ["keys", "keys(value: object): string[]", "按属性顺序返回对象键。"],
  ["max", "max(first: number, ...rest: number[]): number", "返回最大数值。"],
  ["min", "min(first: number, ...rest: number[]): number", "返回最小数值。"],
  ["number", "number(value: unknown): number", "按 Narrava Expression 规则转为数值。"],
  ["random", "random(): number", "使用 Runtime 随机源返回 0（含）到 1（不含）的数值。"],
  ["round", "round(value: number): number", "按 Web 数值语义四舍五入。"],
  ["string", "string(value: unknown): string", "按 Narrava Expression 规则转为字符串。"],
  ["values", "values(value: object): unknown[]", "按属性顺序返回对象值。"],
]

const namespace = [
  [
    "Object.assign",
    "Object.assign<T extends object>(target: T, ...sources: object[]): T",
    "把源对象的自有属性按顺序写入可变目标。",
  ],
  [
    "Object.hasOwn",
    "Object.hasOwn(value: object, key: unknown): boolean",
    "判断对象是否拥有指定自有属性。",
  ],
]

const arrays = [
  ["at", "at(index: number): T | undefined", "按 Web 索引规则读取元素，负数从末尾计算。"],
  ["concat", "concat(...values: (T | T[])[]): T[]", "返回一层展开后的新数组。"],
  ["includes", "includes(value: T): boolean", "判断数组是否包含指定值。"],
  [
    "indexOf",
    "indexOf(value: T, fromIndex?: number): number",
    "返回指定值的首个索引，未找到时为 -1。",
  ],
  ["join", "join(separator?: string): string", "把数组元素连接为字符串。"],
  ["pop", "pop(): T | undefined", "删除并返回末尾元素；需要可写引用。"],
  ["push", "push(...values: T[]): number", "在末尾追加元素并返回新长度；需要可写引用。"],
  ["shift", "shift(): T | undefined", "删除并返回首元素；需要可写引用。"],
  ["slice", "slice(start?: number, end?: number): T[]", "返回指定区间的新数组。"],
  [
    "splice",
    "splice(start: number, deleteCount?: number, ...values: T[]): T[]",
    "原地删除或插入元素，返回被删除项；需要可写引用。",
  ],
  ["unshift", "unshift(...values: T[]): number", "在开头插入元素并返回新长度；需要可写引用。"],
]

const strings = [
  ["endsWith", "endsWith(search: string): boolean", "判断字符串是否以指定文本结尾。"],
  ["includes", "includes(search: string): boolean", "判断字符串是否包含指定文本。"],
  ["slice", "slice(start?: number, end?: number): string", "按 UTF-16 码元返回指定区间。"],
  ["split", "split(separator?: string, limit?: number): string[]", "按分隔符拆分字符串。"],
  ["startsWith", "startsWith(search: string): boolean", "判断字符串是否以指定文本开头。"],
  ["toLowerCase", "toLowerCase(): string", "返回小写字符串。"],
  ["toUpperCase", "toUpperCase(): string", "返回大写字符串。"],
  ["trim", "trim(): string", "删除首尾空白。"],
]

const make = (name, signature, description, kind) =>
  Object.freeze({ name, signature, description, kind })
const EXPRESSION_APIS = Object.freeze([
  ...globals.map((item) => make(...item, "global")),
  ...namespace.map((item) => make(...item, "namespace")),
  ...arrays.map(([name, signature, description]) =>
    make(`array.${name}`, signature, description, "array"),
  ),
  ...strings.map(([name, signature, description]) =>
    make(`string.${name}`, signature, description, "string"),
  ),
])

/** 精确名称查询，供测试和目录消费。 */
function expressionApi(name) {
  return EXPRESSION_APIS.find((api) => api.name === name)
}

/** 把 Twee 调用链解析为可能的原生 API；无法静态确定 includes/slice 接收者时返回两个。 */
function resolveExpressionApis(callName) {
  const exact = expressionApi(callName)
  if (exact) return [exact]
  const method = callName.split(".").at(-1)
  return EXPRESSION_APIS.filter(
    (api) => ["array", "string"].includes(api.kind) && api.name.endsWith(`.${method}`),
  )
}

module.exports = { EXPRESSION_APIS, expressionApi, resolveExpressionApis }

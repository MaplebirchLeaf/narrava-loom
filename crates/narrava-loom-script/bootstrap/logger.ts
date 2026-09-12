import { scriptGlobals } from "./internal"

declare function __narravaLoggerLog(level: NarravaLogLevel, target: string, message: string): void
declare function __narravaLoggerSubscribe(filter?: {
  minimumLevel?: NarravaLogLevel
  target?: string
}): NarravaLogSubscription
declare function __narravaLoggerTake(
  subscription: NarravaLogSubscription,
): NarravaLogRecord[] | undefined
declare function __narravaLoggerUnsubscribe(subscription: NarravaLogSubscription): boolean

export default function logger(): void {
  scriptGlobals.Logger = Object.freeze({
    trace: (target: string, message: string) => __narravaLoggerLog("trace", target, message),
    debug: (target: string, message: string) => __narravaLoggerLog("debug", target, message),
    info: (target: string, message: string) => __narravaLoggerLog("info", target, message),
    warn: (target: string, message: string) => __narravaLoggerLog("warn", target, message),
    error: (target: string, message: string) => __narravaLoggerLog("error", target, message),
    subscribe: (filter) => __narravaLoggerSubscribe(filter),
    take: (subscription) => __narravaLoggerTake(subscription),
    unsubscribe: (subscription) => __narravaLoggerUnsubscribe(subscription),
  } satisfies NarravaLogger)
}

import { logRecords, scriptGlobals, subscriptionId } from "./internal"

export default function logger(): void {
  const methods = Object.fromEntries(
    ["trace", "debug", "info", "warn", "error"].map((level) => [
      level,
      (target: unknown, message: unknown) => logRecords.push({ level, target, message }),
    ]),
  )
  Object.assign(methods, {
    subscribe: () => subscriptionId(),
    take: () => [],
    unsubscribe: () => false,
  })
  scriptGlobals.Logger = Object.seal(methods)
}

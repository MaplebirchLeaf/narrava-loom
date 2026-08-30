import { allocateSubscription, globals, logs } from "./internal"

export function installLogger(): void {
  const logger = Object.fromEntries(
    ["trace", "debug", "info", "warn", "error"].map((level) => [
      level,
      (target: unknown, message: unknown) => logs.push({ level, target, message }),
    ]),
  )
  Object.assign(logger, {
    subscribe: () => allocateSubscription(),
    take: () => [],
    unsubscribe: () => false,
  })
  globals.Logger = Object.seal(logger)
}

import { scriptGlobals } from "./internal"

const next = (): number => __narravaRandomNext()

export default function random(): void {
  scriptGlobals.Random = Object.freeze({
    next,
    seed: (seed: number): void => __narravaRandomSeed(seed),
    current: (): { seed: string; state: string } => __narravaRandomCurrent(),
  })
  Object.defineProperty(Math, "random", { value: next, writable: false, configurable: false })
}

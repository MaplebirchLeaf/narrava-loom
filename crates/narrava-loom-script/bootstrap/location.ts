import type {} from "../../../bindings/typescript/narrava"
import { scriptGlobals } from "./internal"

/** 地点定义与当前坐标由 Core 持有；返回值都是独立快照。 */
export default function location(): void {
  scriptGlobals.Location = Object.freeze({
    add: (place: NarravaPlaceDefinition) => __narravaLocationAdd(place),
    get: (id: string) => __narravaLocationGet(id),
    places: () => __narravaLocationPlaces(),
    locate: (point: NarravaLocationPoint) => __narravaLocationLocate(point),
    current: () => __narravaLocationCurrent(),
    move: (point: NarravaLocationPoint) => __narravaLocationMove(point),
  } satisfies NarravaLocation)
}

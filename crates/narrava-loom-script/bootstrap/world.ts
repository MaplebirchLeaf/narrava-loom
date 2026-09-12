import type {} from "../../../bindings/typescript/narrava"
import { scriptGlobals } from "./internal"

/** 地点定义与当前坐标由 Core 持有；返回值都是独立快照。 */
export default function world(): void {
  scriptGlobals.World = Object.freeze({
    add: (place: NarravaPlaceDefinition) => __narravaWorldAdd(place),
    get: (id: string) => __narravaWorldGet(id),
    places: () => __narravaWorldPlaces(),
    locate: (point: NarravaWorldPoint) => __narravaWorldLocate(point),
    current: () => __narravaWorldCurrent(),
    move: (point: NarravaWorldPoint) => __narravaWorldMove(point),
  } satisfies NarravaWorld)
}

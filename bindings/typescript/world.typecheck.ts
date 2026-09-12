import type {} from "./narrava"

World.add({
  id: "hospital",
  name: "医院",
  parent: "town",
  bounds: [
    [10, 10],
    [20, 10],
    [20, 20],
    [10, 20],
  ],
  entry: [15, 15],
})

const point: NarravaWorldPoint = [15, 15]
const place: NarravaPlace | undefined = World.get("hospital")
const registered: readonly NarravaPlace[] = World.places()
const overlapping: readonly NarravaPlace[] = World.locate(point)
const previous: NarravaWorldPosition | null = World.current()
const moved: NarravaWorldPosition = World.move(point)
const environment: "inside" | "outside" | null = moved.environment

export { point, place, registered, overlapping, previous, moved, environment }

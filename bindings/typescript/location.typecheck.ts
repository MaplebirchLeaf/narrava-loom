import type {} from "./narrava"

Location.add({
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

const point: NarravaLocationPoint = [15, 15]
const place: NarravaPlace | undefined = Location.get("hospital")
const registered: readonly NarravaPlace[] = Location.places()
const overlapping: readonly NarravaPlace[] = Location.locate(point)
const previous: NarravaLocationPosition | null = Location.current()
const moved: NarravaLocationPosition = Location.move(point)
const environment: "inside" | "outside" | null = moved.environment

export { point, place, registered, overlapping, previous, moved, environment }

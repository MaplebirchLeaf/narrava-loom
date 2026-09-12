// parent 可以前向引用；所有坐标都在同一世界平面中。
World.add({
  id: "hospital",
  name: "医院",
  parent: "town",
  bounds: [
    [20, 20],
    [40, 20],
    [40, 40],
    [20, 40],
  ],
  entry: [30, 30],
})
World.add({
  id: "shop",
  name: "商店",
  parent: "town",
  bounds: [
    [60, 20],
    [80, 20],
    [80, 40],
    [60, 40],
  ],
})
World.add({
  id: "town",
  name: "小镇",
  bounds: [
    [0, 0],
    [100, 0],
    [100, 100],
    [0, 100],
  ],
  entry: [10, 10],
})

Macro.add("worldCurrent", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () => {
    const current = World.current()
    if (current === null) return "当前位置：尚未进入地点。"
    const place = World.get(current.place)
    const containing = World.locate(current.point).map(
      (item) => `${item.name ?? item.id} (${item.id})`,
    )
    return Surface.fragment(
      Surface.text(`当前地点：${place?.name ?? current.place} (${current.place})`),
      Surface.hardBreak(),
      Surface.text(`坐标：[${current.point.join(", ")}]；环境：${current.environment ?? "未指定"}`),
      Surface.hardBreak(),
      Surface.text(`所在范围：${containing.join(" → ")}`),
      Surface.hardBreak(),
      Surface.text(`已注册 ${World.places().length} 个地点。`),
    )
  },
})

function worldMoveTo(x: number, y: number): void {
  World.move([x, y])
}

State.global.extend({ worldMoveTo })

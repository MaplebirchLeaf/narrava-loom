/** 统一坐标中的地点 / Places in one coordinate plane. 坐标单位由示例定义。 */
const townPlaces: NarravaPlaceDefinition[] = [
  {
    id: "town_region",
    name: "小镇",
    bounds: [
      [-400, -160],
      [-20, -160],
      [-20, 160],
      [-400, 160],
    ],
  },
  {
    id: "residential_street",
    name: "住宅街",
    parent: "town_region",
    bounds: [
      [-350, -60],
      [-240, -60],
      [-240, 60],
      [-350, 60],
    ],
    entry: [-300, 0],
  },
  {
    id: "guesthouse",
    name: "旅舍",
    parent: "residential_street",
    bounds: [
      [-330, 5],
      [-290, 5],
      [-290, 35],
      [-330, 35],
    ],
    entry: [-310, 10],
  },
  {
    id: "market_street",
    name: "商业街",
    parent: "town_region",
    bounds: [
      [-210, -60],
      [-30, -60],
      [-30, 100],
      [-210, 100],
    ],
    entry: [-110, 0],
  },
  {
    id: "shopping_centre",
    name: "购物中心",
    parent: "market_street",
    bounds: [
      [-110, 10],
      [-80, 10],
      [-80, 40],
      [-110, 40],
    ],
    entry: [-95, 15],
  },
  {
    id: "clock_cafe",
    name: "钟楼咖啡馆",
    parent: "market_street",
    bounds: [
      [-180, -30],
      [-140, -30],
      [-140, 0],
      [-180, 0],
    ],
    entry: [-160, -10],
  },
  {
    id: "town_hospital",
    name: "医院",
    parent: "market_street",
    bounds: [
      [-90, 50],
      [-40, 50],
      [-40, 90],
      [-90, 90],
    ],
    entry: [-70, 60],
  },
  {
    id: "town_park",
    name: "公园",
    parent: "town_region",
    bounds: [
      [-200, 105],
      [-80, 105],
      [-80, 150],
      [-200, 150],
    ],
    entry: [-150, 120],
  },
  {
    id: "forest_edge",
    name: "森林入口",
    bounds: [
      [-650, -150],
      [-420, -150],
      [-420, 150],
      [-650, 150],
    ],
    entry: [-460, 0],
  },
  {
    id: "forest_trail",
    name: "林间小径",
    parent: "forest_edge",
    bounds: [
      [-620, -100],
      [-500, -100],
      [-500, 100],
      [-620, 100],
    ],
    entry: [-550, 0],
  },
]
for (const place of townPlaces) Location.add(place)

# 世界地点与坐标

`World` 提供地点、多边形范围和当前玩家位置，TUI 与 Tauri 使用相同的领域规则。
图形地图等能力的完成度见[项目状态](../development/status.md)。

完整示例见 [world.ts](../../examples/contents/scripts/world.ts) 与
[world.twee](../../examples/contents/story/world.twee)。启动 `examples/` 后，从大厅选择“世界地点与坐标”。

## 注册地点

在 `contents/scripts/*.ts` 的顶层调用 `World.add`：

```typescript
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
  id: "town",
  name: "小镇",
  bounds: [
    [-100, -100],
    [100, -100],
    [100, 100],
    [-100, 100],
  ],
  entry: [-10, 0],
})
```

`id` 是引用地点的英文标识，`name` 是可选显示名。`parent` 表示包含关系，可以引用稍后注册的
地点；所有脚本装载完毕后统一验证父引用、循环和范围包含关系，并关闭注册。运行中的 Macro、
按钮或事件回调不能再调用 `World.add`。

ID 区分大小写，必须以英文字母开头，其余字符允许字母、数字、`_`、`-`、`.`；环境 Tag 和
特殊 Passage 名称为保留字。更新游戏时可以修改显示名，已进入存档的 ID 应保持稳定。

### 坐标约定

所有地点使用同一世界坐标系，子地点坐标不相对父地点偏移。原点可以由作者任意选定，
X、Y 轴的正方向也由作者统一约定；坐标不等同于屏幕像素。负坐标与正坐标同样有效，
例如上例的小镇入口是 `[-10, 0]`。每个坐标必须是 `-(2^53 - 1)` 到 `2^53 - 1` 的安全整数。

`bounds` 是简单多边形的有序顶点，边界也在范围内；自交、退化范围和越界入口会被拒绝。
子地点的整个多边形必须位于父地点内。省略 `entry` 时使用首顶点。

## 用 Passage Tag 进入地点

```twee
:: WorldTown [town outside]
<<link [[进入医院|WorldHospital]]>><</link>>

:: WorldHospital [hospital inside]
医院正文。
<<link [[返回小镇|WorldTown]]>><</link>>
```

已注册的裸 `id` 直接作为地点 Tag。`inside` / `outside` 描述环境，不建立室内坐标系。
普通导航进入不同地点时使用该地点入口；在同一地点的 Passage 间导航会保留坐标。
未写环境 Tag 时，同地点保留原环境，切换地点则变为 `null`。

`Start` 不绑定地点，进入后 `World.current()` 为 `null`；它与其他特殊 Passage 都不能携带 Tag。
没有地点 Tag 的普通 Passage 保留当前位置，适合菜单和说明页；若它带有环境 Tag，则只更新
已有位置的环境。`include`、widget 和页眉等辅助内容的 Tag 不触发地点绑定。

未注册为地点 ID 的普通 Tag 保留原用途。一个 Passage 同时写入两个已注册地点 ID，或同时使用 `inside`
与 `outside`，会在开始运行前被拒绝。

地点切换由作者显式链接决定，当前没有“只能通往相邻地点”的约束。`parent` 表示包含关系，
不自动生成道路、入口按钮或导航权限。

## 查询与地点内移动

| API                    | 结果                                                         |
| ---------------------- | ------------------------------------------------------------ |
| `World.get(id)`        | 地点快照；不存在返回 `undefined`                             |
| `World.places()`       | 按 ID 排序的全部地点快照                                     |
| `World.locate([x, y])` | 包含坐标的全部地点，父先于子，同深度按 ID 排序；保留重叠地点 |
| `World.current()`      | `{ place, point, environment }`；尚未进入地点返回 `null`     |
| `World.move([x, y])`   | 移动后的位置快照；未进入地点或超出当前地点范围会抛错         |

读取结果是独立快照，修改它们不会写回世界。位置必须通过 `World.move` 或 Passage 导航修改，
移动失败时保持原位置。`World.move` 不会自动切换当前地点，即使坐标落入子地点。

在脚本中定义供 Twee 调用的移动函数：

```typescript
function worldMoveTo(x: number, y: number): void {
  World.move([x, y])
}
State.global.extend({ worldMoveTo })
```

```twee
:: WorldHospital [hospital inside]
<<button [[走到接待处|WorldHospital]]>>
  <<run worldMoveTo(35, 30)>>
<</button>>
```

按钮先移动，再导航到同一地点的 Passage 显示最新位置。示例中的 `worldCurrent` 脚本 Macro
使用 `Surface` 输出地点、坐标、环境和 `World.locate` 的包含关系，两个宿主均可显示。

已有 Reaction 生命周期条件也可以查询当前位置：

```typescript
Reaction.add({
  id: "world.hospital.welcome",
  lifecycle: true,
  cond: () => World.current()?.place === "hospital",
  widget: '<<print "欢迎来到医院。">>',
  once: true,
})
```

## 刷新、历史与存档

位置随故事事务提交或回滚。读档和语言切换刷新当前画面，保持恢复后的位置与环境；
刷新期间 `World.move` 仍校验参数，但不提交移动，只返回当前快照。若刷新途中发生新导航，
包括导航到同名 Passage，目标页恢复正常地点绑定与移动。历史后退/前进则恢复目标页进入前的
状态并重放正文。

正式存档 v3 保存当前与历史位置的稳定地点 ID、坐标和环境；地点定义由启动脚本重新注册。
读取旧 v2 存档时，两处位置均默认为未定位（`World.current()` 返回 `null`），不根据 Passage
推测位置。详细保存范围见 [Save](save.md)。

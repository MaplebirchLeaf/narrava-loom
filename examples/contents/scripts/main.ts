/** 小镇规则 / Town rules. 只有显式行动写入 State，页面和侧栏读取状态。 */
type TownState = {
  name: string
  minutes: number
  money: number
  energy: number
  stress: number
  shifts: number
  parcels: number
  coat: boolean
  quest: "available" | "accepted" | "complete"
  notice: string
  finds: number
}

function townState(): TownState {
  return V.town as TownState
}

/** 初始化新一局的角色与行动状态。 */
function townBegin(): void {
  V.demo_screen = "game"
  V.town = {
    name:
      String(V.town_name ?? "Alex")
        .trim()
        .slice(0, 24) || "Alex",
    minutes: 8 * 60,
    money: 20,
    energy: 100,
    stress: 0,
    shifts: 0,
    parcels: 0,
    coat: false,
    quest: "available",
    notice: "窗外传来街道的声音。今天从一件小事开始吧。",
    finds: 0,
  }
  // 新一局也重置声明式规则的触发次数；读档则由 Runtime 恢复这些次数。
  for (const id of ["town.delivery", "town.delivery.notice", "town.tired"]) Reaction.reset(id)
  Logger.info("demo.start", "小镇新一局")
}

function townClock(): string {
  const { minutes } = townState()
  return `第 ${Math.floor(minutes / 1440) + 1} 天 · ${String(Math.floor((minutes % 1440) / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`
}

function townOpen(): boolean {
  const hour = Math.floor((townState().minutes % 1440) / 60)
  return hour >= 8 && hour < 20
}

/** 把校验与扣款放在同一行动中，重复点击也不会产生负余额或重复奖励。 */
function townAct(action: string): void {
  const next: TownState = { ...townState() }
  let delivery = false
  switch (action) {
    case "accept":
      if (next.quest !== "available") return
      next.quest = "accepted"
      next.notice = "林澈 想修好公共休息室的旧收音机。去购物中心取一包零件吧。"
      break
    case "work":
      if (!townOpen() || next.energy < 25 || next.shifts >= 2) return
      next.minutes += 120
      next.energy -= 25
      next.stress = Math.min(100, next.stress + 12)
      next.money += 18
      next.shifts++
      next.notice = "你在钟楼咖啡馆收拾桌子、整理杯碟。两小时后，领到了 £18 工钱。"
      break
    case "coffee":
      if (!townOpen() || next.money < 4) return
      next.minutes += 20
      next.money -= 4
      next.energy = Math.min(100, next.energy + 12)
      next.stress = Math.max(0, next.stress - 10)
      next.notice = "你捧着拿铁坐在窗边，看行人从玻璃另一侧经过。"
      break
    case "parcel":
      if (!townOpen() || next.quest !== "accepted" || next.parcels > 0 || next.money < 8) return
      next.money -= 8
      next.parcels = 1
      next.notice = "店员把收音机零件装进纸袋。该回去找林澈了。"
      break
    case "coat":
      if (!townOpen() || next.coat || next.money < 12) return
      next.money -= 12
      next.coat = true
      next.notice = "一件新的雨衣。林间行走时会少消耗一些体力。"
      break
    case "deliver":
      if (next.quest !== "accepted" || next.parcels !== 1) return
      next.quest = "complete"
      next.parcels = 0
      next.money += 15
      next.stress = Math.max(0, next.stress - 15)
      next.notice = "收音机终于发出了清晰的声音。林澈 付给你 £15，还留了晚餐的位置。"
      delivery = true
      break
    case "explore": {
      const cost = next.coat ? 10 : 15
      if (next.energy < cost || !townOpen()) return
      next.minutes += 30
      next.energy -= cost
      // 抽样只发生在探索行动里；打开地图、弹窗和侧栏不消耗随机数。
      const roll = Math.random()
      if (roll < 0.45) {
        next.money += 6
        next.finds++
        next.notice = "你在路旁找到一枚旧纪念币。收藏摊愿意出 £6 收下它。"
      } else if (roll < 0.8) {
        next.stress = Math.max(0, next.stress - 8)
        next.notice = "一只松鼠从枝头跃过。你停下来听了一会儿风声。"
      } else {
        next.stress = Math.min(100, next.stress + 6)
        next.notice = "一阵雨打湿了小径。你绕过积水，花了些时间辨认路标。"
      }
      Logger.info("demo.explore", JSON.stringify({ roll }))
      break
    }
    case "clinic":
      if (next.money < 5) return
      next.minutes += 30
      next.money -= 5
      next.energy = Math.min(100, next.energy + 30)
      next.stress = Math.max(0, next.stress - 20)
      next.notice = "你在医院休息区补充了水和食物，精神恢复了不少。"
      break
    case "rest":
      next.minutes = (Math.floor(next.minutes / 1440) + 1) * 1440 + 8 * 60
      next.energy = 100
      next.stress = 0
      next.shifts = 0
      next.notice = "一夜过去，新的一天从窗帘间的晨光开始。"
      break
    default:
      throw new Error(`Unknown town action: ${action}`)
  }
  V.town = next
  Logger.info("demo.action", JSON.stringify({ action, minutes: next.minutes, money: next.money }))
  if (delivery) Event.emit("town:delivered", { recipient: "林澈" })
}

Macro.add("townStatus", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () => {
    if (!V.town || V.demo_screen === "menu") return Surface.text("Narrava · 小镇的一天")
    const state = townState()
    const english = I18n.locale === "en"
    return Surface.fragment(
      Surface.text(state.name, { heading: 2 }),
      Surface.text(townClock(), { color: 3 }),
      Surface.hardBreak(),
      Surface.text(`${english ? "Money" : "金钱"} £${state.money}`, { color: 24 }),
      Surface.hardBreak(),
      ...(
        [
          ["energy", english ? "Energy" : "体力", state.energy],
          ["stress", english ? "Stress" : "压力", state.stress],
        ] as const
      ).map(([key, label, value]) =>
        Surface.component(
          "meter",
          1,
          { label, value, min: 0, max: 100 },
          [`${label} ${value}/100`],
          { key: `town-${key}` },
        ),
      ),
      Surface.hardBreak(),
      Surface.text(state.coat ? "装备：雨衣" : "装备：日常便服", { color: 3 }),
    )
  },
})

Macro.add("townCompact", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () =>
    Surface.fragment(
      Surface.text("雨", { key: "town-weather", color: 40 }),
      Surface.text(V.town ? String(townState().energy) : "—", {
        key: "town-stowed-energy",
        color: 32,
      }),
      Surface.text("£", { key: "town-money", color: 24 }),
    ),
})

// 真正的异步宏 / A real suspended macro; the host resumes this transaction.
Macro.add("townWait", {
  body: "inline",
  arguments: "raw",
  execution: "async",
  handler: async () => {
    await Host.delay(100)
    return Surface.text("收音机里响起了晚间音乐。", { delay: 300, color: 40 })
  },
})

Reaction.add({
  id: "town.delivery",
  event: "town:delivered",
  widget: '<<crossFileCard "委托完成：旧收音机重新响起。">>',
  emit: { name: "town:notice", payload: { kind: "delivery" } },
  once: true,
})
Reaction.add({
  id: "town.delivery.notice",
  event: "town:notice",
  include: "DeliveryNotice",
  limit: 1,
})
Reaction.add({
  id: "town.tired",
  state: "$town.energy",
  cond: ({ before, after }) =>
    typeof before === "number" && typeof after === "number" && before >= 30 && after < 30,
  widget: "体力不多了，可以喝咖啡、去医院或回家休息。<br>",
  once: true,
})
Reaction.add({
  id: "town.forest.closed",
  lifecycle: true,
  passage: "ForestTrail",
  cond: () => !townOpen(),
  goto: "ForestClosed",
})

State.global.extend({ townBegin, townClock, townOpen, townAct })

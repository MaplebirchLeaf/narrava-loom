# Narrava · 小镇的一天 / A Day in Town

一个用于展示 Narrava Loom 能力的小镇生活示例。主线是一段约十分钟的日常旅程：
在旅舍醒来，接下林澈的委托，去商业街打工、购物，交付零件后回家休息，也可以先去森林探索。

包含经济、体力、压力、装备、营业时间和一个短任务。人物、地点及数值采用本示例设定。

## 运行

在仓库根目录执行：

```bash
cargo run --locked -p narrava-loom-core -- examples   # 编译检查
cargo run --locked -p narrava-loom-tauri -- examples  # 图文宿主
cargo run --locked -p narrava-loom-tui -- examples    # 文字宿主
```

开始菜单没有地点。选择“新的一天”，填写姓名、种子和今日打算，再开始游戏。
主菜单的“作者手册”提供独立的完整能力演示入口；无需先完成剧情。
TUI 用方向键和 Enter 选择，`b` / `f` 回溯和前进，`s` 切换展开与收拢侧栏。
两个 Host 都支持人物弹窗；图片在 TUI 中以替代文字呈现。

## 推荐游玩路线

1. 卧室 → 旅舍：接下 林澈的零件委托。
2. 住宅街 → 商业街 → 钟楼咖啡馆：帮工两小时，获得 £18。
3. 购物中心：花 £8 买零件；也可以花 £12 买降低探索消耗的雨衣。
4. 回旅舍交付：获得 £15，触发一次性的 Reaction 通知与后续事件。
5. 住宅街 → 森林入口 → 林间小径：保存进度，探索，再读档重复相同行动。
6. 回卧室睡到明早：恢复体力与压力，重置当天的两次工作额度。

每个地点都能打开人物、背包、日记和地图弹窗。独立地图页还显示当前坐标、室内外环境与父地点。
商店和森林在 08:00–20:00 开放，医院全天开放。只有标明耗时的活动推进时间，普通地点导航不耗时。
时间是示例自己的分钟计数，不代表 Core 已经提供历法或星象系统。种子控制探索序列，地图目前为手工注册。

## 源码分工

| 文件 | 职责 |
| --- | --- |
| `contents/story/main.twee` | 开始菜单、角色创建、日常流程、人物弹窗与设置 |
| `contents/scripts/main.ts` | 类型明确的游戏状态、统一行动结算、随机与任务规则、侧栏 |
| `contents/scripts/town-location.ts` | 小镇 → 街道 → 建筑，以及城外森林的统一坐标 |
| `contents/story/reference.twee`、`scripts/reference.ts` | 作者手册：边缘语法和渲染能力 |
| `contents/story/author-tools.twee`、`scripts/author-tools.ts` | Save、I18n、Logger、State、Reaction 验收 |
| `contents/story/location.twee`、`scripts/location.ts` | 最小地点 API 示例，与主线坐标范围分开 |
| `contents/story/widgets.twee` | 跨文件 Widget，主线公园也会实际调用 |
| `styles/main.css` | 只覆盖公开语义 token 的 Tauri 作者主题 |
| `resources/` | 仓库自制示意图、短提示音与文本资源 |
| `languages/en/` | 主菜单、设置页及部分手册消息的英文翻译示例 |

游戏数据统一存于 `$town` / `V.town`，表单设置使用 `$town_*`，手册使用独立的演示变量。
`townAct()` 集中检查余额、体力、营业时间与任务阶段；只有合法行动才扣款、发奖或消耗随机数。
界面正文读取结算后的状态。每个地点在公共菜单前明确记录自己的返回 Passage，便于作者审核导航。

## 能力索引

| 能力 | 游玩入口 / 验收入口 |
| --- | --- |
| textbox / checkbox / radiobutton，状态写回 | 新的一天；手册 `FormGallery` |
| State 的持久、临时、setup、global 命名空间 | 游戏 `$town`；手册 `StateGallery` |
| Region、展开与收拢 Bar、meter、Image、语义样式 | 游戏侧栏、人物弹窗；手册 `SurfaceGallery` |
| dialog / page，多页与单页 | 人物与日记；手册 `DialogGallery` |
| slot、plain/panel、row/stack、稳定 key/Region replace | 手册 `ReplaceGallery` |
| 未知 Component fallback、Resource 选择与文本读取 | 手册 `SurfaceGallery` |
| 64 色阶、8 种字形、heading、delay | 手册 `TextGallery`；交付后的延迟文字 |
| 真正的异步 Host.delay 与事务恢复 | `DeliveryDone` 的 `townWait` 宏 |
| if / switch / for / while / break / continue / unset / include | 日常分支；手册 `MacroGallery` |
| 跨文件纯逻辑 Widget 与调用参数、Macro 注册 | 公园；`widgets.twee` 与脚本宏 |
| Reaction Event 链、State 阈值、once/limit、lifecycle/goto | 交付、低体力、夜间森林；手册 `ReactionGallery` |
| 负坐标、嵌套范围、地点 ID tag、inside/outside | 地图与医院；手册 `LocationGallery` |
| 随机种子、历史与存档回放 | 森林探索；F10 `Random.current()` |
| Audio tag 范围、跨页连续与离开停止 | 森林入口 → 林间小径 → 住宅街 |
| Save、部分 I18n 翻译、统一日志、脚本 Console | 存档与设置；手册 `AuthorToolsGallery`；F10 |

## 存档、语言与调试

“存档与设置”里的按钮写入 `save/town-day.nsave`；主菜单可以读取它。
两个 Host 也会在导航后写 `autosave.nsave`。工作树中已有的存档没有被本次改造删除；
项目 ID 已改为 `example.town-day`，旧综合示例存档不应作为本 demo 的存档导入。

种子文本以固定的 FNV-1a（UTF-16 码元）算法映射为 32 位整数，然后交给 Runtime 的随机源。
只有探索行动调用 `Random.next()`；读档恢复完整随机状态，重复相同行动会得到相同结果。
打开地图、人物页或语言重绘不抽样。历史回溯同样恢复游戏状态。

英文包是局部翻译示例，未翻译的正文回落到中文。翻译消息由编译器生成的身份与原文对应，
不是对 DOM 查找替换。作者手册可以把完整 `I18n.export()` 模板写入日志。

开发配置启用了 Tauri `developer = true`：F10 打开单行脚本控制台，可以输入 `V.town`、
`V.town.money += 10`、`Location.current()` 或 `Random.current()`；Enter 执行。
`State`、`Save`、`Engine` 显示可展开成员和双语帮助；输入 `Save.` 浏览成员，Tab 补全，
`Save.export("debug")` 保存，`Engine.goto("TownMap")` 导航。无候选时上下键回顾命令。
F12 开关 WebView DevTools；TUI 的 F10 或 `:inspect` 保留只读检查。发行时关闭开发配置。
执行与刷新边界见[脚本控制台](../docs/author/random-and-debugging.md)。
`demo.action`、`demo.explore` 日志用于追踪行动与抽样，正文不承担调试控制台的职责。

## 宿主边界

剧情与状态通过 Semantic / Protocol 交给宿主，Core 不依赖 HTML。
地图使用地点列表与统一坐标，可供未来的 Godot 宿主复用。
本示例没有宣称已完成 Godot 渲染器、沙盒地形生成、楼层或星象系统。

Widget 范本使用 Core 纯逻辑节点（基本 print、条件、循环、include 与嵌套 Widget）；
需要宿主组件、样式参数或异步脚本时，放在 Passage 或脚本 Surface 宏中。

表单目前支持 checkbox、radiobutton、textbox。`replace` 使用稳定 key 或 Region，不接受 CSS selector。
Twee 的普通源码换行用于排版，游戏换行使用 `<br>`；多行结构闭合标签独占一行。
编辑器的非运行语法覆盖样例已移到 `editors/vscode-narrava-loom/tests/fixtures/`。

继续制作可参考[作者手册](../docs/author/guide.md)与[API 速查](../docs/reference/api-and-syntax.md)。

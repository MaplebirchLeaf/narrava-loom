# 随机数与脚本控制台

## Engine 根种子与随机结果

根种子交给 Engine，脚本 `Math.random()` 与 Twee `random()`、`either(...)` 共用同一序列。
在 `config.toml` 中指定可复现的开局：

```toml
[engine]
seed = 42
```

省略时，两个 Host 在装载作者脚本前生成根种子。配置接受非负 TOML 整数；
脚本可读取 `Engine.seed`，它是保留完整 u64 精度的十进制字符串，运行中不能重新设种。
相同种子、内容和调用顺序得到相同抽样；更换脚本或分支会改变后续调用顺序。

```ts
const rootSeed = Engine.seed
V.reward = Math.random() < 0.5 ? "coin" : "herb"
```

失败或取消恢复随机进度；历史回放从入页进度执行。读档恢复根种子及进度，
语言和当前页重绘使用临时序列，Header/Footer/Bar 的抽样也不消费正文进度。
`Engine.restart()` 恢复本次启动脚本完成后的状态与序列。
需要保留的行动结果仍应写入游戏变量，避免把行动逻辑放进每次都会执行的呈现正文。

旧 `Random.next()` 改为 `Math.random()`，`Random.seed()` 改为开局配置；不再提供 `Random` 全局。
存档格式与兼容边界见 [Save 格式](../architecture/save-format.md)。

## 游戏内脚本控制台

Tauri 在 `[host.tauri] developer = true` 时显示 `›_` 按钮。F10 开关底部控制台，
单行输入 JavaScript，Enter 执行，上下键浏览历史，Ctrl+L 清屏，Esc 关闭。
F12 仍用于 WebView DevTools；F10 输入在游戏的 Boa Runtime 中执行，可调用已加载的作者函数。

```js
V.town                         // 查看游戏数据 / Inspect game data
V.town.money += 10              // 修改余额 / Change balance
Location.current()             // 当前地点 / Current location
Location.places()              // 注册地点 / Registered places
townAct("rest")                // 调用示例的作者函数 / Call an author function
Save.export("debug")            // 保存到独立槽位 / Save to a separate slot
```

命令与游戏输入共用串行队列，成功修改会参与 Reaction 检查并同步输入控件。
普通 `print` 正文不会因任意变量赋值自动重绘；需要通过 Reaction 或下一次正常导航更新。
查询不会新增历史或自动存档。State、地点、Engine 随机进度与 Reaction 次数在失败时回滚；
日志和 JavaScript realm 中的声明、注册与外部副作用不属于 State 事务。

输入 `State`、`Save`、`Engine` 等对象名，结果会按属性树展示；展开成员可查看类型、
参数和中英双语说明，点击“使用 / Use”把属性路径放入输入行。`V`、`T`、`setup` 和作者
对象也使用相同的预览。内置 API 帮助从 TypeScript 声明生成，和编辑器共用一份说明。

输入 `Save.` 会列出成员，`Save.ex` 后按 Tab 补全；进入 `Save.export(` 后显示参数提示。
候选列表打开时上下键选择，Esc 收起；输入完整名称后 Enter 可直接执行。补全读取对象自身
属性，不执行输入表达式或 getter；作者全局函数可以调用，但不会现场推断 TypeScript 参数。

```js
Save.export("debug")            // 写入正式存档 / Export a save
Save.import("debug")            // 读档并更新画面 / Import and redraw
Engine.goto("TownMap")          // 正常导航 / Navigate
Engine.back()                   // 回到上一历史位置 / Back
Engine.restart()                // 恢复初始化状态并重新开始 / Restart
(await Host.delay(100), V.town)    // 等待后查看状态 / Wait and inspect
```

Promise 会在同一事务内结算；受管等待可用“停止 / Stop”取消并回滚 State。
一次只允许一个受管等待，`Host.delay()` 必须返回或 await，不能留下后台任务。
停止按钮不强行中断已开始的文件操作，也不回滚 JavaScript 声明、注册或日志。
每条命令限制 16 KiB、一百万次循环和 1024 个异步任务；无休止脚本会报错。

结果是执行时的快照，展开不会重新执行脚本。预览最多五层、400 个子节点、每个对象
80 个属性、每个字符串 4096 字符；截断后查询具体属性即可。循环、getter 与非 State
代理显示标记，不自动执行访问器或代理 trap。日志预览最多 2048 字符，前端保留最近
500 行和 100 条命令历史；清屏不删除 Runtime 日志。控制台不提供 TypeScript 现场转译、
断点或浏览器 DOM API；已加载的作者 TypeScript 函数可以直接调用。

TUI 仍使用 F10 或 `:inspect` 查看只读 State / Location / 日志快照。
查询不执行作者表达式、不消费日志订阅；Pending 期间拒绝读取半完成状态。
Tauri 关闭 developer 后，Rust 同时拒绝快照和脚本执行，不只是隐藏入口。

## 统一日志与错误来源

```ts
const subscription = Logger.subscribe({ minimumLevel: "warn", target: "quest" })
Logger.warn("quest", "任务条件尚未满足")
const records = Logger.take(subscription) // 取走该订阅的待处理记录
Logger.unsubscribe(subscription)
```

日志历史和订阅队列都有容量上限，默认各保留最近 1024 条；日志不随剧情失败回滚。
错误优先显示来源文件、行列和稳定错误码。TypeScript 解析错误使用原始位置；运行时能取得的
转译后位置明确标为「Generated」，不会冒充原始 TypeScript 行号。暂未提供 TS source map、
断点或源码自动跳转。

# 调试与故障排查

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

## 日志

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

## 排查顺序

先运行 `cargo run --locked -p narrava-loom-core -- my-game`，修正配置和编译错误；
再启动 Host，复现具体操作并查看错误码、来源和消息。运行失败会回滚相应故事事务，
应修复触发失败的内容，而不是忽略错误继续操作。

| 现象 | 检查 |
| --- | --- |
| 缺少 WebKit/GTK 或 ALSA | 按[快速入门](quick-start.md#准备环境)安装系统依赖 |
| 找不到配置或拒绝游戏路径 | 从仓库根目录传入 `my-game`，确认 `config.toml` 存在 |
| 找不到 Start 或导航目标 | Passage 名区分大小写且全局唯一 |
| `$name` 原样显示 | 用 `<<print $name>>` 显式求值 |
| 结构 Macro 被当成文字 | 条件、循环和分支标签必须顶格且独占一行 |
| Twee 找不到脚本函数 | 用 `State.global.set/extend` 显式导出 |
| 脚本找不到 window 或 document | 游戏脚本不在 WebView 中执行 |
| 图片或 CSS 资源缺失 | 对照[资源路径表](resources.md)，不要混淆 `img/` 与资源根目录 |
| Promise 无法结算 | 返回或 await `Host.delay`；不支持未受管的后台任务 |
| 存档失败 | 在 `Save.after` 读取结果，检查 target、目录权限和游戏版本 |
| 译文未显示 | 检查 locale、游戏兼容范围、文本 ID 与 placeholder |
| 变量改了但正文没变 | 赋值只同步状态和输入；用 Reaction 或导航更新正文 |

更改后重新验证触发错误的操作；只读查询和编译通过都不能代替交互验收。

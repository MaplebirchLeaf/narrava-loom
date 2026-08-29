# Narrava Loom 作者 API 与语法速查

本页只列当前源码已经实现并接受测试的作者接口。详细解释和教程仍看
[/docs/author/guide.md](/docs/author/guide.md)。

> `.twee` 中的 Expression 由 Core 求值，不是 JavaScript；`contents/**/*.ts` 和 `*.js` 才在
> Rust Worker 的 ECMAScript 环境运行。两者的函数和操作不能混用。

## 1. Twee 内置 Macro

Macro 名区分大小写。结构 Macro 的子句不能脱离所属容器单独使用。

| Macro | 形式 | 作用 |
|---|---|---|
| `if` | `<<if condition>>...<</if>>` | 条件分支容器 |
| `elseif` | `<<elseif condition>>` | `if` 内的附加条件分支 |
| `else` | `<<else>>` | `if` 内的最终分支 |
| `switch` | `<<switch value>>...<</switch>>` | 严格相等的多分支容器 |
| `case` | `<<case value>>` | `switch` 的匹配分支 |
| `default` | `<<default>>` | `switch` 的最终分支 |
| `for` | `<<for $x of collection>>...<</for>>` | 遍历集合值 |
| `for` | `<<for $key in collection>>...<</for>>` | 遍历集合键或索引 |
| `for` | `<<for $x range start to end step step>>...<</for>>` | 数字范围循环，`step` 可省略 |
| `while` | `<<while condition>>...<</while>>` | 条件循环 |
| `break` | `<<break>>` | 结束当前循环 |
| `continue` | `<<continue>>` | 进入当前循环下一轮 |
| `set` | `<<set $name = value>>` | 赋值；也接受 `to` 写法 |
| `unset` | `<<unset $name>>` | 删除可写目标 |
| `run` | `<<run expression>>` | 求值并丢弃结果，保留副作用 |
| `print` | `<<print expression [color] [style...]>>` 或 `<<print expression {color, styles, delay, heading}>>` | 求值并入 Passage 输出；带选项时产生带语义样式、color、delay 与结构性标题的 StyledText |
| `include` | `<<include "Passage">>` | 在当前位置执行另一 Passage，不发生导航 |
| `goto` | `<<goto "Passage">>` | 请求导航并停止当前 Passage |
| `link` | `<<link [[文本\|Passage]]>>...<</link>>` | 建立玩家可点击的导航动作；正文激活后执行 |
| `button` | `<<button [[文本\|Passage]]>>...<</button>>` | 与 link 共享事务语义，但由 Host 呈现为按钮 |
| `replace` | `<<replace "header">>...<</replace>>` | 用正文替换 `header/main/footer/bar/bar-stowed/dialog` 固定区域或稳定 Surface key |
| `slot` | `<<slot "status">>...<</slot>>` | 建立可由 `replace "status"` 定位的稳定内容槽 |
| `silently` | `<<silently>>...<</silently>>` | 执行正文但抑制其直接输出 |
| `exit` | `<<exit>>` | 停止当前执行正文 |
| `return` | `<<return expression>>` | 从可返回的 Macro/Widget 正文返回，可省略值 |
| `capture` | `<<capture @name @index>>...<</capture>>` | 把列出的局部变量捕获进延迟正文 |
| `widget` | `<<widget "name">>...<</widget>>` | 在带 `[widget]` Tag 的 Passage 中定义 Inline Macro；调用写作 `<<name ...>>`，不带调用侧闭合标签 |
| `checkbox` | `<<checkbox "$name" unchecked checked>>` | 勾选时写入 `checked`，取消时写入 `unchecked` |
| `radiobutton` | `<<radiobutton "$name" value>>` | 选中时把 `value` 写入同一 receiver |
| `textbox` | `<<textbox "$name" default>>` | 编辑完成时写入文字；receiver 未定义时先写默认值 |

其他名称会作为自定义 Macro 解析。它必须已经由 `widget` 或脚本 `Macro.add()` 注册。

> 状态绑定输入当前支持 `$` 与 `_` receiver。`@` 属于已经结束的 Macro 调用帧；在持久 Widget
> 实例状态完成前，点击后的 Host 输入不会伪造对 `@` 的写回。

`replace` 不接受 CSS selector、HTML 字符串、DOM 节点或终端坐标。固定区域名由每个 Host 映射；
普通 key 必须先由 `slot` 或 Script Surface 建立。`slot` 放进 `silently` 后输出会被丢弃，
因此不会留下可替换目标。
其他名称按稳定 Surface key 解析。当前正文支持静态文本与 Core 逻辑节点；嵌套动态 `print`
的 I18n 身份和动态／异步脚本 Macro 尚未接通，遇到时会报错而不是忽略。

> **书写约束**：结构容器（`if`/  `switch`/  `for`/  `while` 及其子句 `elseif`/  `else`/  `case`/  `default` 与
> 各闭合标签）必须独占一行且从行首开始（顶格、无缩进）；缩进的容器行不会被识别为结构，可能被当作
> 内联调用求值或直接泄漏为文本。`link`/  `button`/  `replace`/  `slot`/  `silently`/  `capture` 既可顶格
> 跨行书写，也可单行内联（`<<silently>><<set $x to 1>><</silently>>`）。`set`/  `unset`/  `print`/  `run`/
> `include`/  `goto`/  `break`/  `continue`/  `exit`/  `return` 等内联 Macro 可出现在行内任意位置。

## 2. Twee Expression 内置函数

| 函数 | 参数 | 结果 |
|---|---:|---|
| `abs(value)` | 1 | 绝对值 |
| `boolean(value)` | 1 | 转换为布尔值 |
| `ceil(value)` | 1 | 向上取整 |
| `clamp(value, min, max)` | 3 | 限制数值范围 |
| `clone(value)` | 1 | 深拷贝值图，断开原 Array/Object 引用并保留拷贝内部共享和循环 |
| `defined(value)` | 1 | 是否不是 `undefined` |
| `empty(value)` | 1 | 字符串、Array 或 Object 是否为空 |
| `entries(value)` | 1 | Array/Object 的键值对 |
| `either(...values)` | 至少 1 | 随机返回一个参数 |
| `floor(value)` | 1 | 向下取整 |
| `keys(value)` | 1 | Array/Object 的键 |
| `max(...values)` | 至少 1 | 最大数值 |
| `min(...values)` | 至少 1 | 最小数值 |
| `number(value)` | 1 | 转换为数值 |
| `random()` | 0 | `[0, 1)` 随机数 |
| `round(value)` | 1 | Web 语义四舍五入 |
| `string(value)` | 1 | 转换为字符串 |
| `values(value)` | 1 | Array/Object 的值 |
| `Object.assign(target, ...sources)` | 至少 1 | 按顺序写入 Object |
| `Object.hasOwn(target, key)` | 2 | 是否有自身属性 |

## 3. Array、String 属性与方法

- Array 属性：`length`
- Array 只读方法：`at(index)`、`concat(...values)`、`includes(value)`、
  `indexOf(value, fromIndex?)`、`join(separator?)`、`slice(start?, end?)`
- Array 可写方法：`pop()`、`push(...values)`、`shift()`、`splice(...)`、
  `unshift(...values)`
- String 属性：`length`，按 UTF-16 码元计数
- String 方法：`includes(text)`、`slice(start?, end?)`、`split(separator?, limit?)`、
  `startsWith(text)`、`endsWith(text)`、`trim()`、`toLowerCase()`、`toUpperCase()`

可写 Array 方法只有在当前 Expression 上下文允许写入时才能执行。

## 4. Expression 值、变量与操作

- 常量：`true`、`false`、`null`、`undefined`
- 数据：Number、String、Array、Object
- 变量：`$name` 持久游戏变量、`_name` 临时变量、`@name` Macro 局部变量
- 读取：成员 `object.name`、索引 `value[index]`、调用 `function(...)`
- 可选链：`?.`、可选索引和可选调用
- 算术：`+ - * / // % **`
- 位运算：`& | ^ << >> >>>`
- 比较：`< <= > >= == != === !== <=>`；别名为 `lt`、`lte`、`gt`、`gte`、`equ`、
  `is`、`isnot`
- 逻辑与空值：`&& || ??`，以及 `and`、`or`、`not`
- 成员判断：`in`、`notin`、`instanceof`、`between`
- 条件：`condition ? yes : no`
- 赋值：`=` 以及算术、位、逻辑、空值复合赋值；支持前置/后置 `++`、`--`

## 5. Worker ECMAScript 全局 API

游戏脚本直接使用下列大写全局；完整类型签名位于
[`bindings/typescript/narrava.d.ts`](../../bindings/typescript/narrava.d.ts)。Worker 没有
`window`、`document` 或 Tauri API，也不存在统一的 `narrava` 聚合对象。

### `State`

- `State.global/variables/temporary.get(name)`、`has(name)`、`set(name, value)`、`del(name)`
- `State.global/variables/temporary.extend(values)`
- `State.setup.get()`、`State.setup.set(value)`

### `Macro`

- 定义：`add`、`update`、`del`、`get`、`has`
- 生命周期：`before(name, hook)`、`after(name, hook)`、`off(subscription)`
- 定义项声明 `body`、`arguments`、`execution` 和 `handler`

### `Engine` 与 `Story`

- `Engine.started`
- `Engine.goto(target)`、`back()`、`forward()`、`restart()`
- `Story.has(name)`、`current()`、`get(name)`、`visits(name)`

### `Logger`

- 写日志：`trace`、`debug`、`info`、`warn`、`error`
- 读取订阅：`subscribe(filter?)`、`take(subscription)`、`unsubscribe(subscription)`

### `Event`

- 作者事件：`emit(name, payload?)`
- 订阅：`subscribe(filter?)`、`take(subscription)`、`unsubscribe(subscription)`
- Engine 保留事件：`passage:init`、`passage:start`、`passage:render`、
  `passage:display`、`passage:end`

### `Host`

- `await Host.delay(milliseconds)`：暂停当前异步 Macro，时间到后恢复同一 Engine 事务；
- 毫秒数必须在 `0..=86400000`，一次 Macro 同时只能等待一个 Host 操作；
- 普通 `setTimeout`、`fetch`、DOM 和 Tauri API 不存在于游戏 Worker。

### `Save`

- 内存：`capture()`、`restore(json)`
- Host 槽位：`export(target?)`、`import(target?)`
- 生命周期：`before(operation, hook)`、`after(operation, hook)`、`off(subscription)`
- operation：`capture`、`restore`、`export`、`import`

### `Resource` 与 `I18n`

- `Resource.paths()`、`has(path)`、`pick(candidates)`、`info(path)`、`read(path)`、
  `text(path)`
- `I18n.defaultLocale`、`I18n.locale`、`I18n.export()`

### `Surface`

- `text(text, { key?, styles?, color?, delay?, heading? })`
- `image(resource, { key?, alt?, caption? })`
- `region(region, children, { key? })`
- `component(capability, version, properties, fallback, { key? })`
- `action(label, "dismiss", { key?, role? })`
- `fragment(...children)`

Twee 的普通正文已经直接编译为 `Surface Text`，动态普通文本使用 `<<print expression>>`，
因此写故事正文不需要调用 `Surface.text()`：

```twee
:: Start
这是普通 Surface Text。
<<print $hero>>
```

`Surface.text()` 这个 builder 本身属于 Worker ECMAScript，不能放进 Twee Expression。
需要在正文中穿插带语义样式或 color 的短文字，直接使用 Core `print` Macro：

```twee
你获得了 <<print "关键道具" 30 "strong">>。
<<print $status 40 "emphasis">>
```

参数依次是内容、可选 color、零到多个 style；内容可以是变量或其他 Twee Expression。它是
Inline Macro，不使用闭合标签。也支持对象形式同时指定 color、styles、delay 与结构性标题：

```twee
<<print $status {color: 40, styles: ["strong", "code"]}>>
<<print "两秒后出现" {color: 20, delay: 2000}>>
<<print "第一页" {heading: 2}>>
```

### `print` 的 color：0..=63 标准调色板

color 是 0..=63 的色阶，**颜色由 Host 映射**（对齐二进制边界：灰阶 0-7（白`1`→亮灰`2`→浅灰`3`→灰`4`→深灰`5`→暗灰`6`→黑`7`），光谱 8-63（红`8`→橙`16`→黄`24`→绿`32`→蓝`40`→紫`48`→深紫`56`→`63`，每色相 8 级）），0 为正文默认（不染色）。必须是 0..=63 的整数，否则报 `macro.print.invalid_arguments`。
Tauri 默认 Renderer 会计算并验证 0..63 的全部映射，游戏作者无需为色阶编写 64 条 CSS；
只有希望覆盖品牌色时才使用 `[data-color="N"]`。

### `print` 的 style：8 个语义字形

| style | 含义 | 渲染提示 |
|---|---|---|
| `emphasis` | 语气强调 | 斜体 |
| `strong` | 重要 | 加粗 |
| `code` | 代码/标识符/键位 | 等宽 |
| `quote` | 引文/信件/留言 | 引用块 |
| `marked` | 需要玩家注意 | 半透明高亮底色；与 color 文字颜色相互独立 |
| `small` | 注释/脚注/次要信息 | 小字 |
| `inserted` | 新增内容 | 下划线/加号（TUI `++…++`） |
| `deleted` | 删除/废弃内容 | 删除线（TUI `~~…~~`） |

Tauri 默认主题已经实现全部 8 种字形及 heading 排版；作者 CSS 只负责可选的品牌覆盖。

### `print` 的 delay：可见延迟

delay 是毫秒，`0..=86400000` 的整数。Host 在此之前不呈现内容；动画方式不属于协议；
TUI 把延迟文本停放在 `frame.delayed`，由消费方按 `render_at` 到点显示，终端无平滑
过渡但时序一致。

### `print` 的 heading：结构性标题

heading 是 `1` 或 `2` 的**结构性标题级别**，不属于字形样式：它表达文档层级（例如
弹窗 Dialog 的页面标题），Host 据此划分页面并生成页签或标题元素。Tauri WebView 把
heading 1/2 渲染为 `h1`/`h2`，弹窗按顶层标题把后续内容归入对应页面并生成页签；
TUI 加粗下划线提示。不带 heading 的文本不受影响。

需要 Region、Component 或 fallback 等结构化表现时，再在 `.ts/.js` 中定义语义 Macro：

```ts
Macro.add("statusCard", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () => Surface.text("状态正常", { styles: ["strong"], color: 30 }),
})
```

这些 builder 不接受 HTML、CSS class 或 DOM 对象。文本 style、color、Region、Component 和
fallback 的完整枚举见 `bindings/typescript/narrava.d.ts`。

## 6. VS Code `.twee` 高亮

仓库扩展位于 [`editors/vscode-narrava-loom`](../../editors/vscode-narrava-loom)。它高亮 Passage、
Tag、Macro、Expression 函数、变量、注释、链接和插值，并提供括号与引号自动闭合。安装方法见
扩展目录中的 README。

语言服务会从内置表、跨文件 Widget 和脚本 `Macro.add/update()` 的 `body` 字段区分 Inline 与
Container：Inline 出现闭合标签、Container 缺少或错配闭合标签都会产生诊断。

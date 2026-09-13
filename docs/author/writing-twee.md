# 编写 Twee

Macro 参数用空白分隔，不使用顶层逗号。`if`、`switch`、`for`、`while` 及其分支、
闭合标签必须顶格并独占一行；完整写法见[Macro 参考](../reference/macros.md)。

## Passage、标签和文件拆分

一个文件可以写多个 Passage：

```twee
:: Entrance [opening daytime]
你站在门外。

:: Hall [indoor hub]
这里是大厅。
```

- Passage 名称区分大小写；
- 名称在整个游戏中必须唯一，即使位于不同 `.twee` 文件；
- 方括号内是以空白分隔的标签；
- 源文件换行和空行只用于整理源码，不会直接控制游戏内排版；
- `/% 注释 %/` 不会显示给玩家；
- `StoryInit` 是初始化 Passage，不能当普通导航目标；
- `Start`、`StoryInit`、`Header`、`Footer`、`Bar`、`BarStowed` 是特殊 Passage，均不能带 Tag；
- 带 `exit` 标签的 Passage 用于退出型执行，不进入普通导航历史。

推荐按章节拆文件，但不要依赖文件名决定执行顺序。故事顺序应由导航和明确生命周期决定。

## 添加可点击选择

```twee
:: Start
你站在一扇门前。
<<link [[推门进入|Hall]]>><</link>>

:: Hall
这里是大厅。
<<link [[回到门外|Start]]>><</link>>
```

`[[推门进入|Hall]]` 中：

- `推门进入` 是玩家看到的文字；
- `Hall` 是目标 Passage；
- 两边区分明确，不要把顺序写反；
- 目标必须存在且大小写一致。

游戏内需要换行时显式写 `<br>`：

```twee
你站在一扇门前。<br>
<<link [[推门进入|Hall]]>><</link>><br>
<<link [[转身离开|Outside]]>><</link>>
```

普通源码换行会折叠为空格，`<br>` 才是游戏内硬换行。文字样式使用
`<<print value color style...>>` 或脚本侧 `Surface.text()`；其他标签会按普通文字显示。

## 变量：什么时候用 `$`、`_`、`@`、`setup`

| 写法 | 生命周期 | 是否进存档 | 典型用途 |
|---|---|---:|---|
| `$name` | 一局游戏 | 是 | 角色名、金币、剧情选择 |
| `_name` | 临时执行 | 否 | 中间计算、当前 Passage 临时值 |
| `@name` | 当前 Macro/Widget 调用 | 否 | 局部变量 |
| `setup.name` | 启动配置 | 否 | 难度表、固定规则 |

赋值与显示：

```twee
:: StoryInit
<<set setup.chapter to 1>>

:: Start
<<set $hero to "Author">>
<<set $coins to 3>>
<<set _cost to 2>>
你好，<<print $hero>>。你有 <<print $coins>> 枚金币。
```

正文中直接写 `$hero` 不会自动替换：

```twee
你好，$hero。
```

上面会原样显示 `$hero`。必须使用 `<<print $hero>>`。

删除变量：

```twee
<<unset $hero>>
```

## 条件、计算与布尔值

```twee
<<if $coins >= 2>>
你买下了地图。
<<set $coins to $coins - 2>>
<<else>>
你的钱不够。
<</if>>
```

多分支：

```twee
<<if $reputation >= 10>>
守卫向你敬礼。
<<elseif $reputation >= 0>>
守卫让你通过。
<<else>>
守卫拦住了你。
<</if>>
```

常用值：字符串写成 `"文字"`，布尔值为 `true`/`false`，空值为 `null`，未定义值为
`undefined`。不要用正文猜测表达式语法，完整运算符以
[Expression 参考](../reference/expressions.md)为准。

## 循环、包含和直接跳转

包含另一个 Passage 的内容但不进行普通导航：

```twee
<<include "SharedDescription">>
```

直接请求导航：

```twee
<<goto "Hall">>
```

`goto` 会停止当前 Passage 的后续执行。不要在它后面依赖仍会运行的赋值。

循环示意：

```twee
<<set $count to 0>>
<<while $count < 3>>
第 <<print $count>> 次。
<<set $count to $count + 1>>
<</while>>
```

可用控制包括 `for`、`while`、`break`、`continue`、`switch`、`include`、`goto`、`run`、
`silently`、`exit`、Widget 和 `capture`。精确参数与作用域见
[Macro 参考](../reference/macros.md)。

## 隐藏输出

```twee
<<silently>>
<<set $found_book to true>>
这句话不会显示。
<</silently>>
结果：<<print $found_book>>
```

`silently` 只丢弃显示输出，不撤销变量修改。它不是事务回滚，也不是注释。

## 可替换内容槽与面板

`slot` 在正文当前位置建立稳定内容槽；随后出现的 `replace` 可以按 key 替换它：

```twee
<<slot "quest-status">>尚未接受任务。<</slot>>
<<replace "quest-status">>任务已经开始。<</replace>>
```

默认 `plain` 槽不增加视觉边界。需要让一组正文与相邻内容形成独立面板时，传入
`"panel"`：

```twee
<<slot "quest-panel" "panel">>
任务：寻找旧地图。<br>
线索：旧船长最后一次在码头见过地图。
<</slot>>
```

面板默认使用 `stack` 上下排列。需要多个相邻面板同行时，对每个面板显式写第三参数 `"row"`：

```twee
<<slot "quest" "panel" "row">>任务：寻找旧地图。<</slot>>
<<slot "place" "panel" "row">>地点：码头。<</slot>>
```

第三参数只接受 `stack`／`row`；不提供尺寸、间距或任意布局参数。

`panel` 表示独立内容组；边框、间距和窄屏换行由 Host 决定。
当前 Twee `slot` / `replace` 正文只支持静态文本和 Core 逻辑节点，动态 `print`、输入、
按钮或脚本 Macro 会报错。需要动态替换时参考[事件与 Reaction](events.md)。

## 点击正文与复用

`link` 正文在点击时执行，显示选项时不执行。带目标的 link 执行正文后导航；
字符串形式 `<<link "查看角色">>` 只执行动作，适合[打开弹窗](dialog.md)。

```twee
<<link [[进入森林|Forest]]>>
<<set $visitedForest to true>>
<</link>>
```

可复用的小片段放在带 `[widget]` 标签的 Passage 中：

```twee
:: Widgets [widget]
<<widget "greeting">>
你好，<<print @args[0]>>。
<</widget>>

:: Start
<<greeting "旅人">>
```

Widget 调用不写闭合标签，实参通过 `@args` 读取，局部变量属于本次调用。
需要复杂计算时再定义[脚本 Macro](scripting.md#自定义-macro)。

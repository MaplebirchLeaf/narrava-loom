# Twee、选择、变量、条件和循环

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

当前 Tauri Renderer 会按 Core 输出顺序把导航显示成行内选择。

游戏内需要换行时显式写 `<br>`：

```twee
你站在一扇门前。<br>
<<link [[推门进入|Hall]]>><</link>><br>
<<link [[转身离开|Outside]]>><</link>>
```

普通源码换行会折叠为空格，`<br>` 才是游戏内硬换行。文字样式使用
`<<print value tone style...>>` 或脚本侧 `Presentation.text()`；其他标签会按普通文字显示。

## 变量：什么时候用 `$`、`_`、`@`、`setup`

| 写法 | 生命周期 | 是否进存档 | 典型用途 |
|---|---|---:|---|
| `$name` | 一局游戏 | 是 | 角色名、金币、剧情选择 |
| `_name` | 临时执行 | 否 | 中间计算、当前 Passage 临时值 |
| `@name` | 当前 Macro/Widget 调用 | 否 | 局部变量 |
| `setup.name` | 启动配置 | 通常作为初始化数据 | 难度表、固定规则 |

赋值与显示：

```twee
:: StoryInit
<<set setup.chapter to 1>>

:: Start
<<set $hero to "Maple">>
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
[/docs/architecture/expression.md](/docs/architecture/expression.md)为准。

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
[/docs/architecture/macro.md](/docs/architecture/macro.md)。

## `silently` 到底做什么

```twee
<<silently>>
<<set $found_book to true>>
这句话不会显示。
<</silently>>
结果：<<print $found_book>>
```

`silently` 只丢弃显示输出，不撤销变量修改。它不是事务回滚，也不是注释。

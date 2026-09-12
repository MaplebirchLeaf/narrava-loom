# Macro：使用与组合

Macro 写在 Twee 正文中，用于显示内容、改变状态或建立交互。精确参数见
[API 与语法速查](../reference/api-and-syntax.md)；执行器内部实现见
[Macro 执行与所有权](../architecture/macro-runtime.md)。

## 显示文字与图片

```twee
<<print $hero.name>>
<<image "tree.png" "树">>
```

图片文件放在 `resources/img/tree.png`；参数使用 Resource 逻辑路径。
Tauri 显示图片，TUI 显示包含“树”的方框。alt 可省略，省略时 TUI 显示空方框。

## 单次调用与正文容器

`print`、`image`、`set` 这类调用不需要闭合标签。`if`、`for`、`link`、`slot` 等容器
包含正文，需要对应闭合。控制流容器及其分支应独占一行，从行首开始。

```twee
<<if $score > 0>>
你已经获得积分。
<<else>>
继续探索吧。
<</if>>
```

参数用空白分隔，不使用顶层逗号。字符串写在引号中，变量直接使用 `$name`。

## 点击后执行

```twee
<<link [[进入森林|Forest]]>>
<<set $visitedForest = true>>
<</link>>
```

link 的正文在点击时执行。显示内容本身不会提前执行这段正文。
当前 link 使用带目标 Passage 的交互参数；无跳转的弹窗按钮语法仍待定。

## 重用内容

简单复用可以使用带 `[widget]` 标签的 Passage：

```twee
:: Widgets [widget]
<<widget "greeting">>
你好，<<print @args[0]>>。
<</widget>>

:: Start
<<greeting "旅人">>
```

需要脚本逻辑时再使用 [TypeScript/JavaScript Macro](scripting-and-resources.md)。
Widget、脚本 Macro 与原生 Macro 共用现有输出语义，不需要生成 HTML。

## 内容区域与弹窗

`slot` 建立可替换的内容槽，`replace` 更新槽或标准区域。显示样式由 Host 决定。
`dialog/page` 显式定义单页或多页弹窗，`dialog` 的参数选择默认页标题。
无导航 `<<link "查看角色">>` 点击后执行正文；完整语法见[弹窗与页面](dialog.md)。

背景音使用 `<<audio "forest.ogg" "ambience" "forest">>`，参见 [Audio 生命周期与标签](audio.md)。
图片路径相对于 `resources/img/`，音频路径相对于 `resources/audio/`。

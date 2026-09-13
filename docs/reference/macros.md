# Twee Macro 参考

Macro 名区分大小写。结构 Macro 的子句不能脱离所属容器单独使用。

| Macro | 形式 | 作用 |
| ------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
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
| `meter` | `<<meter "体力" $stamina 0 100>>` | label、value、min、max；有限数值且 min < max；复用 meter@1 component，TUI 字符条、Tauri 图形条 |
| `audio` | `<<audio resource channel? tag?>>` | 声明当前 Passage 所需背景音；默认 bgm，按 tag 匹配 |
| `dialog` | `<<dialog "默认页">>...<</dialog>>` | 打开单个弹窗，默认页标题必须存在 |
| `page` | `<<page "页标题">>正文` | dialog 的直接子句；页标题非空且唯一，不写闭合标签 |
| `image` | `<<image "tree.png" "树">>` | 用 Resource 逻辑路径输出图片；alt 为可选字符串，路径支持 Expression；Tauri 显示图片，TUI 将 alt 显示在方框中 |
| `include` | `<<include "Passage">>` | 在当前位置执行另一 Passage，不发生导航 |
| `goto` | `<<goto "Passage">>` | 请求导航并停止当前 Passage |
| `link` | `<<link [[文本\|Passage]]>>...<</link>>` 或 `<<link "文本">>...<</link>>` | 点击后执行正文；字符串形式不导航 |
| `button` | `<<button [[文本\|Passage]]>>...<</button>>` | 与 link 共享事务语义，但由 Host 呈现为按钮 |
| `replace` | `<<replace "header">>...<</replace>>` | 用正文替换 `header/main/footer/bar/bar-stowed/dialog` 固定区域或稳定 Surface key |
| `slot` | `<<slot "status">>...<</slot>>` 或 `<<slot "status" "panel" "row">>...<</slot>>` | 建立稳定内容槽；`panel` 请求面板，`stack`／`row` 选择上下或同行排列 |
| `silently` | `<<silently>>...<</silently>>` | 执行正文但抑制其直接输出 |
| `exit` | `<<exit>>` | 停止当前执行正文 |
| `capture` | `<<capture @name @index>>...<</capture>>` | 把列出的局部变量捕获进延迟正文 |
| `widget` | `<<widget "name">>...<</widget>>` | 在带 `[widget]` Tag 的 Passage 中定义 Inline Macro；调用写作 `<<name ...>>`，不带调用侧闭合标签 |
| `checkbox` | `<<checkbox "$name" unchecked checked>>` | 勾选时写入 `checked`，取消时写入 `unchecked` |
| `radiobutton` | `<<radiobutton "$name" value>>` | 选中时把 `value` 写入同一 receiver |
| `textbox` | `<<textbox "$name" default>>` | 编辑完成时写入文字；receiver 未定义时先写默认值 |

其他名称会作为自定义 Macro 解析。它必须已经由 `widget` 或脚本 `Macro.add()` 注册。

> 状态绑定输入当前支持 `$` 与 `_` receiver。`@` 属于已经结束的 Macro 调用帧；在持久 Widget
> 实例状态完成前，点击后的 Host 输入不会伪造对 `@` 的写回。

输入 receiver 必须是无副作用的变量、成员或索引路径，例如 `$name`、`$hero.name`、
`$items[$index + 1]`；索引中的函数调用、赋值和自增会在控件创建时拒绝。

`replace` 不接受 CSS selector、HTML 字符串、DOM 节点或终端坐标。固定区域名由每个 Host 映射；
普通 key 必须先由 `slot` 或 Script Surface 建立。`slot` 放进 `silently` 后输出会被丢弃，
因此不会留下可替换目标。

`slot` 的表现默认为 `plain`，保持视觉透明；可选 `panel` 表达“与相邻正文形成感知边界的
独立内容组”。Protocol 不规定方框字符、HTML 标签、边框、圆角、阴影、尺寸或间距：TUI
可以画矩形框，Tauri 可以使用 card/panel，其他 Host 可使用自己的原生容器。未知表现值会
在 Runtime 边界报错，不会作为 Host 样式字符串透传。
排列默认为 `stack`。第三参数只接受 `stack` 或 `row`，且仅描述同级容器上下堆叠或连续同行，
只适用于 `panel`；不携带宽度、gap、对齐、换行点或任何 CSS/layout 参数。
其他名称按稳定 Surface key 解析。当前正文支持静态文本与 Core 逻辑节点；嵌套动态 `print`
的 I18n 身份和动态／异步脚本 Macro 尚未接通，遇到时会报错而不是忽略。

**书写约束**：结构容器（`if`/ `switch`/ `for`/ `while` 及其子句 `elseif`/ `else`/ `case`/ `default` 与
各闭合标签）必须独占一行且从行首开始（顶格、无缩进）；缩进的容器行不会被识别为结构，可能被当作
内联调用求值或直接泄漏为文本。`link`/ `button`/ `replace`/ `slot`/ `silently`/ `capture` 既可顶格
跨行书写，也可单行内联（`<<silently>><<set $x to 1>><</silently>>`）。`set`/ `unset`/ `print`/ `run`/
`include`/ `goto`/ `break`/ `continue`/ `exit`/ `return` 等内联 Macro 可出现在行内任意位置。

### 原生图片 Macro

```twee
<<image "tree.png" "树">>
<<image $portrait $name>>
```

路径相对于 `resources/img/`；`tree.png` 与 `img/tree.png` 指向同一资源。
不接受绝对路径、父目录跳转、盘符或 URL。alt 为可选字符串，默认为空。
Tauri 通过现有 Resource 协议显示图片；TUI 将 alt 放进方框，不显示路径。
图片不提供 caption 字段，说明正文可单独使用 `print`。

### 原生状态条 Macro

```twee
<<meter "体力" $stamina 0 100>>
```

四个参数都必填，label 必须是字符串，其余参数必须是有限数值，且 min < max。
值可以超出量程：保留原值，Host 将填充限制在空条与满条之间。
宏复用 `Surface.component("meter", 1, {label, value, min, max}, fallback)` 的现有语义链，
不另设 Meter 类型。TUI 使用十格字符条；Tauri 使用图形条，外观由 Host/CSS 决定。

### 音频声明

`<<audio resource channel? tag?>>` 声明循环背景音，默认 channel 为 `bgm`。
Script 使用 `Audio.play(resource, { channel?, tags?, loop?, volume? })` 和 `Audio.stop(channel)`。
按当前主 Passage 的 tag 计算所需音频；Header/Footer 可提供公共规则；同音频续播，离开自动停止。
Audio 通过 RuntimeUpdate effect 交付，不属于 SemanticNode、Surface 或 Save。
完整规则见 [Audio](../author/audio.md)。

音频路径相对于 `resources/audio/`，保留 `audio/` 前缀也会归一到同一资源。

### 弹窗与页面

所有页面正文在打开时执行一次，Host 负责切页与关闭；重开重新执行正文。
无导航 link 不创建历史项；其独立正文不支持 include/goto。完整规则见[弹窗与页面](../author/dialog.md)。

## Print 文本选项

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

### 颜色

color 是 0..=63 的色阶，**颜色由 Host 映射**（对齐二进制边界：灰阶 0-7（白`1`→亮灰`2`→浅灰`3`→灰`4`→深灰`5`→暗灰`6`→黑`7`），光谱 8-63（红`8`→橙`16`→黄`24`→绿`32`→蓝`40`→紫`48`→深紫`56`→`63`，每色相 8 级）），0 为正文默认（不染色）。必须是 0..=63 的整数，否则报 `macro.print.invalid_arguments`。
Tauri 默认 Renderer 会计算并验证 0..63 的全部映射，游戏作者无需为色阶编写 64 条 CSS；
只有希望覆盖品牌色时才使用 `[data-color="N"]`。

### 字形

| style      | 含义               | 渲染提示                                  |
| ---------- | ------------------ | ----------------------------------------- |
| `emphasis` | 语气强调           | 斜体                                      |
| `strong`   | 重要               | 加粗                                      |
| `code`     | 代码/标识符/键位   | 等宽                                      |
| `quote`    | 引文/信件/留言     | 引用块                                    |
| `marked`   | 需要玩家注意       | 半透明高亮底色；与 color 文字颜色相互独立 |
| `small`    | 注释/脚注/次要信息 | 小字                                      |
| `inserted` | 新增内容           | 下划线/加号（TUI `++…++`）                |
| `deleted`  | 删除/废弃内容      | 删除线（TUI `~~…~~`）                     |

Tauri 默认主题已经实现全部 8 种字形及 heading 排版；作者 CSS 只负责可选的品牌覆盖。

### 延迟

delay 是毫秒，`0..=86400000` 的整数。Host 在此之前不呈现内容；动画方式不属于协议；
TUI 把延迟文本停放在 `frame.delayed`，由消费方按 `render_at` 到点显示，终端无平滑
过渡但时序一致。

### 标题

heading 是 `1` 或 `2` 的结构性标题级别。Tauri 映射为 `h1`/`h2`，TUI 用加粗下划线提示；
普通标题不划分弹窗页面。显式分页使用 `<<dialog "默认页标题">>` 与 `<<page "页标题">>`，
单页和多页共用同一语法；无导航 `<<link "文本">>` 点击执行正文而不进入新 Passage。
详见[弹窗与页面](../author/dialog.md)。

## 保留语法

`return` 已被 Parser/HIR 识别，但执行层尚未提供返回值调用域；不能用于可运行故事，也不能由动态 Macro 覆盖。

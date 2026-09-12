"use strict"

// 原生宏的形态、语法与说明共用一份目录；依据 docs/reference/api-and-syntax.md。
const MACRO_APIS = Object.freeze({
  dialog: {
    kind: "container",
    signature: '<<dialog "默认页标题">><<page "页标题">>正文<</dialog>>',
    description:
      "打开含一个或多个 page 的弹窗；参数选择默认页标题，标题必须存在且唯一；Host 自带关闭按钮",
  },
  page: {
    kind: "clause",
    signature: '<<page "页标题">>',
    description: "dialog 的直接子句，正文延续到下一个 page 或 /dialog；不需要 /page",
  },
  audio: {
    kind: "inline",
    signature: '<<audio "forest.ogg" "ambience" "forest">>',
    description:
      "声明当前 Passage 所需的循环背景音；位置参数为资源、可选 channel（默认 bgm）、可选 tag；相邻页面同音频连续播放",
  },
  if: {
    kind: "container",
    signature: "<<if condition>>...<</if>>",
    description: "条件分支容器",
  },
  elseif: {
    kind: "clause",
    signature: "<<elseif condition>>",
    description: "`if` 内的附加条件分支",
  },
  else: {
    kind: "clause",
    signature: "<<else>>",
    description: "`if` 内的最终分支",
  },
  switch: {
    kind: "container",
    signature: "<<switch value>>...<</switch>>",
    description: "严格相等的多分支容器",
  },
  case: {
    kind: "clause",
    signature: "<<case value>>",
    description: "`switch` 的匹配分支",
  },
  default: {
    kind: "clause",
    signature: "<<default>>",
    description: "`switch` 的最终分支",
  },
  for: {
    kind: "container",
    signature:
      "<<for $x of collection>>...<</for>>\n<<for $key in collection>>...<</for>>\n<<for $x range start to end step step>>...<</for>>",
    description: "遍历集合值；遍历集合键或索引；数字范围循环，`step` 可省略",
  },
  while: {
    kind: "container",
    signature: "<<while condition>>...<</while>>",
    description: "条件循环",
  },
  break: {
    kind: "inline",
    signature: "<<break>>",
    description: "结束当前循环",
  },
  continue: {
    kind: "inline",
    signature: "<<continue>>",
    description: "进入当前循环下一轮",
  },
  set: {
    kind: "inline",
    signature: "<<set $name = value>>",
    description: "赋值；也接受 `to` 写法",
  },
  unset: {
    kind: "inline",
    signature: "<<unset $name>>",
    description: "删除可写目标",
  },
  run: {
    kind: "inline",
    signature: "<<run expression>>",
    description: "求值并丢弃结果，保留副作用",
  },
  print: {
    kind: "inline",
    signature:
      "<<print expression [color] [style...]>>\n<<print expression {color, styles, delay, heading}>>",
    description:
      "求值并入 Passage 输出；带选项时产生带语义样式、color、delay 与结构性标题的 StyledText",
  },
  meter: {
    kind: "inline",
    signature: '<<meter "体力" $stamina 0 100>>',
    description:
      "参数依次为 label、value、min、max；数值必须有限且 min < max。TUI 显示字符状态条，Tauri 显示图形状态条。",
  },
  image: {
    kind: "inline",
    signature: '<<image "img/tree.png" "树">>',
    description:
      "使用 Resource 逻辑路径；alt 为可选字符串。Tauri 显示图片，TUI 将 alt 显示在方框中。",
  },
  include: {
    kind: "inline",
    signature: '<<include "Passage">>',
    description: "在当前位置执行另一 Passage，不发生导航",
  },
  goto: {
    kind: "inline",
    signature: '<<goto "Passage">>',
    description: "请求导航并停止当前 Passage",
  },
  link: {
    kind: "container",
    signature: '<<link "文本">>...<</link>> 或 <<link [[文本|Passage]]>>...<</link>>',
    description: "建立玩家可点击的导航动作；正文激活后执行",
  },
  button: {
    kind: "container",
    signature: "<<button [[文本|Passage]]>>...<</button>>",
    description: "与 link 共享事务语义，但由 Host 呈现为按钮",
  },
  replace: {
    kind: "container",
    signature: '<<replace "header">>...<</replace>>',
    description: "用正文替换 `header/main/footer/bar/bar-stowed/dialog` 固定区域或稳定 Surface key",
  },
  slot: {
    kind: "container",
    signature: '<<slot "status">>...<</slot>>\n<<slot "status" "panel" "row">>...<</slot>>',
    description: "建立稳定内容槽；`panel` 请求面板，`stack`／`row` 选择上下或同行排列",
  },
  silently: {
    kind: "container",
    signature: "<<silently>>...<</silently>>",
    description: "执行正文但抑制其直接输出",
  },
  exit: {
    kind: "inline",
    signature: "<<exit>>",
    description: "停止当前执行正文",
  },
  return: {
    kind: "inline",
    signature: "<<return expression>>",
    description: "从可返回的 Macro/Widget 正文返回，可省略值",
  },
  capture: {
    kind: "container",
    signature: "<<capture @name @index>>...<</capture>>",
    description: "把列出的局部变量捕获进延迟正文",
  },
  widget: {
    kind: "container",
    signature: '<<widget "name">>...<</widget>>',
    description:
      "在带 `[widget]` Tag 的 Passage 中定义 Inline Macro；调用写作 `<<name ...>>`，不带调用侧闭合标签",
  },
  checkbox: {
    kind: "inline",
    signature: '<<checkbox "$name" unchecked checked>>',
    description: "勾选时写入 `checked`，取消时写入 `unchecked`",
  },
  radiobutton: {
    kind: "inline",
    signature: '<<radiobutton "$name" value>>',
    description: "选中时把 `value` 写入同一 receiver",
  },
  textbox: {
    kind: "inline",
    signature: '<<textbox "$name" default>>',
    description: "编辑完成时写入文字；receiver 未定义时先写默认值",
  },
})

module.exports = { MACRO_APIS }

"use strict"

// 原生宏的形态、语法与说明共用一份目录；依据 docs/reference/api-and-syntax.md。
const MACRO_APIS = Object.freeze({
  dialog: {
    kind: "container",
    signature: '<<dialog "默认页标题">><<page "页标题">>正文<</dialog>>',
    description:
      "打开含一个或多个 page 的弹窗；参数选择默认页标题，标题必须存在且唯一；Host 自带关闭按钮。\n\nOpen a dialog with one or more pages. The default page title must exist and be unique; the host provides a close button.",
  },
  page: {
    kind: "clause",
    signature: '<<page "页标题">>',
    description:
      "dialog 的直接子句，正文延续到下一个 page 或 /dialog；不需要 /page。\n\nA direct dialog clause. Its body ends at the next page or /dialog; no /page is needed.",
  },
  audio: {
    kind: "inline",
    signature: '<<audio "forest.ogg" "ambience" "forest">>',
    description:
      "声明当前 Passage 所需的循环背景音；位置参数为资源、可选 channel（默认 bgm）、可选 tag；相邻页面同音频连续播放。\n\nDeclare looping background audio for this Passage: resource, optional channel (bgm by default), then optional tag. Matching audio continues across adjacent pages.",
  },
  if: {
    kind: "container",
    signature: "<<if condition>>...<</if>>",
    description: "条件分支容器。\n\nRender a conditional branch.",
  },
  elseif: {
    kind: "clause",
    signature: "<<elseif condition>>",
    description: "`if` 内的附加条件分支。\n\nAdd a conditional branch inside if.",
  },
  else: {
    kind: "clause",
    signature: "<<else>>",
    description: "`if` 内的最终分支。\n\nProvide the final fallback branch inside if.",
  },
  switch: {
    kind: "container",
    signature: "<<switch value>>...<</switch>>",
    description: "严格相等的多分支容器。\n\nSelect a branch by strict equality.",
  },
  case: {
    kind: "clause",
    signature: "<<case value>>",
    description: "`switch` 的匹配分支。\n\nProvide a matching branch inside switch.",
  },
  default: {
    kind: "clause",
    signature: "<<default>>",
    description: "`switch` 的最终分支。\n\nProvide the fallback branch inside switch.",
  },
  for: {
    kind: "container",
    signature:
      "<<for $x of collection>>...<</for>>\n<<for $key in collection>>...<</for>>\n<<for $x range start to end step step>>...<</for>>",
    description:
      "遍历集合值；遍历集合键或索引；数字范围循环，`step` 可省略。\n\nIterate over collection values, keys or indices, or a numeric range. The step is optional.",
  },
  while: {
    kind: "container",
    signature: "<<while condition>>...<</while>>",
    description: "条件循环。\n\nRepeat the body while the condition is true.",
  },
  break: {
    kind: "inline",
    signature: "<<break>>",
    description: "结束当前循环。\n\nExit the current loop.",
  },
  continue: {
    kind: "inline",
    signature: "<<continue>>",
    description: "进入当前循环下一轮。\n\nContinue with the next loop iteration.",
  },
  set: {
    kind: "inline",
    signature: "<<set $name = value>>",
    description: "赋值；也接受 `to` 写法。\n\nAssign a value; the to operator is also accepted.",
  },
  unset: {
    kind: "inline",
    signature: "<<unset $name>>",
    description: "删除可写目标。\n\nDelete a writable target.",
  },
  run: {
    kind: "inline",
    signature: "<<run expression>>",
    description:
      "求值并丢弃结果，保留副作用。\n\nEvaluate an expression for its side effects and discard the result.",
  },
  print: {
    kind: "inline",
    signature:
      "<<print expression [color] [style...]>>\n<<print expression {color, styles, delay, heading}>>",
    description:
      "求值并入 Passage 输出；带选项时产生带语义样式、color、delay 与结构性标题的 StyledText。\n\nEvaluate and append to Passage output. Options add semantic styles, color, delay, or a structural heading.",
  },
  meter: {
    kind: "inline",
    signature: '<<meter "体力" $stamina 0 100>>',
    description:
      "参数依次为 label、value、min、max；数值必须有限且 min < max。TUI 显示字符状态条，Tauri 显示图形状态条。\n\nArguments are label, value, min, and max. Numbers must be finite and min < max. TUI renders a text meter; Tauri renders a graphical meter.",
  },
  image: {
    kind: "inline",
    signature: '<<image "img/tree.png" "树">>',
    description:
      "使用 Resource 逻辑路径；alt 为可选字符串。Tauri 显示图片，TUI 将 alt 显示在方框中。\n\nUse a Resource logical path and optional alt text. Tauri displays the image; TUI displays alt text in a frame.",
  },
  include: {
    kind: "inline",
    signature: '<<include "Passage">>',
    description:
      "在当前位置执行另一 Passage，不发生导航。\n\nExecute another Passage at this position without navigating.",
  },
  goto: {
    kind: "inline",
    signature: '<<goto "Passage">>',
    description: "请求导航并停止当前 Passage。\n\nRequest navigation and stop the current Passage.",
  },
  link: {
    kind: "container",
    signature: '<<link "文本">>...<</link>> 或 <<link [[文本|Passage]]>>...<</link>>',
    description:
      "建立玩家可点击的导航动作；正文激活后执行。\n\nCreate a clickable navigation action; its body executes when activated.",
  },
  button: {
    kind: "container",
    signature: "<<button [[文本|Passage]]>>...<</button>>",
    description:
      "与 link 共享事务语义，但由 Host 呈现为按钮。\n\nUse the same transaction semantics as link, rendered as a button by the host.",
  },
  replace: {
    kind: "container",
    signature: '<<replace "header">>...<</replace>>',
    description:
      "用正文替换 `header/main/footer/bar/bar-stowed/dialog` 固定区域或稳定 Surface key。\n\nReplace a fixed region (header/main/footer/bar/bar-stowed/dialog) or a stable Surface key with the body.",
  },
  slot: {
    kind: "container",
    signature: '<<slot "status">>...<</slot>>\n<<slot "status" "panel" "row">>...<</slot>>',
    description:
      "建立稳定内容槽；`panel` 请求面板，`stack`／`row` 选择上下或同行排列。\n\nCreate a stable content slot. panel requests a panel; stack and row select vertical or inline flow.",
  },
  silently: {
    kind: "container",
    signature: "<<silently>>...<</silently>>",
    description:
      "执行正文但抑制其直接输出。\n\nExecute the body while suppressing its direct output.",
  },
  exit: {
    kind: "inline",
    signature: "<<exit>>",
    description: "停止当前执行正文。\n\nStop the current body.",
  },
  return: {
    kind: "inline",
    signature: "<<return expression>>",
    description:
      "从可返回的 Macro/Widget 正文返回，可省略值。\n\nReturn from a Macro or Widget body that supports returns; the value is optional.",
  },
  capture: {
    kind: "container",
    signature: "<<capture @name @index>>...<</capture>>",
    description:
      "把列出的局部变量捕获进延迟正文。\n\nCapture the listed local variables for a deferred body.",
  },
  widget: {
    kind: "container",
    signature: '<<widget "name">>...<</widget>>',
    description:
      "在带 `[widget]` Tag 的 Passage 中定义 Inline Macro；调用写作 `<<name ...>>`，不带调用侧闭合标签。\n\nDefine an inline Macro in a Passage tagged widget. Invoke it as <<name ...>> without a closing tag.",
  },
  checkbox: {
    kind: "inline",
    signature: '<<checkbox "$name" unchecked checked>>',
    description:
      "勾选时写入 `checked`，取消时写入 `unchecked`。\n\nWrite checked when selected and unchecked when cleared.",
  },
  radiobutton: {
    kind: "inline",
    signature: '<<radiobutton "$name" value>>',
    description:
      "选中时把 `value` 写入同一 receiver。\n\nWrite value to the shared receiver when selected.",
  },
  textbox: {
    kind: "inline",
    signature: '<<textbox "$name" default>>',
    description:
      "编辑完成时写入文字；receiver 未定义时先写默认值。\n\nWrite text when editing completes; initialize an undefined receiver with the default.",
  },
})

module.exports = { MACRO_APIS }

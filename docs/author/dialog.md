# 弹窗与页面

`dialog` 打开一个弹窗，参数选择默认页的标题。`page` 开始一页，正文延续到下一个
`page` 或 `<</dialog>>`；不写 `<</page>>`。

```twee
<<link "查看角色">>
<<dialog "装备">>
<<page "属性">>
<<meter "体力" $stamina 0 100>>
<<page "装备">>
<<image "sword.png" "铁剑">>
装备正文。
<<page "经历">>
经历正文。
<</dialog>>
<</link>>
```

点击“查看角色”时才执行正文，打开第二页“装备”。`<<link "文本">>` 不导航，保留当前
Passage 和历史；原有 `<<link [[文本|Passage]]>>` 仍在执行正文后导航。

- 一个 `page` 就是单页弹窗；页数没有一页或两页的限制。
- 默认页与页标题均为字符串表达式；标题不能为空、不能重复，默认页必须存在。
- `page` 必须是 `dialog` 的直接子句。首个 page 前只允许排版空白。
- 页面正文沿用变量、条件、循环、print、image、meter、输入和脚本 Macro。
- 所有页的正文在打开时执行一次；切页由 Host 完成，不重复执行正文。
- 同一弹窗更新保留选中页；关闭后再次点击入口，重新执行并打开默认页。
- 使用 Tauri 自带关闭按钮或 Esc；TUI 用 Esc/Backspace 关闭。没有 `closeDialog` 宏。
- Tauri 页签支持左右键、Home/End；TUI 左右键切页，PgUp/PgDn 滚动正文。
- 普通 `heading` 只表达内容标题，不再用于推断页面。

当前只支持一个活动弹窗，新弹窗替换旧弹窗，不支持嵌套弹窗栈。无导航 link 的独立正文
沿用 Macro Body 的边界，不支持 `include` 或 `goto`；有需要时将内容直接写入页面，
或者用导航 link 进入其他 Passage。错误及异步取消会回滚本次打开，不提交半个弹窗。

脚本的 `Surface.region("dialog", ...)` 仍可显示普通单页内容；需要显式分页时使用本页的
原生宏。完整可运行用例见 `examples` 的 `DialogGallery`。

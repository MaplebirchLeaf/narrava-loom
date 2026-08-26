# Narrava Loom Twee for VS Code

为 Narrava Loom 的 `.twee` 文件提供语法高亮、跨文件 Macro／Passage 识别、错误诊断、
跳转定义、补全、括号补全和 `/% ... %/` 注释配置。
关键字、Macro 结构和诊断只依据 Narrava Core 当前已实现契约；TextMate scope 使用通用命名，
以适配常见 VS Code 主题。

高亮范围包括金色 Passage 名、独立颜色的 Passage tag、HTML 标签与属性、内置及自定义 Macro、
闭合 Macro、`$`/`_`/`@`/`setup` 变量链、Expression 内置函数、字符串、数字、
运算符、反引号字符串及 `${...}`、`<<link [[label|target]]>>...<</link>>` 链接和 HTML entity。
`[[...]]` 只在 `link` Macro 参数内着色，Narrava 不支持脱离 Macro 的裸链接，
也不支持 `[[target<-label]]` 形式。只有反引号字符串内的 `${...}`
按插值表达式着色，普通正文中的 `${...}` 不会被当成插值。

链接的外层方括号使用低调的结构色，内层方括号与 `|` 使用链接操作符色，标签和目标 Passage
保持各自的语义颜色。链接目标也进入工作区 Passage 索引。在 `[[进入大厅|Hall]]` 的 `Hall` 上按住 `Ctrl`
并单击（macOS 使用 `Cmd+单击`）会跳到同文件或其他 `.twee` 文件中的 `:: Hall`；目标不存在时，
扩展只在目标名称下报告 `Passage \`Hall\` 未定义`。Core 保留的 `Start`、`StoryInit`、
`Header`、`Footer`、`Bar`和`BarStowed` 使用特殊 Passage 颜色，与普通 Passage 区分。
这六个特殊 Passage 都不能带 Tag；扩展会在 Tag 位置直接报告错误。

Widget 的规范定义形式是 `<<widget "name">>`：定义位置的 `"name"` 保持字符串色；
工作区内已定义的 `<<name>>` 会获得紫色语义高亮。扩展会扫描 `.twee` 中的 Widget，
以及 `.js`/`.ts` 中的 `Macro.add()`、`Macro.update()`；未定义的 Macro 保持中性色并报告错误。
因此一个文件定义、另一个文件调用也能识别。Macro 的 `<<`、`/`、`>>` 复用 HTML
标签的 `punctuation.definition.tag` 边界 scope，与 `</...>` 一样渲染为灰色标点；
反引号字符串内 `${` 的 `$` 使用模板插值起始色，`{`/`}` 使用嵌入区域边界色，
不再与 `$hero` 混在一起，也不会作为普通运算符显示成白色。
变量链也按语义拆开：`$hero`、`setup` 是变量根，`.` 是访问符，`profile`、`build`
等后缀是属性；访问符使用独立的 accessor scope，在 `${...}` 内也不会继承反引号
字符串的颜色，长表达式可以一眼分清对象与成员。

在 Twee 表达式中 Ctrl+点击项目脚本函数（例如 `scriptedGreeting()`）会跳到工作区内
`.js`/`.ts` 的函数声明或箭头函数定义。`Object.assign()` 这类 ECMAScript 原生 API
没有项目内定义文件，因此只参与语法高亮，不提供虚假的工作区跳转。

扩展还读取脚本 Macro 的 `body: "inline" | "container"`。`widget` 的定义正文是 Container，
但它注册出来的自定义 Macro 固定为 Inline 调用，例如 `<<highlightCard "内容">>`。
`<<inlineMacro>><</inlineMacro>>` 会报告多余闭合，`<<containerMacro>>` 缺少对应闭合或闭合名
不匹配也会报告错误；`if/switch/for/while/link/silently/capture/widget` 使用各自的内置结构规则。

开发安装：

1. 在 VS Code 中打开本目录。
2. 按 `F5` 启动 Extension Development Host。
3. 打开任意 `.twee` 文件。

打包安装（产物统一输出到仓库 `dist/vscode-narrava-loom/`）：

```bash
cd ../.. && scripts/build-vscode-extension.sh
```

脚本会在缺少依赖时运行 `npm ci`，随后执行回归测试，并把精确版本的 VSIX 写入
`dist/vscode-narrava-loom/`。构建并立即安装：

```bash
scripts/build-vscode-extension.sh --install
# VSCodium 等兼容编辑器：
scripts/build-vscode-extension.sh --install --editor codium
```

扩展不访问网络。它只读取当前工作区的 `.twee`、`.js` 和 `.ts` 文件来建立 Macro 索引。

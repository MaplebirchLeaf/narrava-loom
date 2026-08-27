# TypeScript/JavaScript、Macro 与 Resource

## TypeScript/JavaScript：什么时候才需要

简单故事不需要脚本。遇到重复计算、复杂数据处理、自定义 Macro 或资源读取时再使用。

创建 `contents/scripts/main.ts`：

```ts
/// <reference path="../../../bindings/typescript/narrava.d.ts" />

function greeting(name: string): string {
  return `欢迎，${name}！`
}

State.global.set("greeting", greeting)
```

然后在 Twee 中调用：

```twee
<<print greeting("Maple")>>
```

重要规则：

- 脚本内声明的函数不会自动进入 Twee；必须用 `State.global.set/extend` 显式导入；
- `.ts` 不需要作者预编译；
- Script Bundle 按发现后的稳定路径顺序执行；不要用隐含文件顺序表达复杂依赖；
- 当前执行环境是 Rust Worker，不提供浏览器或 Tauri API；
- 可保存数据只能是 Narrava 数据，函数句柄不能进入存档变量图。

### 13.1 为什么游戏脚本直接使用顶层单例

游戏脚本在 Rust 内的 ECMAScript Runtime 中执行。

Script Binding 直接提供职责明确的顶层单例，不再套一层 `narrava` 命名空间：

```ts
State.variables.set("coins", 10)
Logger.info("game", "脚本已加载")
Event.emit("game:ready", { coins: 10 })
```

`Engine`、`State`、`Macro`、`Story`、`Logger`、`Event`、`Host`、`Save`、`Resource`、`I18n`
和 `Surface` 是彼此独立的公开契约。Worker 中不存在 `narrava.Save` 或聚合对象
`globalThis.narrava`。

WebView DevTools（F12）只属于开发模式调试桥，不是游戏脚本 API，发布模式不会注入。

上面的三斜线声明路径取决于脚本文件与仓库 `bindings/` 的相对位置。复制 `examples/` 时无需
修改；如果把游戏移到仓库外，编辑器可能找不到声明文件，但这不影响 Host 运行。可把
`narrava.d.ts` 复制到自己的类型目录，并调整 reference 路径。

## State 脚本 API

四个入口：

```ts
State.global.get("name")
State.global.has("name")
State.global.set("name", value)
State.global.del("name")
State.global.extend({ one: 1, two: 2 })

State.variables.set("coins", 3)
State.temporary.set("result", "ok")
State.setup.set({ difficulty: "normal" })
```

`set` 返回旧值；`del` 删除并返回旧值；`extend` 返回插入和替换的数量。`global` 可存放导出的
脚本函数；`variables`、`temporary` 和 `setup` 应保持为可转换的 Narrava 数据。

## 自定义 Macro

最小同步 Macro：

```ts
Macro.add("sparkle", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () => "✨",
})
```

Twee：

```twee
这里有一道光：<<sparkle>>
```

Promise Macro 可以立即完成，也可以等待 Host 的受控异步操作：

```ts
Macro.add("delayedAnswer", {
  body: "inline",
  arguments: "raw",
  execution: "async",
  handler: async () => {
    await Host.delay(250)
    return 42
  },
})
```

`Host.delay(milliseconds)` 不是浏览器 `setTimeout`。它会暂停当前 Engine 事务，把 continuation
留在 Rust Core；时间到后 Tauri Host 恢复同一个执行 Token、VM 位置和 Macro 局部域。允许范围
是 0 到 86400000 毫秒。一个 Macro 同时只能等待一个 Host 操作，但恢复后可以继续等待下一次。

不要自己构造永不完成的 Promise；没有受管 Host 操作的未决 Promise 会得到
`script.macro_unmanaged_promise`。Worker 没有 `fetch`、DOM、浏览器计时器或任意
Tauri 调用。文件选择与网络也尚未成为公开能力，因为它们需要各自的权限和结果契约。

当前 Host 自定义 Macro 可以返回可显示标量，也可以返回下节介绍的 `Surface` 语义片段。
容器正文、编译器 Expression 参数、完整 before/after 生命周期还不是可依赖的 Tauri 作者功能。

## Surface 语义渲染

结构化输出使用冻结的 `Surface` builder：

```ts
Macro.add("statusCard", {
  body: "inline",
  arguments: "raw",
  execution: "sync",
  handler: () => Surface.fragment(
    Surface.text("体力不足", {
      key: "stamina-warning",
      styles: ["strong"],
      color: 40,
    }),
    Surface.text("第一页", { key: "page-one", heading: 2 }),
    Surface.image("images/hero.png", {
      key: "hero",
      alt: "站在森林入口的主角",
      caption: "图片来自 Resource。",
    }),
  ),
})
```

可组合的文本结构为 `emphasis`、`strong`、`code`、`quote`、`marked`、`small`、
`inserted`、`deleted` 共 8 个。`color` 是 0..=63 的 Narrava 标准调色板索引，Host 负责映射。
`Surface.text()` 与 Twee 的 `<<print>>` 还可携带 `delay`（毫秒，0..=86400000）与
结构性 `heading`（1 或 2，用于弹窗页签等页面划分，不属于字形样式）。delay 让渲染器在此之前
保持文字不可见；具体动画由 Host 决定。

`Surface.region()` 可写入 `header`、`main`、`footer`、`bar`、`dialog`。Region 的 children
只能是普通字符串或 Surface 节点。`dialog` 中可以使用
`Surface.action("关闭", "dismiss")`，这是 Host 动作，不会伪造 Passage 导航。

版本化组件必须有 fallback：

```ts
Surface.component(
  "meter",
  1,
  { label: "体力", value: 42, min: 0, max: 100 },
  ["体力：42 / 100"],
  { key: "stamina-meter" },
)
```

Tauri 原生支持 `meter@1`。其他 Host 或未知版本显示 fallback。`properties` 只能包含有限纯数据，
不能放函数、DOM、Tauri 对象或循环引用。稳定 `key` 应描述同一逻辑节点；同一输出内重复 key
会报错。完整可运行示例见 `examples` 的 `SurfaceGallery` Passage。

## Resource 资源

所有资源放进 `resources/`。例如：

```text
my-game/resources/data/guide.txt
my-game/resources/images/forest.png
```

脚本中使用的逻辑路径不带 `resources/`：

```ts
Resource.has("data/guide.txt")
Resource.paths()
Resource.info("images/forest.png")
Resource.read("images/forest.png")
Resource.text("data/guide.txt")
Resource.pick(["data/guide.zh-CN.txt", "data/guide.txt"])
```

- `paths()` 返回稳定排序的全部路径；
- `info()` 返回路径、媒体类型和字节大小；
- `read()` 返回 `Uint8Array`；
- `text()` 只适合 UTF-8 文本；
- 开发目录只在首次 `read()`/`text()` 时读取对应文件，成功内容会缓存；读取失败或文本不是
  UTF-8 时会抛出错误，不会伪装成 `undefined`；
- `pick()` 返回候选列表中第一个存在的路径；
- 未知扩展名仍可作为二进制资源；
- 路径拒绝绝对路径、空段、`.`、`..` 和反斜杠。

## CSS 和 `resource("path")`

CSS 完全可选。没有 `styles/` 时，Tauri Host 使用内置的完整默认样式。

要覆盖外观，创建 `styles/game.css`：

```css
nv-story {
  --narrava-background: #16130f;
  --narrava-text: #f2e8d5;
  --narrava-accent: #d6a85f;
  --narrava-accent-hover: #f0c878;
}

nv-passage {
  max-width: 48em;
}
```

使用游戏资源作为背景：

```css
nv-story {
  background-image: resource("images/forest.png");
  background-size: cover;
}
```

Host 会把 `resource("...")` 转为受 CSP 允许的 `narrava-resource://localhost/...` URL，按需
读取对应资源。这里的 `localhost` 只是 Tauri 自定义协议的虚拟 host：不会连接网络、没有端口，
也不是仅开发模式有效；正式安装包使用同一机制。推荐只依赖 `nv-story`、
`nv-passage`、`.passage-header`、`.passage-main`、`.passage-footer`、`nv-ui-bar`、`#nv-dialog`
和 `--narrava-*` 变量；更深的内部节点不保证长期兼容。

CSS 只能改变外观，不能执行游戏脚本、访问 Rust 或替换 Renderer。

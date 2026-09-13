# 游戏脚本

脚本在 Rust 中的 ECMAScript 环境运行；Twee Expression 是另一种受控语言。
全局对象和签名见[脚本 API](../reference/script-api.md)。

## 导出函数给 Twee

简单故事不需要脚本。遇到重复计算、复杂数据处理、自定义 Macro 或资源读取时再使用。

创建 `contents/scripts/main.ts`：

```ts
function greeting(name: string): string {
  return `欢迎，${name}！`
}

State.global.set("greeting", greeting)
```

然后在 Twee 中调用：

```twee
<<print greeting("Author")>>
```

执行规则：

- 脚本内声明的函数不会自动进入 Twee；必须用 `State.global.set/extend` 显式导入；
- `.ts` 不需要作者预编译；
- Script Bundle 按发现后的稳定路径顺序执行；不要用隐含文件顺序表达复杂依赖；
- 当前执行环境是 Rust Worker，不提供浏览器或 Tauri API；
- 可保存数据只能是 Narrava 数据，函数句柄不能进入存档变量图。

## 编辑器类型配置

游戏根目录的 [`tsconfig.json`](../../examples/tsconfig.json) 统一加载 Narrava 声明，覆盖
`contents/` 下的 TS/JS 脚本。在仓库内复制 `examples/` 时保留该文件，仓库根目录运行
`bun install` 后即可使用；新增脚本不需要逐文件添加三斜线引用。

这份配置只加载 `@narrava-loom/types` 和 ECMAScript 标准库，并关闭自动类型获取。
若 `setup` 被识别为 `Mocha.HookFunction`，或 `Event` 被识别为浏览器事件，说明编辑器加载了
游戏运行环境以外的类型。确认游戏根目录保留上述配置；在 VS Code 中可用“TypeScript: Go to
Project Configuration”检查当前脚本所属的配置。

把游戏移到仓库外且没有安装类型包时，保留该配置并完成两步：

1. 将 `bindings/typescript/narrava.d.ts` 和 `narrava-contract.generated.d.ts` 一起复制到游戏的
   `types/` 目录。
2. 将 `compilerOptions.types` 改为 `[]`，并在 `include` 中增加 `"types/**/*.d.ts"`。

`tsconfig.json` 只服务编辑器和类型检查，不参与 Host 的脚本加载或执行。

## State 脚本 API

日常变量操作直接使用属性语法：

```ts
V.coins = 3 // State.variables：进入存档
T.result = "ok" // State.temporary：读档时清空
setup.difficulty = "normal" // State.setup：启动配置

const coins = V.coins
const dynamic = V[variableName]
delete T.result
```

`V`、`T` 和 `setup` 是活动 Rust State 的代理，不是 JavaScript 副本；读取、赋值、`in`、
`Object.keys` 与 `delete` 都立即作用于同一份状态。需要动态键、旧值或批量写入时使用完整入口：

```ts
State.global.get("name")
State.global.has("name")
State.global.set("name", value)
State.global.del("name")
State.global.extend({ one: 1, two: 2 })

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
留在 Rust Core；时间到后 Host 恢复同一个执行 Token、VM 位置和 Macro 局部域。允许范围
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
  handler: () =>
    Surface.fragment(
      Surface.text("体力不足", {
        key: "stamina-warning",
        styles: ["strong"],
        color: 40,
      }),
      Surface.text("角色状态", { key: "status-title", heading: 2 }),
      Surface.image("hero.png", {
        key: "hero",
        alt: "站在森林入口的主角",
      }),
    ),
})
```

可组合的文本结构为 `emphasis`、`strong`、`code`、`quote`、`marked`、`small`、
`inserted`、`deleted` 共 8 个。`color` 是 0..=63 的 Narrava 标准调色板索引，Host 负责映射。
`Surface.text()` 与 Twee 的 `<<print>>` 还可携带 `delay`（毫秒，0..=86400000）与
结构性 `heading`（1 或 2，用于正文标题，不划分弹窗页面）。delay 让渲染器在此之前
保持文字不可见；具体动画由 Host 决定。

`Surface.region()` 可写入 `header`、`main`、`footer`、`bar`、`dialog`。Region 的 children
只能是普通字符串或 Surface 节点。`dialog` 中可以使用
`Surface.action("关闭", "dismiss")`，这是 Host 动作，不会伪造 Passage 导航。

版本化组件必须有 fallback：

```ts
Surface.component("meter", 1, { label: "体力", value: 42, min: 0, max: 100 }, ["体力：42 / 100"], {
  key: "stamina-meter",
})
```

Tauri 将 `meter@1` 显示为图形状态条，TUI 显示字符状态条。未知能力或版本显示 fallback。`properties` 只能包含有限纯数据，
不能放函数、DOM、Tauri 对象或循环引用。稳定 `key` 应描述同一逻辑节点；同一输出内重复 key
会报错。完整可运行示例见 `examples` 的 `SurfaceGallery` Passage。

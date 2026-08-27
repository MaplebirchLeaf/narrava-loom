# Narrava 第三版架构纲要

> 状态：基础结构实现中
>
> 更新日期：2026-08-22

## 1. 项目定位

Narrava 是一个以 Rust 为核心、可嵌入不同宿主环境的叙事内容编译、运行和模组管理系统。

项目收集两条源码管线：

| 内容 | 职责 |
|---|---|
| `.twee` | 剧情、Passage、选择和叙事控制流 |
| `.ts` / `.js` | 复杂逻辑与运行时扩展 |

以下是构建或运行期数据，不与三类源码混为一层：

| 内容 | 职责 |
|---|---|
| `.nmod` | 模组包 |
| `.nres` | 构建后的资源包 |
| `.nsave` | 玩家存档 |
| `.nlang` | 语言安装包；包含 manifest、紧凑 NMSG 与动态字典 JSON |

开发项目、发布后的游戏包和安装模组后的有效运行版本必须明确分离。

## 2. 核心原则

1. Rust 负责内容加载、编译、运行、模组、存档和缓存等核心能力。
2. 不同内容使用独立管线，不把 scripts 塞进 Narrative IR。
3. 发布包保留可重建的基础输入，模组变化后从基础内容重新生成有效构建。
4. 内嵌模组顺序来自 `config.toml`；玩家模组默认禁用，并由游戏内界面管理。
5. 已完成最小可运行闭环；当前收束可嵌入基础游戏 Runtime 与 Host 边界。
6. 本体、I18n 与 Script Bundle 位于 `narrava-loom-core` crate；Surface 协议语义与跨 Host 传输层位于 `narrava-loom-protocol` crate（依赖 Core）；ECMAScript 游戏脚本执行与宏分发位于 `narrava-loom-script` crate（依赖 Core 与 Protocol），Host 同时消费三者。ModLoader 是只依赖 Core 的独立项目。所有依赖保持单向，Core 不反向依赖上层 crate。

具体目录、依赖方向与产物归属见[仓库布局与文件归属](../development/repository-layout.md)。

## 3. 核心管线

```text
基础内容 → Token → AST → 初始 HIR
        ↓
提取 I18nCatalog 并应用当前语言
        ↓
形成本地化后的候选内容
        ↓
按明确顺序应用 .nmod 修改
        ↓
最终 Semantic Analysis
        ↓
有效 HIR → MIR → LIR → Bytecode
        ↓
Rust VM
```

两条源码管线彼此独立：

```text
.twee → HIR → MIR → LIR → Bytecode → Rust Runtime／VM

.ts → .js ─┐
.js ───────┴→ Script Bundle → ECMAScript Runtime Adapter

assets → 处理与混淆 → resources/ → Resource Identity / Host
```

`ECMAScript Runtime Adapter` 只表示目标环境提供的脚本执行能力，不限定为浏览器。浏览器、Tauri WebView 或未来的嵌入式 JavaScript Runtime 可以提供不同 Adapter；Narrava 核心不把某一种宿主写进 Narrative IR。

## 4. 项目与发布结构

开发项目：

```text
NarravaProject/
├─ config.toml
├─ assets/
├─ contents/
│  ├─ story/
│  └─ scripts/
├─ mods/
├─ save/
├─ cache/
└─ build/
```

`config.toml` 不逐项登记源码文件。当前已实现的游戏配置：

```toml
[game]
id = "example.forest"
name = "Forest"
version = "0.3.1"
default_locale = "zh-CN"
```

- `id`：存档、缓存和模组兼容使用的稳定标识；
- `name`：玩家可见名称；
- `version`：游戏版本；
- `default_locale`：游戏源语言（Twee 原文语言），作为翻译基准与回退终点（BCP 47，如 `zh-CN`）。

起始 Passage 名称固定为 `Start`，由引擎内部约定，不写入配置；入口是否真实存在由编译期语义检查确认。

配置加载时执行基础验证：`id` 非空且不含空白，`name` 非空，`version` 符合 SemVer，`default_locale` 符合语言标签形状。

Source 获取层自动递归扫描 `contents/`；其他配置段在对应功能实现时再加入。

发布版本：

```text
NarravaGame/
├─ narrava
├─ game.nar
├─ languages/
│  ├─ zh-CN.nlang
│  └─ en.nlang
├─ resources/
│  └─ *.nres
├─ mods/
│  └─ *.nmod
└─ save/
```

`game.nar` 的计划内容至少包括：

- 游戏清单；
- 基础源码记录；
- 可执行叙事构建；MIR 保存控制流，LIR 完成执行索引，Bytecode 是 VM 唯一输入；
- JavaScript 脚本包；
- 资源索引；
- 构建信息。

I18n 是发布数据，不属于 `.twee` 或 `.ts/.js` 源码。每种语言由 `manifest.json`、`translations.nmsg` 与 `dictionary.json` 组成，发布时共同包装成 `.nlang`。Runtime 校验目标语言后按稳定文本 ID 查询，缺少条目时回退 `default_locale` 原文。翻译不能改变控制流、添加 Expression 或依赖某一种 Host。

发布内容保留足够的 IR 文本元数据，使开发者控制台可以调用 `I18n.export()` 导出或合并目标语言的 NMSG 与字典 JSON。即使发布包不携带原始 Twee，导出仍来自 IR 翻译目录，不依赖反编译执行指令。消息键以 PassageName 与 IR 结构路径为基础，不以容易变化的原文或行号作为唯一身份。

导出结果最终应分开列出只读原文、来源、受控 placeholder、动态值字典绑定和译者填写的目标文本；空目标文本表示回退默认语言。翻译者只填写允许翻译的字段，不需要修改 Twee、Expression 或 Macro。再次导出时保留仍兼容的译文，补充新增项，并把已删除或 placeholder 不兼容的旧消息完整移入失效报告，不能覆盖翻译成果。

`${expression}`、`$variable` 等动态文本在 AST/HIR 中保留为 Expression，在翻译消息中只表现为已经编译声明的 placeholder。译文可以调整 placeholder 的语序，但必须保留完整集合；JSON 不能添加未知 placeholder，也不能把任意文本重新解释为 Expression。每个动态值可以选择绑定到 `items`、`names` 等 dictionary；空绑定表示直接显示运行时值。字典找不到对应动态值时保留原值。

基础源码记录用于在安装、移除或调整模组后重新构建，不承诺能够绝对隐藏客户端内容。

玩家目录中不保留开发期的 `contents/` 或 `assets/`。构建器对资源进行压缩、重命名或打包后写入 `resources/`；目录内使用资源 ID 或不透明路径，不保留原始开发目录结构。

`game.nar` 通过资源索引引用 `resources/`。Runtime 只能通过资源 ID 获取内容，不直接依赖原始文件名。资源混淆用于提高直接提取难度，不作为绝对安全保证。

### 4.1 包容器

`.nar`、`.nmod` 和 `.nres` 第一阶段统一使用 ZIP 容器，由 Narrava 构建工具生成。`.nar` 在 ZIP 负载前携带 `NAR1` 魔数头（`nar::NAR_MAGIC`）：普通 zip 工具仍可按 ZIP 规范跳过前导字节解压，但 Host 加载时会校验魔数并拒绝无头文件，任意 ZIP 不能只修改后缀就视为合法游戏包；加载器还会验证包类型、清单和内容哈希。

`.nmod` 使用自己的 `mod.toml` 描述模组身份、版本、目标游戏和依赖，不复用 `.nlang` 的语言 manifest。实际内容修改固定发生在 I18n 应用之后；提前解析清单和依赖只用于建立候选顺序，不代表已经修改 Story。

Core 通过 `NmodPackageInput` 接收 Binding 解包后的内存文件，不依赖 ZIP 实现。包级路径只允许 `mod.toml`、`contents/`、`patches/` 与 `resources/*.nres`；通过路径、清单及游戏兼容校验后才生成 `NmodValidatedPackage`。

`.nres` 文件使用资源 ID 或内容哈希命名。已经压缩的图片、音频和视频使用 ZIP Store，文本与索引再使用压缩算法。

### 4.2 内容路径

`contents/` 是开发项目的挂载根目录，不属于保存路径：

```text
磁盘路径：contents/story/main.twee
保存路径：story/main.twee
```

基础源码记录、VFS、包清单和诊断索引统一使用省略 `contents/` 的保存路径。磁盘路径只用于开发期读取文件。

Source 获取完成后按保存路径后缀确定 `SourceKind`，再进入对应管线：

| 后缀 | SourceKind |
|---|---|
| `.twee` | `Twee` |
| `.ts` | `TypeScript` |
| `.js` | `JavaScript` |

Twine 是所属生态，Twee 是文件格式。识别 SourceKind 只表示完成分流，不代表对应编译或打包管线已经实现。

`SourceList` 自动递归发现 `contents/` 下已支持的源码，忽略其他文件且不跟随符号链接。发现结果按平台无关保存路径稳定排序，保证不同设备得到相同顺序。

## 5. Rust 模块边界

`narrava-loom-core` package 的 `src/lib.rs` 提供可嵌入 Core，`src/main.rs` 是最小 CLI Host。Tauri Host 单向依赖 Core；ModLoader 独立演进，不属于 Core package 或 Core 完成度。

| 模块 | 职责 |
|---|---|
| `source`、`config` | 项目输入与配置 |
| `twee`、`hir`、`expression` | 编译和表达式语义 |
| `macro_runtime`、`runtime` | Macro 与 HIR 执行 |
| `state`、`story`、`engine` | 游戏状态、导航与事务 |
| `surface` | 最小宿主无关语义输出 |
| `i18n` | 稳定文本身份与默认语言目录 |
| `diagnostic`、`logger` | 问题数据与可观察记录 |

未来只有在编译边界、依赖或发布方式真正需要时，才拆出 diagnostics、compiler、bytecode、VM、runtime、project 或 CLI crate。源码与测试规范见 [/docs/development/code-style.md](/docs/development/code-style.md)。

## 6. Twee 编译边界

Twee 编译器当前负责从 Source 到 MIR 的叙事管线，不负责运行时控制器或渲染。详细设计与当前进度见 [/docs/architecture/twee.md](/docs/architecture/twee.md)。

```text
Story
└─ Passage
   ├─ Name
   ├─ Tags
   └─ Body
      ├─ Text
      └─ Choice（由 link 等宏产生）
```

语义检查至少覆盖 Passage 全局唯一、入口存在和选择目标存在。名称比较区分大小写。

## 7. 模组模型

模组分为：

- 构建期内置模组：发布时融入基础内容；
- 玩家模组：运行期安装，由玩家在游戏内启用、关闭和排序。

每个 `.nmod` 包内部使用 `mod.toml` 描述自身 ID、版本和依赖。

### 7.1 内嵌模组

开发项目可在 `config.toml` 中选择内嵌模组，并以数组顺序确定构建顺序：

```toml
[mods]
embedded = [
  "mods/official-fix.nmod",
  "mods/chapter-extension.nmod",
]
```

`embedded` 是可选字段；省略或使用空数组表示没有内嵌模组。构建器必须验证依赖关系，但不得静默改变作者写下的顺序。顺序错误、依赖缺失或版本不兼容时直接终止构建。

内嵌模组在发布时融入 `game.nar` 的基础内容，不再作为玩家可关闭的独立模组。

### 7.2 玩家模组

生产版本的 `mods/` 只存放玩家安装的 `.nmod` 文件：

- 新发现的玩家模组默认禁用；
- 启用、关闭和排序通过游戏内模组管理界面完成；
- 引擎保存玩家选择，具体持久化格式留到 Runtime 配置设计时确定；
- 删除或升级模组后，引擎重新验证当前选择；
- 引擎不得根据文件名、目录枚举顺序或模组声明的 `priority` 自动决定覆盖顺序。

玩家确认启用配置时，引擎按界面中的顺序验证：

```text
读取游戏内模组配置
→ 加载已启用模组
→ 验证 ID、版本、依赖和顺序
→ 生成当前有效构建
```

若验证失败，保持上一份可用配置，不进入损坏的有效构建。

`.nmod` 的包加载、身份和依赖验证可以提前完成，但内容修改必须发生在 I18n 已把基础内容修正为当前语言之后。模组不能先改默认语言内容，再要求基础 I18n 猜测如何翻译修改结果。

I18n 后的修改仍分两层：

1. Token 前的文件新增与完整覆盖；
2. AST 后、语义分析前的结构化补丁。

不把字符串搜索替换作为长期模组机制。

## 8. Runtime

Core 向 Host API 暴露 `Engine`、`State`、`Macro`、`Story`、`Logger`、`ModLoader`、`ModUtils`、`Resource`、`Save`、`Event`、`I18n` 等稳定能力。Renderer 不属于 Core 控制器；Host 通过 Surface Protocol 取得语义输出，再交给自己的 Renderer。详细五层边界见 [/docs/architecture/protocol.md](/docs/architecture/protocol.md)。

普通 Passage 的语义输出若不包含导航动作，Engine 会追加 SafeReturn，指向 history 中最近的安全普通 Passage。Core 只定义动作语义与目标，不规定它显示成按钮、链接、3D 对象或终端选项。`[exit]` Passage 执行逻辑但不产生可显示输出，也不进入安全返回目标集合。

`.twee` 运行在以 `State.global` 为根的游戏沙箱中。`scripts` 通过 State API 向 `global`、`setup` 等命名空间登记内容，使其可被 Twee 使用；ECMAScript `export` 与 `import` 不作为进入 State 环境的机制。

Engine 负责启动、Passage 生命周期与事务，不直接实现任何平台表现。Native Twee 正文按字面进入宿主无关的语义 Text；动态求值必须通过 `print` 等显式 Macro 产生 Text。Macro 解析并执行显式动态 Twee 片段。Binding 只转换类型与生命周期，Host Renderer 决定最终表现。脚本模块之间的 `import` 仍可用于组织代码，只是不因此自动进入 `State.global`。

启动顺序保持显式：加载 Core 与游戏、建立 Host API/Binding、注册可选能力、验证必需能力，最后由 `Engine.start()` 进入起始 Passage。首个真实 Host 选择 Tauri，在原生 Rust 后端直接复用 Core；WebView 只实现该 Host 的表现层。Godot 作为第二个结构不同的 Host 验证同一协议，浏览器、TUI、Python 和 Java 后续再通过各自 Binding 接入。任何缺失能力或注册错误都应在 Story 开始前形成 Diagnostic，并可由 Logger 观察。

Rust crate 已以 `src/lib.rs` 提供 Core，`src/main.rs` 只是使用该 library 的最小 CLI Host。后续 Host API 应从 library 边界增量收束，不能把平台事件循环或 Renderer 重新放回 Core。

## 9. 存档与有效构建

存档除游戏状态外，还需记录：

- 游戏和引擎版本；
- 存档格式版本；
- 当前 Passage 与执行状态；
- 有效构建哈希；
- 模组 ID、版本、哈希和顺序；
- 随机数种子。

有效构建缓存由基础包、编译器版本、目标平台、模组内容与顺序共同决定。缓存未命中时从基础输入重新构建。

## 10. 当前阶段：基础游戏 Runtime 完整化

Narrative 编译与运行闭环已经建立：

```text
读取单个 Twee 文件
→ Lexer
→ Parser
→ 基础语义检查
→ HIR
→ MIR
→ LIR
→ Bytecode
→ Rust VM 执行逻辑并产生语义输出
```

这是当前真实实现。Bytecode 已拥有格式头、Opcode、入口表、常量目录、Expression 与完整
Macro HIR，不再借用构建期 MIR/HIR。Core 提供 `.nar` 的拥有型基础源码记录、游戏身份、
格式版本，以及源码和 Bytecode 各自的 SHA-256 完整性校验；已校验包直接读取可执行
Bytecode，并从拥有型源码建立 Script Bundle。ZIP 文件 I/O 由构建工具或 Binding 拥有。

最小输入：

```twee
:: Start
你在森林中醒来。
<<link [[查看四周|LookAround]]>><</link>>

:: LookAround
四周非常安静。
```

当前重点不是继续扩张语法，而是把 Engine、Host API、Surface、Script
Binding、Save 与 I18n 串成可嵌入的基础游戏链。首个平台目标是 Tauri；具体
ECMAScript Runtime、WebView 表现层、文件选择器和存储位置仍由 Tauri Host
拥有。ModLoader、Resource 包、发布缓存与其他平台 Binding 在本体边界稳定后
按独立切片推进。

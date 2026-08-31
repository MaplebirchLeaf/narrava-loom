# Narrava Loom 架构

Narrava Loom 是以 Rust 实现的 Host-neutral 叙事编译与运行核心。Core 处理游戏语义，
Host 处理窗口、输入和文件 IO，Renderer 只解释拥有型 Protocol 数据。

## 设计原则

- Core 不依赖 Tauri、DOM、CSS 或具体 Renderer。
- Twee 与 scripts 使用独立管线，不把 ECMAScript 塞入 Narrative IR。
- State、Story 和可见输出由 Engine 作为一个事务提交或回滚。
- 跨语言、跨线程边界只传递拥有型 DTO，不暴露 Rust 引用或 Runtime 对象。
- 文档只定义已实现的行为；完成度统一记录在[项目状态](../development/status.md)。

目录与依赖约束见[仓库布局](../development/repository-layout.md)。

## 内容管线

```text
.twee → AST → HIR → MIR → LIR → Bytecode → VM

.ts → .js ─┐
.js ─────└→ Script Bundle → ECMAScript Runtime

assets → resources → Resource API → Host
```

Bytecode 是 VM 的唯一叙事指令输入。Script Bundle 由 `narrava-loom-script` 通过 Boa 执行，
Oxc 只用于移除 TypeScript 类型语法。脚本通过受控 Adapter 访问 State、Story、Event、
Reaction、Macro、Resource、I18n 和 Save。

Twee 的 Parser、IR 与 VM 边界见 [Twee 编译器](twee.md)；Expression 与 Macro 分别见
[Expression](expression.md) 和 [Macro](macro.md)。

## 项目输入

开发项目的最小结构为：

```text
NarravaProject/
├─ config.toml
├─ contents/
│  ├─ story/
│  └─ scripts/
└─ assets/
```

`config.toml` 定义游戏身份和源语言：

```toml
[game]
id = "example.forest"
name = "Forest"
version = "1.0.0"
default_locale = "zh-CN"
```

`SourceList` 递归扫描 `contents/`，按平台无关的相对路径排序，并识别 `.twee`、`.ts`
和 `.js`。保存路径不包含 `contents/` 前缀。Source 记录、路径校验和发布存档见
[源码记录](source-record.md)。

## 发布边界

桌面发布目录为：

```text
NarravaGame/
├─ narrava
├─ game.nar
├─ languages/
├─ resources/
└─ save/
```

`game.nar` 包含游戏清单、拥有型 Bytecode、Script Bundle、Source 记录、资源索引和内容
哈希。容器使用 `NAR1` 魔数头与确定性 ZIP 负载；Host 校验包类型、格式版本和
哈希后才运行。玩家目录不携带开发期 `contents/` 或 `assets/`。

`.nlang` 是单语言安装包，由 `manifest.json`、`translations.nmsg` 和 `dictionary.json`
组成。译文可调整 placeholder 顺序，不能添加表达式或改变控制流。详细见
[I18n](i18n.md)。

资源以 ID 访问，Runtime 不依赖开发期文件名。资源打包只提高直接提取成本，
不提供客户端保密保证。

## Crate 边界

| Crate | 职责 |
|---|---|
| `narrava-loom-core` | Source、编译、Bytecode、VM、Engine 与领域状态 |
| `narrava-loom-protocol` | 零 Core 依赖的 Runtime/Host 命令、更新与 Surface DTO |
| `narrava-loom-script` | ECMAScript、RuntimeSession 与 Core/Protocol 适配 |
| `narrava-loom-tauri` | 桌面 Host、Worker、资源 IO 与 WebView Renderer |
| `narrava-loom-tui` | 终端 Host 与 Protocol 语义验证 |

Protocol 不引用 Core；Script Runtime 显式转换两侧类型。Host 不得越过 RuntimeSession
直接修改 State、Story 或 VM frame。

## 运行时所有权

```text
HostInput
   ↓
RuntimeSession → Engine → VM / Macro / Script
   ↓
HostUpdate → Protocol DTO → Renderer
```

Engine 在事务检查点内执行 Passage 生命周期、Macro、Reaction 和导航。同步链完成时一次
提交；诊断、取消、预算耗尽或无效恢复会回滚。异步 Macro 只向 Host 暴露不透明
operation ID，continuation 仍由 Runtime 持有。

Surface 只表达文本、语义样式、区域、交互、稳定 Key 和替换意图。方框、card、间距、
焦点和动画属于 Host。详细见 [Runtime](runtime.md)、[Runtime Session](runtime-session.md) 和
[Host Surface](protocol.md)。

## 领域所有权

- State 持有持久、临时、setup 与 Macro 局部值。
- Story 持有 Passage 索引、历史和当前光标。
- Save 只序列化稳定领域状态，不序列化 Host handle、continuation 或脚本函数。
- Logger 保存结构化运行记录；Diagnostic 表达可定位的失败，两者不代替彼此。
- I18n 选择属于 Runtime 执行上下文，不写入 State。

Save 格式与恢复事务见 [Save](save.md)。

## 未实现范围

模组加载、Android/iOS 平台工程、云存档和通用存档迁移不属于当前 API。
不为这些范围保留空 crate、配置段或占位类型。

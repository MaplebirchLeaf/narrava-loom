# 运行、构建、自检与故障排查

## 运行、检查、构建：三个命令不是一回事

### 只检查游戏

```bash
cargo run -p narrava-loom-core -- my-game
```

它检查配置、Source、Resource、Twee、HIR、MIR、LIR 与 Bytecode，不打开窗口。

### 开发时打开 Tauri

```bash
cargo run -p narrava-loom-tauri -- my-game
```

它编译并运行 Tauri Host 的桌面入口。开发二进制位于：

```text
target/debug/narrava-loom-tauri
```

开发二进制没有内置游戏；请继续显式传入游戏目录。

### 生成优化后的裸二进制

```bash
cargo build --release -p narrava-loom-tauri
```

产物位于：

```text
target/release/narrava-loom-tauri
```

这只是优化后的 Host 可执行文件，不是 Narrava 游戏发行目录。正式目标首先是
`NarravaGame/narrava + game.nar + languages/ + resources/ + mods/ + save/`；`.deb`、`.msi`、
`.dmg` 或 AppImage 只是该目录之后可选的系统外包装。`tauri.conf.json` 故意不直接启用通用
bundle，避免生成身份仍叫 Narrava Loom、又没有携带游戏包的错误安装程序。

仓库约定：所有构建输出统一写入 `dist/`（不入库），不要写进 `target/`。游戏发行目录
输出到 `dist/NarravaGame/`，VS Code 扩展包输出到 `dist/vscode-narrava-loom/`。

### 生成正式可移动游戏目录

先构建优化后的官方 Host，再让 Core 打包游戏：

```bash
cargo build --release -p narrava-loom-tauri
cargo run --release -p narrava-loom-core -- \
  build my-game dist/NarravaGame target/release/narrava-loom-tauri
```

输出固定为：

```text
NarravaGame/
├─ narrava
├─ game.nar
├─ languages/
│  └─ <locale>.nlang
├─ resources/
│  └─ base.nres
├─ mods/
│  └─ *.nmod
└─ save/
```

`game.nar` 是带 `NAR1` 魔数头的 ZIP 容器（普通 zip 工具可直接解压查看内部，但 Host 会校验魔数拒绝无头文件）。构建器不会覆盖已存在的输出目录。把整个目录复制到别处后，直接双击或运行 `narrava`；不传
参数时，它会把自身所在目录当作游戏根目录。游戏作者和玩家都不需要安装 Rust。开发阶段的
`languages/<locale>/` 会被验证并打成一个 `.nlang`；`resources/` 会打成 `base.nres`，同时纳入
`game.nar` 的完整性校验；已有的 `mods/*.nmod` 会原样复制，`save/` 初始为空。

仓库的 `.github/workflows/release.yml` 在 `v*` tag 或手动触发时执行同一构建，分别生成
Linux、Windows、macOS 的可移动 `NarravaGame` 压缩包，并同时构建 VS Code 扩展。这里发布的是
完整游戏目录，不是一个脱离 `game.nar` 的通用 Host 安装器；游戏身份和内容始终一起交付。

## 修改后怎样自检

每次只改一点，然后运行：

```bash
cargo run -p narrava-loom-core -- my-game
```

确认通过后再开 Tauri 桌面窗口：

```bash
cargo run -p narrava-loom-tauri -- my-game
```

这不是移动端启动命令。共享 crate 已保留 mobile entry point，但仓库尚无 Android/iOS 工程与
打包脚本；移动交付必须另行完成 Tauri 平台初始化、签名和真机验收。

普通游戏作者不需要运行 Clippy 或全仓库测试。引擎开发者使用的完整门禁见
[仓库命令](../development/commands.md)。

## 错误信息怎么看

错误通常以 `领域.阶段` 形式出现，例如：

```text
tauri_host.source
tauri_host.twee
tauri_host.hir
tauri_host.mir
tauri_host.lir
script.parse
script.execute
tauri_host.resource
```

这不是乱码。前半段告诉你错误发生在哪一层，后面的 message 才是具体原因。

曾经出现的：

```text
engine.mir.begin_failed：LIR Passage 启动后继续执行失败，事务已回滚
```

不是正常提示。它表示执行失败，但 Core 已把本次未完成修改回滚，避免留下半更新状态。优先检查
当前 Passage 中的 Macro、脚本函数、变量类型和导航目标。

## 超详细故障排查表

| 现象 | 最可能原因 | 处理方法 |
|---|---|---|
| `cargo: command not found` | 没安装 Rust | 安装 rustup 后重新打开终端 |
| Tauri 编译缺少 WebKit/GTK | 系统依赖未安装 | 按 Tauri 官方 prerequisites 安装当前系统依赖 |
| 找不到 `config.toml` | 路径或工作目录错误 | 回到仓库根目录，确认 `my-game/config.toml` 存在 |
| 绝对游戏路径被拒绝 | Host 当前只接受安全相对路径 | 使用 `my-game`，不要使用 `/home/.../my-game` |
| `game.id` 无效 | ID 为空或有空白 | 使用 `author.game-name` 这样的无空白 ID |
| `game.version` 无效 | 不是语义化版本 | 改为 `0.1.0` 等合法版本 |
| `default_locale` 无效 | 语言标签格式错误 | 使用 `zh-CN`、`en` 等语言标签 |
| 找不到 Start | 名称拼错或大小写错误 | 必须准确写 `:: Start` |
| Passage 重名 | 不同文件用了同一名称 | 全局搜索并重命名其中一个 |
| 点击后找不到目标 | link 目标大小写不一致 | 对照 `:: PassageName` 精确修改 |
| `$name` 原样显示 | 正文不会自动插值 | 使用 `<<print $name>>` |
| `.twee` 没读取 | 不在 `contents/` 或扩展名错误 | 放到 `contents/**/*.twee` |
| `.ts` 没执行 | 不在 `contents/` 或启动即报脚本错 | 放到 `contents/**/*.ts` 并看 `script_*` 错误 |
| TS 函数在 Twee 中找不到 | 未显式导入 | `State.global.set("name", function_)` |
| 脚本提示 `window` 不存在 | 游戏脚本运行在 Rust Worker | 删除 DOM/WebView 依赖 |
| Resource 找不到 | 写了 `resources/` 前缀 | 改用 `images/a.png` 这样的逻辑路径 |
| `Resource.text()` 得不到文本 | 文件不是 UTF-8 | 转为 UTF-8 或使用 `read()` |
| CSS 没有加载 | 文件不在 `styles/` | 放入 `styles/**/*.css` |
| CSS 背景资源失败 | resource 路径不存在 | 对照 `Resource.paths()`，不写 `resources/` 前缀 |
| 只有默认深色样式 | 游戏没有 CSS | 正常；默认 CSS 属于 Host |
| 没看到 Narrava Loom 标题 | 固定品牌标题已删除 | 正常；窗口标题来自游戏配置 |
| `Save.export()` 没有文件 | target 非法或目录不可写 | 查看“日志”页与 `save/` 目录权限 |
| 语言列表只有默认语言 | 没有有效语言输入 | 开发目录放 `languages/<locale>/`；发行目录放构建出的 `languages/<locale>.nlang` |
| Promise 一直不完成 | 依赖外部异步任务 | 当前只支持可由 microtask job queue 完成的 Promise |
| 窗口启动后空白 | 启动或 Renderer 报错 | 看错误 Dialog 和终端输出，先运行 Core 检查命令 |
| 再次启动说端口占用 | 通常是你另开的开发服务 | Tauri Host 本身不要求作者启动 HTTP 服务 |

## 推荐的制作顺序

1. 只写 `config.toml` 和一个 `Start`。
2. 加两个 Passage 和来回 link。
3. 加 `$` 变量与一个 `if`。
4. 每一步运行 Core 检查。
5. 故事结构稳定后才拆分多个 Twee 文件。
6. 重复逻辑出现后才加入 TypeScript。
7. 先用 `State.global` 导入一个纯函数。
8. 再加入 Resource。
9. 最后加入可选 CSS 和语言包。
10. 最后用 `:: Bar` 中的 `<<barDemo>>` 入口验证存档、语言和日志。

## 一个可直接复制的最小游戏

`config.toml`：

```toml
[game]
id = "tutorial.first-game"
name = "门后的故事"
version = "0.1.0"
default_locale = "zh-CN"
```

`contents/story/main.twee`：

```twee
:: StoryInit
<<set setup.author to "我">>

:: Start
<<set $name to "旅行者">>
<<set $opened to false>>
你好，<<print $name>>。
<<link [[打开门|Room]]>><</link>>

:: Room
<<set $opened to true>>
门后是一间安静的房间。
<<if $opened>>
你记住了这里。
<</if>>
<<link [[返回|Start]]>><</link>>
```

运行：

```bash
cargo run -p narrava-loom-core -- my-game
cargo run -p narrava-loom-tauri -- my-game
```

这就是一个合法的纯内容游戏：没有 Rust、没有 JavaScript、没有 CSS、没有资源也能运行。

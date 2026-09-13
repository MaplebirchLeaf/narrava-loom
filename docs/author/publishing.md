# 打包游戏

发行物是包含 Host、游戏数据和语言包的可移动目录。玩家运行其中的程序，无需安装 Rust。
制作开发版内容仍需要[快速入门](quick-start.md)中的工具链。

## 构建

先检查游戏，再构建 Host 和发行目录。以下从仓库根目录执行：

```bash
cargo run --locked -p narrava-loom-core -- my-game
cargo build --release --locked -p narrava-loom-tauri
cargo run --release --locked -p narrava-loom-core -- \
  build my-game dist/NarravaGame target/release/narrava-loom-tauri
```

Windows 的 Host 路径使用 `target/release/narrava-loom-tauri.exe`。
`cargo build` 只得到 Host 程序；最后一条才把游戏内容一起打包。
构建器拒绝覆盖已有输出目录，重建时选择新目录或先保存旧发行结果。

## 发行目录

```text
NarravaGame/
├── narrava                  Windows 为 narrava.exe
├── game.nar                 配置、可执行故事与脚本
├── resources/base.nres      游戏资源与打包样式
├── languages/*.nlang        语言包
├── mods/                    已有包的复制位置
└── save/                    初始为空，运行后写入存档
```

发布整个目录。无参数启动 `narrava` 时，以程序所在目录作为游戏根目录。
开发期 `contents/` 不随发行目录交付，`game.nar` 在启动时校验格式和内容哈希。
`mods/` 可以保留已有包，但当前 Runtime 尚未实现模组加载。

## 交付前检查

1. 将 `[host.tauri].developer` 设为 `false`，确认游戏 ID、版本、窗口名和图标。
2. 从发行目录启动，验证入口、选择、输入、历史、弹窗和侧栏。
3. 验证图片、主题、语言切换与音频，尤其是从开发目录改为打包资源后的行为。
4. 写入并导入独立测试槽位，确认目录可写、恢复后的画面和随机结果正确。
5. 移动整个目录后再次启动，确认没有依赖开发目录中的文件。

`.msi`、`.dmg`、AppImage 等系统安装包属于后续外包装；仓库当前交付边界及移动端状态见
[项目状态](../development/status.md)。引擎维护者发布跨平台产物时使用[版本与发布](../development/release.md)。

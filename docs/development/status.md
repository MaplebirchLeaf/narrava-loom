# 项目状态

> 基线版本：0.9.0
>
> 更新日期：2026-09-13

本页记录完成度与实际缺口。作者接口见[API 与语法速查](../reference/api-and-syntax.md)，
实现边界见[总体架构](../architecture/overview.md)，版本历史见 [CHANGELOG](../../CHANGELOG.md)。

## 已实现

- Source → Twee → HIR → MIR → LIR → 拥有型 Bytecode → VM 编译执行链。
- Engine、State、Story、Macro、Event/Reaction 的事务、挂起恢复、取消与回滚。
- I18n、Save、Resource、Script Bundle 与 TypeScript 作者契约。
- 共享随机序列已接通 Twee / Script、事务、历史和 schema 4 存档；兼容读取 v2/v3。
- 有界统一 Logger、带来源的诊断与 Tauri/TUI 只读状态检查。
- [Location](../author/location.md) 地点与位置状态，已接入事务、历史及存档。
- 零 Core 依赖的 Protocol DTO、契约生成与 Host 直接驱动的 RuntimeSession。
- Tauri Worker 与 WebView Renderer，以及可操作的 TUI；支持输入、历史、存读档、语言切换和侧栏。
- 原生 image、meter、dialog/page；Audio 声明、tag 匹配、连续播放与自动停止。
- 无 Rust 的 [小镇示例](../../examples/README.md)、完整能力手册、Twee VS Code 扩展、可移动桌面发行目录与构建流水线。

## 限制与待验收

| 范围         | 当前边界                                                             |
| ------------ | -------------------------------------------------------------------- |
| 桌面发行目录 | 待真实窗口验收图片、弹窗、输入、I18n、Save、缩放与键盘操作           |
| Audio        | 离线回归已覆盖生命周期与播放效果；待真实音频设备验收                 |
| TUI          | 存档／语言的独立菜单尚未提供，可使用快捷操作                         |
| Location     | 图形地图、寻路、楼层、种子生成、通行图和空间索引未实现               |
| 宿主目标     | Tauri 尚未达到完整 HTML5 图文能力；Godot 像素沙盒与 2D/3D 宿主未实现 |
| 脚本调试     | Tauri 对象控制台、补全、受管异步与日志，TUI 只读检查；TS source map、断点和自动源码跳转尚未提供        |
| 移动端       | Android/iOS 平台工程、签名、打包与真机验收未接通                     |
| 后续能力     | Model、模组加载、文件选择器、云存档与通用存档迁移未实现              |

自动测试、浏览器验收与真实窗口/设备验收分别记录，不能互相代替；验证命令见[仓库命令](commands.md)。

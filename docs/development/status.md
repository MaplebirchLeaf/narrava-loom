# 项目状态

基线：`0.10.0`；核对日期：2026-09-13。版本历史见 [CHANGELOG](../../CHANGELOG.md)。
本页区分当前能力、已知边界和待验收项，参数与使用规则见[契约参考](../reference/README.md)。

## 可用范围

| 领域 | 已接通的执行路径 |
| --- | --- |
| 编译 | Source → Twee → HIR → MIR → LIR → 拥有型 Bytecode → VM |
| 运行 | Engine/State/Story 事务、历史、Macro、Event/Reaction、挂起恢复和取消 |
| 随机 | Engine 根种子统一驱动 Twee 与 Math.random，进度进入回滚、历史和 schema 5 存档 |
| 内容 | I18n、Save、Resource、ScriptBundle、TS 作者契约、Location 坐标与位置 |
| 宿主 | Tauri Worker/WebView 与 TUI；输入、侧栏、历史、存读档和语言切换 |
| 呈现 | image、meter、dialog/page；Audio 声明、匹配、续播和停止 |
| 工具 | 有界日志、Tauri 脚本控制台与补全、TUI 只读检查、Twee 编辑器扩展 |
| 交付 | 可运行小镇示例、能力手册、可移动桌面目录及跨平台构建流水线 |

## 已知边界

| 领域 | 限制 |
| --- | --- |
| Twee | return 尚不可执行；slot/replace 正文不支持动态 print、输入、按钮或脚本 Macro |
| Script Macro | 可依赖标量/Surface 与受管异步路径；容器正文、Expression 参数和完整 Hook 路径仍有限制 |
| I18n | 动态脚本输出和导航标签没有自动编译期文本身份 |
| Save | 拒绝实验格式 1–4；要求游戏身份精确匹配，无通用迁移、云存档或文件选择器 |
| Dialog | 单活动弹窗；无导航动作正文不支持 include/goto |
| Location | 无图形地图、寻路、楼层、生成器、通行图或空间索引 |
| 调试 | 无 TS source map、断点、自动源码跳转；TUI 不执行控制台脚本 |
| TUI | 无独立存档/语言菜单，可使用快捷操作 |
| 平台与扩展 | Godot Host、Android/iOS 平台工程及移动打包、Model、模组加载未实现 |

## 待实际验收

桌面发行目录仍需真实窗口检查图片、弹窗、输入、语言、存档、缩放和键盘操作。
Audio 离线回归已覆盖生命周期与效果，真实设备播放需单独验收。
共享响应式 CSS 和移动入口不代表 Android/iOS 已完成交付。

验收流程见[测试指南](testing.md)，自动测试、WebView 检查和真实设备结果分别记录。

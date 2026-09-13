# 游戏作者手册

游戏项目由配置、故事和可选脚本、资源组成。开发版从仓库启动，游戏内容不需要修改 Rust。

## 从零开始

1. [快速入门](quick-start.md)：安装依赖，运行示例，创建两页故事。
2. [编写 Twee](writing-twee.md)：选择、变量、条件、循环和内容复用。
3. [游戏流程](flow.md)：初始化、导航、历史、根种子和存读档。
4. [打包游戏](publishing.md)：生成可移动目录并检查发行结果。

## 按需增加能力

| 需求 | 阅读 |
| --- | --- |
| 复杂计算、自定义 Macro | [脚本](scripting.md) |
| 图片、数据文件、主题 CSS | [资源与主题](resources.md) |
| 任务事件、阈值触发、入场规则 | [事件与 Reaction](events.md) |
| 地点、坐标和移动 | [Location](location.md) |
| 单页与多页弹窗 | [Dialog](dialog.md) |
| 背景音、环境音与音效 | [Audio](audio.md) |
| 翻译和切换语言 | [I18n](i18n.md) |
| 日志、控制台和错误排查 | [调试](debugging.md) |

写代码时用[契约参考](../reference/README.md)查精确参数。
内部数据结构和宿主实现放在[架构](../architecture/README.md)，不属于作者的前置知识。

# 游戏作者手册

这是没有 Rust 经验也能从零开始的入口。不必一次读完：按顺序完成前两册，然后遇到什么再查什么。

## 怎么读

- **完全没经验**：按下面分册顺序从头做到尾；
- **已经成功打开 `examples/`**：直接从 `config.toml` 与第一个 Passage 开始；
- **只想查"现在能写什么"**：直接打开 [作者 API 与语法速查](../reference/api-and-syntax.md)，
  那里集中列出了所有当前内置 Macro、Expression 函数/方法、操作符、Worker ECMAScript API 和基础事件。

## 分册

1. [从安装到第一次运行](getting-started.md)：安装、启动、创建目录和配置；
2. [`config.toml` 与第一个 Passage](configuration.md)：配置与第一个能运行的故事；
3. [Twee、选择、变量、条件和循环](writing-twee.md)：Passage、导航、变量、条件、循环与 Macro；
4. [TypeScript/JavaScript、Macro 与 Resource](scripting-and-resources.md)：TS/JS、State、自定义 Macro、Surface、Resource 与 CSS；
5. [Story、Engine、Event、I18n 与 Save](runtime-and-save.md)：重新开始、Logger/Event、多语言与存档；
6. [Reaction](reaction.md)：Event、State 与 lifecycle 的声明式叙事反应；
7. [运行、构建、自检与故障排查](build-and-troubleshooting.md)：三个命令的区别、自检与逐项排错；
8. [继续查阅](further-reading.md)：架构与参考文档的入口。

精确的宏、内置函数、运算符和公开脚本 API 不在教程里重复维护，统一见
[作者 API 与语法速查](../reference/api-and-syntax.md)。

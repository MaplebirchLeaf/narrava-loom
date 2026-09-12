# 游戏作者手册

这是没有 Rust 经验也能从零开始的入口。不必一次读完：按顺序完成前两册，然后遇到什么再查什么。

## 怎么读

- **完全没经验**：按下面分册顺序从头做到尾；
- **已经成功打开 `examples/`**：直接从 `config.toml` 与第一个 Passage 开始；
- **只想查“现在能写什么”**：直接打开 [API 与语法速查](../reference/api-and-syntax.md)。

## 分册

1. [从安装到第一次运行](getting-started.md)：安装、启动、创建目录和配置；
2. [`config.toml` 与第一个 Passage](configuration.md)：配置与第一个能运行的故事；
3. [Twee、选择、变量、条件和循环](writing-twee.md)：Passage、导航、变量、条件、循环与 Macro；
4. [TypeScript/JavaScript、Macro 与 Resource](scripting-and-resources.md)：TS/JS、State、自定义 Macro、Surface、Resource 与 CSS；
5. [Event](event.md)：作者事件、拉取订阅、Engine Passage 事件与 Reaction 事件链；
6. [Story、Engine、Logger 与 I18n](runtime-and-i18n.md)：重新开始、诊断与多语言；
7. [Save](save.md)：保存范围、读写存档和处理结果；
8. [Macro](macro.md)：原生宏、点击正文与内容复用；
9. [Reaction](reaction.md)：Event、State 与 lifecycle 的声明式叙事反应；
10. [弹窗与页面](dialog.md)：无导航打开、默认页与关闭；
11. [Audio](audio.md)：背景音、tag 匹配与播放生命周期；
12. [Location](location.md)：地点、坐标、移动与位置恢复；
13. [随机数与只读调试](random-and-debugging.md)：随机种子、回放与游戏内状态检查；
14. [运行、构建、自检与故障排查](build-and-troubleshooting.md)：三个命令的区别、自检与逐项排错。

架构、Host 和仓库开发文档见[文档总入口](../README.md)。

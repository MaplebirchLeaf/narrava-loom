# Audio 生命周期与 Host effect

> 本轮实现；取代显式停止宏和可见播放器提案。作者入口见 [Audio](../author/audio.md)。

Audio 是声明驱动的背景音能力，不属于 SemanticNode 或 Surface。
原生宏仅有 resource、可选 channel、可选 tag 三个位置参数；复杂配置由 Script Audio 接收。
资源前缀在作者入口归一化，Protocol 仍携带完整 Resource 逻辑路径。

## 数据流

Twee audio / Script Audio → Bootstrap 作用域声明 → Runtime 命令成功后计算差异 →
Protocol AudioEffect(play/stop) → TUI 命令循环 / Tauri Worker → rodio 本地播放。

Runtime Protocol 版本升级为 2，以明确区分新增 Audio 结果的边界，旧客户端不能按版本 1 接收新结果。
Protocol 仅有 resource、channel、loop、volume，以及 stop 的 channel。
tag 匹配、作用域与回滚留在 Runtime；Host 不读取 Passage tag，不实现规则引擎。

## 生命周期

Bootstrap 保存全局、公共区域和正文声明；Passage Init 清除上一页的局部声明，保留脚本装载时
建立的全局规则。现有 Header/Footer/Bar/BarStowed 在隔离 State/Story 视图中执行，并按区域
收集声明；无任何新特殊 Passage。正文同 channel 声明优先于公共区域。

Audio 有自己的轻量声明检查点，随 Runtime 命令跨 Pending 保留。最终成功才计算有效声明与
上次已交付状态的差异；错误 operation ID 不影响活动事务，取消和失败恢复原声明。
不把声音或规则放入 State、Story、Reaction、Save，也不把 play 当成可重复渲染节点。

相同资源与 loop 不重建播放器，volume 修改直接更新。不同音频在同 channel 替换；退出有效
声明集合的 channel 停止。Host 失败报告为提示，不能倒退已提交的故事事务。

## Host 实现与验证

两个现有 Host crate 通过普通 Rust 模块复用 `hosts/audio.rs`，该模块实际拥有 rodio 设备及
每 channel 的播放器。没有新增工作区 crate、Manager、Service 或 trait。Tauri 不经过 WebView。
字节来自 ResourceCatalog，发行包资源无需解包为文件；设备首次 play 时打开，退出时释放。
Linux 构建需要 `libasound2-dev`，CI/release 工作流已同步。
播放 API 依据 [rodio 0.22.2](https://docs.rs/rodio/0.22.2/rodio/)。

验收覆盖位置参数、tag、Header/Footer、相邻页面连续播放、正文覆盖、离开自动停止、历史回退、
取消与失败、Protocol 往返、离线解码/循环/停止和资源错误。离线测试不等同于真实扬声器验收。

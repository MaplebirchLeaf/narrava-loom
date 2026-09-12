# 弹窗执行与 Host 边界

作者语法见[弹窗与页面](../author/dialog.md)。

## 数据流

`dialog/page` → MIR BeginDialog/BeginDialogPage/EndDialog → Bytecode VM →
Semantic Dialog（initial、pages）→ Protocol Dialog → TUI/Tauri。

页面只有 title 与已有语义正文；Core 不持有 DOM、终端几何或视觉属性。
VM 在原执行帧内暂存页面输出，异步 Macro 暂停时一起保留；EndDialog 校验默认页后一次提交。
Bytecode 格式为 2；不兼容版本的编译产物需重新编译。

无导航 link 使用原有 MacroInteractions、捕获域、BytecodeMacroBody 和 MacroSuspension。
RuntimeSession 的命令检查点负责失败与取消回滚；动作正文不创建 Story Entry，不触发 Passage
生命周期。导航 link 保留原有导航入口。无导航正文成功后合并当前输出；新 Dialog 替换旧 Dialog，
已不可见的动作 ID 从表中释放。无导航 Macro Body 仍不支持 include/goto。

## Host 状态

每次打开分配新的节点 key，同一弹窗重绘沿用该 key。Tauri 保留选中页与关闭状态，
按 title 复用页内容容器；TUI 用独立页索引管理纯文字页，按钮焦点不决定当前页面。
关闭和切页不发送 Runtime 命令；页面只由显式 page 子句划分。

## 验证边界

回归覆盖四页、单页、非首页默认页、图片/meter/脚本正文、重复打开、异步恢复和取消、
失败回滚，以及 Host 页选择与重绘。浏览器渲染验收与真实 Tauri 窗口、音频设备验收分别记录，
不以 DTO 测试代替真实设备结论。

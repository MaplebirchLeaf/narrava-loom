# Protocol 与宿主

Protocol 是零 Core 依赖的纯数据 crate。Script Runtime 在 Core SemanticOutput 与
拥有型 Host DTO 之间转换；Host 只显示结果、回传输入和完成平台操作。

## 数据边界

| 内容 | 负责方 |
| --- | --- |
| 游戏状态、控制流、交互 receiver 和允许值 | Core / RuntimeSession |
| RuntimeCommand、RuntimeUpdate、节点与身份 | Protocol |
| UTF-16 Value 与 UTF-8 DTO 转换 | Script protocol_adapter |
| DOM、CSS、终端几何、焦点、播放器 | 对应 Host |

TextValue 到 DTO 的转换拒绝无法表示的孤立代理项，不向 IPC 泄漏非法文本。
输入只引用 Runtime 提供的 Interaction ID；过期身份、非法值和错误 operation ID 都在边界校验。
Host 不保存第二份 State / Story，也不从显示标签推导执行目标。

## 语义呈现

节点包括文本、硬换行、样式文本、图片、区域、容器、替换、组件、输入、导航和弹窗。
稳定 key 让 Renderer 复用节点；同一 key 替换更新内容，保留容器的表现意图。

- Container 的 plain / panel 表示透明分组或可感知的独立内容组。
- flow 的 stack / row 是必需协议字段，描述同级上下或同行排列，不传 CSS 尺寸和间距。
- Component 携带 capability、version、纯数据 properties 与必需 fallback，未知能力显示 fallback。
- 文本样式与颜色按[文本选项](../reference/macros.md#print-文本选项)映射；delay 到时前不可见。
- heading 只表达标题层级，不能推断弹窗页面。

固定 Region 为 main/header/footer/bar/bar-stowed/dialog。自定义非空 Region 仍有效；
Tauri 回退正文，TUI 保留在自定义区域表，不静默丢弃。
没有作者导航的普通 Passage 可由 Engine 追加 SafeReturn；公共区域不生成该兜底。

## Tauri

专用 Worker 持有编译产物与 RuntimeSession，逐步串行执行命令。managed state 中的 facade
只保存命令发送端和平台资源；异步 command 等待 Worker，timer 和 blocking 文件 IO 在外围完成。
等待结束后通过 Resume 交还拥有型结果，不从 Host 直接恢复 VM。

WebView 仅渲染 DTO。游戏脚本由 Worker 中的 Boa 执行，Oxc 去除 TypeScript 类型。
Renderer 按 key 协调 DOM，区域映射到稳定插槽；侧栏内容来自 Bar / BarStowed，
窗口标题来自游戏配置。作者 CSS 按路径排序、在默认主题之后加载。

资源自定义协议按已验证逻辑路径读取字节，WebView 启动只取得元数据，不把全部资源塞入 JSON。
`narrava-resource://localhost/` 的 localhost 是虚拟协议 host，不是网络服务。
CSS `resource(...)` 与开发/发行资源使用同一入口，作者规则见[资源与主题](../author/resources.md)。

## TUI

真实终端使用 Crossterm + Ratatui alternate screen；重定向输入/输出时退回普通文本循环。
Renderer 保存 TuiFrame，平台层装载游戏、完成 Pending、语言包与存档 IO。
它不依赖 Tauri，终端焦点和尺寸不进入 Core。

Image 用 alt 文本方框表示，meter 用字符条，未知组件显示 fallback。
延迟文本保存在 frame.delayed，由 render_at 到点显示。
键位与双 Host 验收见[测试指南](../development/testing.md)。

## 弹窗

```text
dialog/page → BeginDialog / BeginDialogPage / EndDialog → VM → Semantic Dialog → Protocol → Host
```

VM 在同一执行帧暂存各页输出，跨异步暂停保留，EndDialog 校验默认页后整体提交。
页面只携带 title 与语义正文。无导航 link 使用现有 MacroInteractions、捕获域和
BytecodeMacroBody，成功后合并输出，不建立历史或 Passage 生命周期。

新 Dialog 替换旧 Dialog 并释放不可见动作 ID；无导航动作正文不支持 include/goto。
Host 用节点 key 和 title 保留同一弹窗选中页，重开得到新 key；TUI 页索引独立于按钮焦点。
切页和关闭不发送 Runtime 命令，正文只在打开时执行。

## 音频

```text
Twee / Script 声明 → Runtime 成功结算并计算差异 → AudioEffect → Host 本地播放器
```

Audio 不属于 Surface 或 Save。Bootstrap 保存全局与区域声明，Runtime 持有轻量检查点，
Pending 暂存，失败取消回滚，最终成功才交付 play/stop 差异。
Tag 匹配与作用域优先级留在 Runtime，Host 不重复解释 Passage 规则。

两个 Host 复用 [hosts/audio.rs](../../hosts/audio.rs)，从 ResourceCatalog 读取字节，
首次播放时打开设备；相同资源和 loop 不重建播放器，仅 volume 变化时更新音量。
缺失声明停止对应 channel，会话退出释放设备。播放错误作为 Host 提示，不倒退已提交故事事务。

## 存档与调试

两端通过 [hosts/save_io.rs](../../hosts/save_io.rs) 读写命名槽位，统一目标名验证、16 MiB 导入
限制和临时文件覆盖，保留 Host 错误来源。存档内容与恢复事务属于[Save](save-format.md)。
Tauri 的 F10 控制台和 F12 DevTools 由 developer 同时在界面和 Rust 入口控制；TUI 提供只读检查。

自动协议测试、WebView 测试、真实窗口和设备验收分别验证不同边界。
移动工程与其他 Host 的实际进度只在[项目状态](../development/status.md)维护。

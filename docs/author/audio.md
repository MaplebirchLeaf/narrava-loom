# 背景音与环境音

Audio 声明当前 Passage 需要的音频，不产生可见节点。TUI 与 Tauri 都由本地音频后端播放。

```twee
<<audio "forest.ogg">>
<<audio "forest.ogg" "ambience">>
<<audio "forest.ogg" "ambience" "forest">>
```

三个位置参数依次为资源、channel、tag。channel 默认 `bgm`；省略 tag 时不筛选。
宏默认循环、音量为 1，不接收选项对象，也没有停止宏。

资源路径相对于 `resources/audio/`，例如 `forest/rain.ogg`。
保留 `audio/` 根前缀也会归一到同一资源，不会重复添加；不能使用绝对路径或 URL。

## 按 Passage tag 选择背景音

```twee
:: Header
<<audio "forest.ogg" "ambience" "forest">>
<<audio "town.ogg" "ambience" "town">>

:: ForestPath [forest]
林间小径。

:: ForestLake [forest]
林中的湖泊。

:: Town [town]
城镇广场。
```

Header/Footer 是已有固定 Passage，每次正文呈现时参与声明，tag 匹配的是当前主 Passage，
不是 Header/Footer 自身。示例中两处森林页面之间音频连续播放；进入城镇切换；进入没有匹配
声明的页面自动停止。不要为音频创建新的特殊 Passage。

正文里的声明随当前 Passage 生效，优先于公共区域的同 channel 声明。
相同 channel、相同资源和 loop 连续存在时不会重新播放；仅 volume 变化时调整音量。
不同 channel 可同时播放。多个规则匹配同 channel 时，优先级从低到高为 Script 全局声明、
Header、Footer、Bar、BarStowed、正文；同一作用域后执行且匹配的声明优先。

## Script 控制复杂配置

在启动脚本顶层登记全局规则，每次进入页面按 tag 重新匹配：

```typescript
Audio.play("forest.ogg", {
  channel: "ambience",
  tags: ["forest", "woods"],
  loop: true,
  volume: 0.5,
})
```

需要在动作中停止当前 channel 时，在相应 Macro 或作者函数内调用：

```typescript
Audio.stop("ambience")
```

`Audio.play(resource, options?)` 的选项为 channel、tags、loop、volume。
channel 为普通非空字符串，默认 `bgm`；tags 是字符串数组，任意一个匹配即可，空数组表示不筛选。
loop 默认 true；volume 默认 1，必须是 0..1 的有限数值。
脚本可以把一次性音效设为 `loop: false`；已自然播放结束的相同声明不会因重绘自动重播。

脚本装载期间的声明是全局规则；正文或固定区域执行期间的调用属于相应作用域。
`Audio.stop(channel)` 在当前作用域抑制该 channel 的低优先级声明；下一次 Passage 执行将重新求值，
它不删除源代码中的全局规则。同一次执行先 play 再 stop，只提交最终所需状态，不播放中间状态。

Runtime 成功后计算需要 play/stop 的差异，Pending 暂存，失败和取消恢复之前的声明。
Save 不保存设备、播放器或时间轴。历史回退、读档、语言重绘通过实际正文重新匹配声明；
仍需要相同音频就保持播放，不凭前端重绘重复发送命令。会话退出时停止全部音频。

资源缺失、格式损坏或无音频设备由 Host 报告，不回滚已经成功的故事操作。
示例 `AudioGallery` → `AudioForest` → `Hall` 演示连续播放与自动停止。

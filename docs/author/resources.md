# 资源与主题

资源逻辑路径相对于 `resources/`。`Resource` 与 CSS 使用完整逻辑路径，原生图片和音频入口
分别补齐 `img/`、`audio/` 前缀。

| 使用位置 | 参数示例 | 对应文件 |
| --- | --- | --- |
| `<<image "forest.png" "森林">>` | `forest.png` 或 `img/forest.png` | `resources/img/forest.png` |
| `Audio.play("rain.ogg")` | `rain.ogg` 或 `audio/rain.ogg` | `resources/audio/rain.ogg` |
| `Resource.read("img/forest.png")` | `img/forest.png` | `resources/img/forest.png` |
| CSS `resource("img/forest.png")` | `img/forest.png` | `resources/img/forest.png` |

## Resource 资源

所有资源放进 `resources/`。例如：

```text
my-game/resources/data/guide.txt
my-game/resources/img/forest.png
```

脚本中使用的逻辑路径不带 `resources/`：

```ts
Resource.has("data/guide.txt")
Resource.paths()
Resource.info("img/forest.png")
Resource.read("img/forest.png")
Resource.text("data/guide.txt")
Resource.pick(["data/guide.zh-CN.txt", "data/guide.txt"])
```

- `paths()` 返回稳定排序的全部路径；
- `info()` 返回路径、媒体类型和字节大小；
- `read()` 返回 `Uint8Array`；
- `text()` 只适合 UTF-8 文本；
- 开发目录只在首次 `read()`/`text()` 时读取对应文件，成功内容会缓存；读取失败或文本不是
  UTF-8 时会抛出错误，不会伪装成 `undefined`；
- `pick()` 返回候选列表中第一个存在的路径；
- 未知扩展名仍可作为二进制资源；
- 路径拒绝绝对路径、空段、`.`、`..` 和反斜杠。

## CSS 和 `resource("path")`

CSS 完全可选。没有 `styles/` 时，Tauri Host 使用内置的完整默认样式。

要覆盖外观，创建 `styles/game.css`：

```css
nv-story {
  --narrava-background: #16130f;
  --narrava-text: #f2e8d5;
  --narrava-accent: #d6a85f;
  --narrava-accent-hover: #f0c878;
}

nv-passage {
  max-width: 48em;
}
```

使用游戏资源作为背景：

```css
nv-story {
  background-image: resource("img/forest.png");
  background-size: cover;
}
```

Host 会把 `resource("...")` 转为受 CSP 允许的 `narrava-resource://localhost/...` URL，按需
读取对应资源。这里的 `localhost` 只是 Tauri 自定义协议的虚拟 host：不会连接网络、没有端口，
也不是仅开发模式有效；正式安装包使用同一机制。推荐只依赖 `nv-story`、
`nv-passage`、`.passage-header`、`.passage-main`、`.passage-footer`、`nv-ui-bar`、`#nv-dialog`
和 `--narrava-*` 变量；更深的内部节点不保证长期兼容。

CSS 只能改变外观，不能执行游戏脚本、访问 Rust 或替换 Renderer。

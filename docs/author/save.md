# Save：保存与恢复游戏进度

本页面向游戏作者。二进制格式、值图和恢复事务属于 [Save 内部设计](../architecture/save-format.md)。

## 保存哪些内容

进入存档：`$` 变量、Story 历史以及 Core 规定的数据。
不进入存档：`_` 临时变量、`@` 局部变量、脚本函数、DOM、Blob URL、平台对象。

Core 已实现存档捕获、校验和恢复模型。声明文件包含：

```ts
const snapshot = Save.capture()
Save.restore(snapshot)
Save.export()
Save.import()
```

Save 生命周期 Hook：

```ts
const beforeExport = Save.before("export", ({ target }) => {
  Logger.info("save", `准备导出到 ${target}`)
  return target === "quick" ? "quick-backup" : undefined
})

const afterExport = Save.after("export", completion => {
  if (completion.succeeded) Logger.info("save", "导出完成")
  else Logger.error("save", completion.error ?? "导出失败")
})

Save.off(beforeExport)
Save.off(afterExport)
```

支持 `capture/restore/export/import` 四种 operation。before 按登记顺序执行；export/import 的
before 可以返回新字符串改写 Host target。after 只在操作取得真实完成结果后执行，不能把失败
改成成功。capture/restore 在 Worker 内同步完成；export/import 由 Tauri Host 写入或读取游戏
目录中的 `save/<target>.nsave`，磁盘操作完成后才触发 after。target 只允许 1 至 80 个 ASCII
字母、数字、`-` 或 `_`，所以不能借此访问 `save/` 外的文件。

详细边界见 [Save 格式与恢复事务](/docs/architecture/save-format.md)。

## 使用边界

存档要求游戏 ID 和版本精确匹配。恢复其他游戏或其他版本的存档会失败，不会隐式迁移。
`Save.export()` 与 `Save.import()` 的文件读写由 Host 完成；要在 `Save.after` 中判断真实结果。
不要把尚未完成的保存请求当作已写入磁盘。

Reaction 的启用、次数与销毁状态会随存档恢复；脚本定义本身由游戏启动时重新注册。

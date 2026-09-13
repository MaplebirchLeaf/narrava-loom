# 契约参考

按执行环境查阅。教程说明怎样完成任务，参考页集中说明可接受的写法、参数和边界。

| 参考 | 适用位置 |
| --- | --- |
| [项目配置](configuration.md) | `config.toml`、游戏目录和窗口设置 |
| [Twee Macro](macros.md) | `<<...>>` 动作、控制流、交互与文本选项 |
| [Twee Expression](expressions.md) | Macro 参数中的值、运算符、函数和方法 |
| [脚本 API](script-api.md) | `contents/**/*.ts` 与 `*.js` 中的全局对象 |

完整脚本签名及中英说明由[作者类型声明](../../bindings/typescript/narrava.d.ts)维护；
跨 Host 数据结构由[生成的 DTO 声明](../../bindings/typescript/narrava-contract.generated.d.ts)提供。
类型声明、编辑器和控制台帮助共用契约来源，文档不另建一套签名。

Twee Expression 与 Worker JavaScript 是不同执行环境。Twee 不提供浏览器对象，
游戏脚本也没有 DOM、fetch 或任意 Tauri 调用。开始使用见[作者手册](../author/README.md)。

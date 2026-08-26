# Narrava Mod 与有效构建

> 状态：I18n 前置已完成，Mod 功能待继续
>
> 更新日期：2026-08-22

## 职责

`ModLoader` 管理候选内容的验证、显式顺序和有效构建切换。它不解析 Twee、不执行 Macro，也不负责 ZIP 或平台文件选择。

有效构建必须遵循：

```text
基础内容建立 I18nCatalog
→ 应用当前 .nlang，得到本地化候选内容
→ 按作者／玩家明确顺序应用 .nmod 修改
→ 完整验证 Story、Resource、模组结果与依赖
→ 一次性提交新有效构建
```

任一步失败都保留上一份完整有效构建，不允许只切换其中一个领域。

## 当前实现

- `GameIdentity` 与 `GameCompatibility` 统一 Config、`.nlang`、未来 `.nmod` 和 Save 的游戏身份规则；
- `NlangPackageInput` 完成语言包的内存文件、路径和包级校验；
- `ModLoader::activate_language()` 将候选译文绑定当前 `I18nCatalog`，成功后才替换活动语言；
- 失败会返回 `ModLoaderError` 与未丢失的候选包，上一份 `ModLanguageBuild` 保持不变；
- `deactivate_language()` 显式回到游戏默认语言，不自动猜测其他 fallback 包；
- `NmodManifest` 严格解析 `mod.toml` 的身份、版本、目标游戏和依赖声明；
- `NmodPackageInput` 接收 Binding 解包后的路径与字节，不依赖 ZIP 库或平台文件系统；
- 包级校验拒绝不安全路径、越界目录、重复文件、缺失或非 UTF-8 清单，以及不兼容的目标游戏；
- `NmodValidatedPackage` 只在全部边界校验成功后产生，并保留经过校验的文件；
- `NmodSelection` 保留作者或玩家提交的顺序，并验证重复 ID、缺失依赖、依赖版本和前后顺序；
- `ModBuildInput` 把默认语言或已绑定当前 `I18nCatalog` 的目标语言与 `NmodSelection` 合并为下一阶段输入。

当前 `ModLanguageBuild` 只是完整有效构建的第一个切片。它不能被描述为 Story、Mod、Resource 已经整体切换。

`ModBuildInput` 也不是有效运行构建：它只证明语言与模组选择各自有效，尚未读取或应用 `patches/`，更没有生成最终 Story、Resource 或 MIR。该类型存在的目的，是让后续管线不能误用未经验证的语言包或任意模组数组。

## 顺序

内嵌模组顺序来自 `config.toml` 的 `mods.embedded` 数组。玩家模组默认禁用，启用后的顺序由游戏内管理界面明确提交。引擎验证这个顺序，但不根据文件名、目录枚举或 `priority` 自动重排。

语言包每个 locale 只有一个活动基础包。模组随后修改已经本地化的候选内容，不把多个无身份 `.nlang` 文件当作隐式覆盖层。

## `.nmod` 清单

`.nmod` 与只承载一种语言的 `.nlang` 不共用 manifest。当前确定的最小 `mod.toml`：

```toml
id = "forest.expansion"
name = "Forest Expansion"
version = "1.2.0"

[game]
id = "example.forest"
versions = ">=0.2.0, <0.3.0"

[[dependencies]]
id = "shared.library"
versions = "^2.0"
```

- `id` 区分大小写，并作为稳定模组身份；
- `version`、`game.versions` 和依赖版本使用 SemVer；
- 清单拒绝未知字段、自依赖和重复依赖；
- 清单当前不携带无实际用途的 `format_version`；
- `dependencies` 保留作者声明顺序，但不替代 `config.toml` 或玩家提交的全局模组顺序；
- `priority` 不属于清单，引擎不会按隐式优先级重排。

预定包内职责保持独立：

```text
mod.toml    模组身份、兼容性和依赖
contents/   新增的逻辑内容
patches/    对 I18n 后候选内容的修改声明
resources/  .nres 逻辑资源包
```

当前目录白名单为：

- 根目录只允许 `mod.toml`；
- `contents/` 接收新增逻辑内容；
- `patches/` 接收后续定义的修改声明；
- `resources/` 只接收 `.nres`。

`contents/` 与 `patches/` 当前只固定安全路径和所属职责，具体文件后缀将在对应编译及 patch schema 确定后收紧。Core 当前接收已经解包的内存文件，不负责 ZIP 读取。

## Patch 状态

Patch schema 暂未固定。此前试验性的文本 patch 依赖 Core 内部 I18n 身份，已在 crate 解耦时删除，避免为了尚未完成的 Mod 功能扩大 Core API。

完成本体与 I18n 后，再从独立 `narrava-loom-modloader` crate 设计 patch。届时仍须遵守：实际修改发生在 I18n 之后，不向 Core 注入 ModLoader 类型，也不依赖具体 Host 或 Renderer。

## I18n 后置模组修改

`.nmod` 的修改时间点固定在 I18n 修正基础内容之后。提前读取模组清单和验证依赖不等于提前应用修改；实际 Twee、Script、结构化文本和 Resource 变更都面向当前语言的候选内容。

因此 `.nmod` 不参与基础 `translations.nmsg` 与 `dictionary.json` 的生成，也不能改变“先选择语言、后应用模组”的顺序。需要支持多语言的模组必须明确携带自己的 locale 内容或 locale 相关补丁，不能依赖 Renderer 或运行时字符串搜索猜测语言。

## 下一步

> I18n 前置条件已完成：ModLoader 可以继续定义 I18n 后的最小 patch 契约。

1. 重新定义最小 patch 契约；
2. 将 Story 与 Resource 结果组合成一个可原子提交的有效构建；
3. 让 `ModLoader` 只在完整候选成功后原子替换活动构建；
4. 最后再实现 Binding 的 `.nmod`／`.nlang` ZIP 读取和持久化选择。

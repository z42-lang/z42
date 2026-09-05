# Design: 应用自定义配置属性

> 状态：🔴 DRAFT | 提案 [proposal.md](proposal.md) | spec [specs/app-properties/spec.md](specs/app-properties/spec.md)

## Decisions

### Decision 1：分表，不用前缀约定

VM 旋钮与用户属性必须**结构上**可区分。若同住一张表靠名字区分，`gc-mdoe`（typo）就会被
当成一个合法的用户属性静默收下——而「未知旋钮就明确报出来」正是
`complete-runtime-settings` 的核心承诺之一。

dotnet 走前缀（`configProperties` 里 `System.*` 归 runtime、其余归 app）之所以可行，是因为
它的 runtime 旋钮**天然带命名空间前缀**。z42 的旋钮 key 是 `gc-mode` / `libs` / `log`——
要走前缀路线得给所有现有 key 改名，代价远大于分一张表。

### Decision 2：属性用**完整 TOML 类型**，靠"交出原文 + `Std.Toml`"实现

User 裁决②要求完整类型、以后不用扩展。三条路：

| 方案 | 代价 |
|---|---|
| VM 暴露结构化 marshal（数组 / 表对象过 ABI）| 要发明一套值模型 + 生命周期，最贵，且仍要为将来的新 TOML 类型扩展 |
| VM 暴露路径访问 `Get("limits.max-retries")` | 发明一套 mini-language；数组仍无法干净返回 |
| **VM 交出 `[properties]` 的 TOML 原文，脚本用 `Std.Toml` 解析** ✅ | 一个返回 `string` 的 builtin；**TOML 有什么就支持什么**，永不需要扩展 |

z42 stdlib 已有完整 TOML 解析器（`z42.toml`），这条路把"完整类型"变成**零新增 ABI**。

**同时给标量便捷入口**：`AppProperties.Get(key)` 由 VM 侧做顶层标量查表（它手里就有解析好的
`toml::Table`），覆盖 90% 的场景且**不需要 app 依赖 `z42.toml`**。结构化情形才走 `Raw()`。
简单的事简单，复杂的事可能——两者都不牺牲。

### Decision 3：属性独立于 `RuntimeConfig`，不是它的一部分

两者的**保证**完全不同：

| | 旋钮 | 属性 |
|---|---|---|
| 登记表 | 有（`KNOWN_KNOBS`）| 无 |
| 类型 / 取值域 | 声明并校验 | 不校验 |
| 可用性（build / feature / 平台）| 有 | 无 |
| 分层 | 五层 | 只有 app 侧车 |
| 未知 key | 诊断 | 就是正常情形 |

混进 `RuntimeConfig.Get` 会让"返回 null"同时意味着"取默认值"和"这个旋钮压根不存在"，
调用方无法区分。故独立类 `Std.Runtime.AppProperties`。

### Decision 4：基表 + per-profile 逐 key 覆盖，**浅覆盖**

```toml
[properties]                  # 基表
api-endpoint = "https://prod"
limits = { max-retries = 3 }

[profile.debug.properties]    # 覆盖
api-endpoint = "http://localhost:8080"
```

"dev 用本地端点、prod 用线上端点"正是这类配置最典型的用法，所以 per-profile 必须有；
但一个 app 常有 5 个属性只 1 个随环境变——只给 per-profile 就要重复 4 个，故要基表。

**浅覆盖**（profile 里出现的顶层 key 整体替换基表的值，不深合并子表）：深合并要解释
"子表怎么合、数组怎么合"，规则一多就没人记得住；浅覆盖一句话讲完，且与本系统别处
「逐 key 叠加」的心智一致。

### Decision 5：用户配置里的 `[properties]` 警告而非静默忽略

按裁决①属性不分层，所以 `Z42_CONFIG` 里的 `[properties]` 不生效。但**静默**忽略会让人
debug 半天——给一行 warn 说明它归 app 所有。与 `.json` 配置文件给迁移提示同一原则：
读不懂的东西要说出来，不要假装没看见。

### Decision 6：生成器改用 `TomlValue.Stringify`，顺带换掉手拼

现有 `RuntimeConfigSidecar.z42` 自己拼 `key = "value"`（含一个 `_tomlScalar` 判断要不要
加引号）。属性支持完整类型后手拼不可行——数组 / 嵌套表 / 转义都要自己处理。
改为构造 `TomlValue` 根表交给 `Stringify`，顺带**删掉手拼那段**：一个正确的 TOML 写出器
比三行判断可靠。

## Implementation Notes

- `ProjectManifest` 加 `Properties: TomlValue`（基表）；`Profile` 加 `Properties: TomlValue`。
  两者缺省是空表（`TomlValue.OfTable()`），避免到处判 null。
- 合并在**生成器**里做（`base` 浅拷贝后用 profile 的顶层 key 覆盖），不在 loader 里——
  loader 只如实反映 manifest，"哪个 profile 生效"是 build 的上下文。
- VM 侧：`[properties]` 存成 `Option<toml::Table>` 挂在 `RuntimeConfig` 上（**不进
  `resolved`、不进 `KNOWN_KNOBS`**）。只从 app-config 层的那份表里取。
- builtin 四个：`__app_prop(key)->string?`（顶层标量）/ `__app_prop_has(key)->bool` /
  `__app_prop_names()->string[]` / `__app_props_toml()->string?`（原文）。追加注册。
- `Raw()` 由 VM 用 `toml::to_string` 重新序列化——不保留原始文本切片，避免持有整份侧车。

## Testing Strategy

| 层 | 测试 |
|---|---|
| 解析 | 基表 / per-profile 各自解析；缺省是空表 |
| 合并 | profile 覆盖基表的同名顶层 key；基表独有的 key 保留；**浅**覆盖（子表整体替换）|
| 序列化 | 标量 / 数组 / 嵌套表 round-trip：`Stringify` 出来的侧车能被 `Std.Toml` 再解析回等价结构 |
| VM 读取 | `Get` 取顶层标量（int/bool 渲染成字符串）；未知 key → null；`Has` / `Names` |
| 不分层 | `Z42_CONFIG` 里的 `[properties]` **不**生效且**有** warn；`--set` 无法设置属性 |
| 不干扰旋钮 | `[properties]` 里的键**不**触发 "unknown runtime knob" 诊断；`[runtime]` 里的未知键**仍然**触发 |
| e2e | 完整链路：manifest → build → 侧车 → `z42vm app.zpkg` 读到标量与嵌套表 |

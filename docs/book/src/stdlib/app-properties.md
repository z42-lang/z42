# `Std.Runtime.AppProperties` —— 应用自定义配置

> 对齐：2026-09-05（change `add-app-properties`）。代码：
> `src/libraries/z42.core/src/Runtime/AppProperties.z42`、
> `src/runtime/src/corelib/appprops.rs`、`z42c.driver/src/RuntimeConfigSidecar.z42`。
> 运行时旋钮那一套见 [运行时设置](../runtime/runtime-settings.md) 与
> [`Std.Runtime.RuntimeConfig`](runtime-config.md)。

## 它解决什么

app 常有自己的配置——API 端点、feature flag、限额。这些**不是 VM 旋钮**：VM 不认识
它们，也不该假装认识。但它们和旋钮一样，需要「写在工程里、随产物分发、运行时读得到」。

对照 .NET：这就是 `runtimeOptions.configProperties` + `AppContext.GetData` 那一层。

## 从 manifest 到 `Main()`

```toml
# app.z42.toml
[properties]                       # 基表：所有 profile 共用
app-name = "demo"
api-endpoint = "https://prod.example.com"
feature-flags = ["x", "y"]
[properties.limits]
max-retries = 3

[profile.debug]
mode = "interp"                    # ← 这是 VM 旋钮，进 [runtime]
[profile.debug.properties]         # ← 这是应用属性，逐 key 浅覆盖基表
api-endpoint = "http://localhost:8080"
```

`z42c build` 把合并结果烤进产物旁的侧车：

```toml
# dist/app.runtimeconfig.toml（生成）
[runtime]
mode = "interp"

[properties]
app-name = "demo"
api-endpoint = "http://localhost:8080"
feature-flags = ["x", "y"]
[properties.limits]
max-retries = 3
```

运行时**不需要任何人指路**——VM 按「同目录、同 stem」自己找到侧车（见
[app-config 层](../runtime/runtime-settings.md)），所以 `z42vm <app>` 直跑、`z42 run`、
已发布的 apphost、以及 wasm / iOS / Android 的嵌入入口**都**读得到。

```z42
using Std.Runtime;

void Main() {
    string ep = AppProperties.GetOrDefault("api-endpoint", "https://prod.example.com");
    // → "http://localhost:8080"（debug profile 覆盖了基表）
}
```

## API

| 方法 | 语义 |
|---|---|
| `Get(key) : string?` | 顶层**标量**属性。不存在、或值是数组/表 → `null`（后者用 `Raw()`）|
| `Has(key) : bool` | 顶层是否存在该键（值为数组/表时也为 true）|
| `Names() : string[]` | 全部顶层键 |
| `Raw() : string?` | 整段 `[properties]` 的 TOML 文本；无属性时 `null`（区别于空表）|
| `GetOrDefault(key, fallback)` | 取不到时给兜底 |

整数 / 布尔 / 日期由 `Get` 渲染成字符串。

### 完整 TOML 类型

标量之外的一切（数组、嵌套表、日期）走 `Raw()` + `Std.Toml`：

```z42
using Std.Runtime;
using Std.Toml;          // using 只能在文件顶部

void Main() {
    TomlValue p = TomlValue.Parse(AppProperties.Raw() ?? "");
    long retries = p.Get("limits").Get("max-retries").AsLong();
    string first = p.Get("feature-flags").At(0).AsString();
}
```

**为什么是"交出原文"而不是让 VM 暴露一套结构化 ABI**：z42 stdlib 已有完整的 TOML
解析器，VM 把那段文本交出去即可——简单情形一次 `Get` 搞定且**零依赖**，结构化情形用
现成的 `Std.Toml`。这样「完整类型」是零新增 ABI 得到的：**TOML 有什么就支持什么，
将来不需要为新的值类型再扩展一次 marshal**。发明结构化 marshal 或路径 mini-language
成本更高，表达力还更差（数组尤其难干净返回）。

## 与 `RuntimeConfig` 的分界

|  | `RuntimeConfig`（旋钮）| `AppProperties`（属性）|
|---|---|---|
| 登记表 | 有（`KNOWN_KNOBS`）| 无 |
| 类型 / 取值域 | 声明并校验 | 不校验 |
| build / feature / 平台可用性 | 有 | 无 |
| 优先级链 | 五层（cli > env > 用户配置 > app 侧车 > 默认）| **只有 app 侧车** |
| 未知 key | 产生诊断 | 就是正常情形 |

**为什么两张表、两个 API**：

- **分表**是为了保住诊断。若属性与旋钮同住 `[runtime]`，`gc-mdoe` 这种 typo 会被当成一个
  合法的用户属性静默收下，「未知旋钮就明确报出来」的承诺随之失效。
  （dotnet 走前缀约定可行，是因为它的 runtime 旋钮天然带 `System.*` 前缀；z42 的
  `gc-mode` / `libs` / `log` 没有统一前缀，走前缀路线等于给所有现有 key 改名。）
- **分 API** 是因为保证不同。混进一个 `Get`，"返回 null"会同时意味着"取默认值"和
  "这个旋钮压根不存在"，调用方没法区分。

## 边界

- **只读**：没有任何写入方法，与旋钮一致。
- **不分层**：`--set` 设不了属性（会按未知旋钮报错）；`Z42_CONFIG` 里写 `[properties]`
  不生效，且会给一行 warn 说明它归 app 所有——静默忽略会让人 debug 半天。
- **两段都空就不产侧车**：与「无旋钮不产文件」同一克制，让从前没有侧车的工程 dist
  内容逐字节不变。

## 关联

- [运行时设置](../runtime/runtime-settings.md)——五层链、旋钮登记表、侧车如何到达各形态
- [`Std.Runtime.RuntimeConfig`](runtime-config.md)——旋钮那一侧的只读查询面
- change `add-app-properties`（`docs/spec/changes/`）

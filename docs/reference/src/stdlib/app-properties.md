# `Std.Runtime.AppProperties` —— 应用自定义配置

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.core/`（`src/Runtime/AppProperties.z42`）；
> 命名空间 `Std.Runtime`

app 自己的配置——API 端点、feature flag、限额——写在工程清单里，`z42c build` 把它烤进
产物旁的 `<app>.runtimeconfig.toml`，运行时**只读**地查询。对照 .NET 就是
`runtimeOptions.configProperties` + `AppContext.GetData` 那一层。

这些**不是 VM 旋钮**：VM 不认识、不校验它们，未知 key 是正常情形，也没有命令行 / 环境
变量的覆盖层。VM 旋钮走
[`Std.Runtime.RuntimeConfig`](runtime-config.md)，两者不可混用。

VM 按「与产物同目录、同 stem」自己找到侧车，所以 `z42vm <app>` 直跑、`z42 run`、已发布
的 apphost、以及 wasm / iOS / Android 的嵌入入口都读得到，不需要指路。

## API

```z42
public static class AppProperties {
    public static extern string?  Get(string key);
    public static extern bool     Has(string key);
    public static extern string[] Names();
    public static extern string?  Raw();
    public static string GetOrDefault(string key, string fallback);
}
```

| 方法 | 语义 |
|---|---|
| `Get(key)` | 顶层**标量**属性。不存在、或值是数组 / 表 → `null`（后者用 `Raw()`）|
| `Has(key)` | 顶层是否存在该键（值为数组 / 表时同样为 `true`）|
| `Names()` | 全部顶层键；没有属性时是空数组 |
| `Raw()` | 整段 `[properties]` 的 TOML 文本；**没有属性时返回 `null`**（区别于空表）|
| `GetOrDefault(key, fallback)` | `Get` 取不到（不存在或非标量）时返回 `fallback` |

整数 / 布尔 / 日期由 `Get` 渲染成字符串。

**没有 setter**——属性是只读的。

## 清单里怎么写

```toml
# app.z42.toml
[properties]                       # 基表：所有 profile 共用
app-name = "demo"
api-endpoint = "https://prod.example.com"
feature-flags = ["x", "y"]
[properties.limits]
max-retries = 3

[profile.debug.properties]         # 逐 key 浅覆盖基表
api-endpoint = "http://localhost:8080"
```

`z42c build` 把合并结果写进 `dist/app.runtimeconfig.toml` 的 `[properties]` 段。
`[properties]` 与 `[runtime]`（旋钮）两段都空时不产侧车文件。

## 用法

标量直接 `Get`，零依赖：

```z42
using Std.IO;
using Std.Runtime;

void Main() {
    string ep = AppProperties.GetOrDefault("api-endpoint", "https://prod.example.com");
    Console.WriteLine(ep);
}
```

数组、嵌套表、日期等非标量走 `Raw()` + `Std.Toml` —— TOML 有什么就支持什么：

```z42
using Std.Runtime;
using Std.Toml;          // using 只能写在文件顶部

void Main() {
    string raw = AppProperties.Raw();
    if (raw == null) { raw = ""; }          // Raw() 标了 `?`：没有 app-properties 时为 null
    TomlValue p = TomlValue.Parse(raw);
    long retries = p.Get("limits").Get("max-retries").AsLong();
    string first = p.Get("feature-flags").At(0).AsString();
}
```

## 与 `RuntimeConfig` 的分界

|  | `RuntimeConfig`（VM 旋钮）| `AppProperties`（应用属性）|
|---|---|---|
| 有登记表 | 是 | 否 |
| 类型 / 取值域校验 | 有 | 无 |
| build / 平台可用性 | 有（`IsAvailable`）| 无 |
| 来源 | cli / env / 用户配置 / app 侧车 / 默认 | **只有 app 侧车** |
| 未知 key | 产生诊断 | 正常情形 |

因此：`--set` 设不了属性（会按未知旋钮报错）；在 `Z42_CONFIG` 指向的用户配置里写
`[properties]` 不生效，并会得到一行 warn 说明它归 app 所有。

## 关联页面

- [`Std.Runtime.RuntimeConfig`](runtime-config.md) —— VM 旋钮那一侧的只读查询面
- [工程清单 z42.toml](../toolchain/z42-toml.md) —— 工程清单的其余字段

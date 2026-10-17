<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.AppProperties

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Runtime`

`src/libraries/z42.core/src/Runtime/AppProperties.z42:12`

应用自定义配置属性（**app 自己的**配置，只读）。来自工程 manifest 的 `[properties]` + `[profile.<n>.properties]`，由 `z42c build` 烤进 runtimeconfig.toml 随产物分发。

与 `Std.Runtime.RuntimeConfig` 的分工：RuntimeConfig 是 VM 旋钮（有登记表、校验、
五层优先级链）；AppProperties 是 app 自己的配置（VM 不理解不校验，未知 key 是正常
情形，只从 app 侧车来，没有 CLI / env / 用户配置覆盖）。

完整 TOML 类型：标量用 `Get` 一次拿到；数组 / 嵌套表用 `Raw()` 取整段 TOML 文本
交给 `Std.Toml` 解析。

```z42
public static class AppProperties {
    static extern string? Get(string key)
    static extern bool Has(string key)
    static extern string[] Names()
    static extern string? Raw()
    static string GetOrDefault(string key, string fallback)
}
```

### static extern string? Get(string key)

`src/libraries/z42.core/src/Runtime/AppProperties.z42:16`

顶层标量属性；不存在、或该键的值不是标量（数组 / 表）→ null（非标量请用 `Raw()`）。 整数 / 布尔 / 日期渲染为字符串。

### static extern bool Has(string key)

`src/libraries/z42.core/src/Runtime/AppProperties.z42:20`

顶层是否存在该键（值为数组 / 表时同样为 true）。

### static extern string[] Names()

`src/libraries/z42.core/src/Runtime/AppProperties.z42:24`

全部顶层键。

### static extern string? Raw()

`src/libraries/z42.core/src/Runtime/AppProperties.z42:28`

整段 `[properties]` 的 TOML 文本；没有属性时返回 null（区别于空表）。

### static string GetOrDefault(string key, string fallback)

`src/libraries/z42.core/src/Runtime/AppProperties.z42:31`

便利包装：取不到（不存在或非标量）时返回 fallback。


<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.RuntimeConfig

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Runtime`

`src/libraries/z42.core/src/Runtime/RuntimeConfig.z42:7`

运行时设置的**只读**查询面。z42vm 把每个旋钮按固定优先级链解析一次，boot 后冻结： `cli (--set / --mode)` > `env (Z42_*)` > `用户配置 (Z42_CONFIG)` > `应用侧车 (Z42_APP_CONFIG)` > `内置默认`。本类给解析后的生效值与来源。**没有 setter**（boot 后物理不可变）； 运行期可调的能力走专门 API（`Std.GC`）。

```z42
public static class RuntimeConfig {
    static extern string? Get(string key)
    static extern string Source(string key)
    static extern string[] Names()
    static extern string[] Dump()
    static extern string? Describe(string key)
    static extern bool IsAvailable(string key)
    static string GetOrDefault(string key, string fallback)
}
```

### static extern string? Get(string key)

`src/libraries/z42.core/src/Runtime/RuntimeConfig.z42:11`

旋钮的生效值（如 "stw"）。未知 key、或该旋钮取内置默认时返回 null。key 用旋钮的配置键 （`gc-mode`），也接受环境变量名（`Z42_GC_MODE`）。

### static extern string Source(string key)

`src/libraries/z42.core/src/Runtime/RuntimeConfig.z42:16`

生效值来自哪一层："cli" | "env" | "user-config" | "app-config" | "default"。 未知 key 返回 "unknown"。

### static extern string[] Names()

`src/libraries/z42.core/src/Runtime/RuntimeConfig.z42:20`

全部旋钮的 key。

### static extern string[] Dump()

`src/libraries/z42.core/src/Runtime/RuntimeConfig.z42:25`

扁平转储，每项形如 `"gc-mode=stw|env"`；取默认时 value 段为空。按**第一个** `=` 和**最后一个** `|` 切分（值本身可能含 `=`）。

### static extern string? Describe(string key)

`src/libraries/z42.core/src/Runtime/RuntimeConfig.z42:29`

旋钮的一行说明；未知 key 返回 null。

### static extern bool IsAvailable(string key)

`src/libraries/z42.core/src/Runtime/RuntimeConfig.z42:34`

该旋钮在**当前 build 与平台**是否存在（如 `jit-profile` 需要 jit feature，interp-only 的 z42vm 上返回 false）。未知 key 返回 false。

### static string GetOrDefault(string key, string fallback)

`src/libraries/z42.core/src/Runtime/RuntimeConfig.z42:37`

便利包装：取不到时返回 fallback。


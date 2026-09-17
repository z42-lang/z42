# `Std.Runtime.RuntimeConfig` —— 只读地查询运行时旋钮

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.core/`（`src/Runtime/RuntimeConfig.z42`）；
> 命名空间 `Std.Runtime`

z42vm 的每个运行时旋钮（`gc-mode` / `mode` / `log` / `jit-threshold` …）在启动时被解析
一次并冻结。这个类让 z42 代码读到**解析后的生效值、它来自哪一层、以及这个旋钮在当前
build 存不存在**。

直接 `Environment.GetEnvironmentVariable("Z42_GC_MODE")` 只能拿到环境变量那一层的原始
字符串：看不到 `--set` 的覆盖，也分不清「根本没设」与「设了但被更高层压过」。

应用自己的配置不在这里，走
[`Std.Runtime.AppProperties`](app-properties.md)。

## API

```z42
public static class RuntimeConfig {
    public static extern string?  Get(string key);
    public static extern string   Source(string key);
    public static extern string[] Names();
    public static extern string[] Dump();
    public static extern string?  Describe(string key);
    public static extern bool     IsAvailable(string key);
    public static string GetOrDefault(string key, string fallback);
}
```

| 方法 | 语义 | 未知 key |
|---|---|---|
| `Get(key)` | 旋钮的生效值。**取内置默认时返回 `null`** | `null` |
| `Source(key)` | 生效值来自哪一层 | `"unknown"` |
| `Names()` | 全部旋钮的 key | — |
| `Dump()` | 扁平转储，每项 `"key=value\|source"` | — |
| `Describe(key)` | 旋钮的一行说明 | `null` |
| `IsAvailable(key)` | 该旋钮在当前 build 与平台是否存在 | `false` |
| `GetOrDefault(key, fallback)` | `Get` 为 `null` 时给 `fallback` | `fallback` |

**没有 setter**——配置在 VM 侧启动后物理不可变，且多数旋钮只在启动期被消费一次。需要
运行期可调的能力走专门 API（如 `Std.GC`）。

### key 的写法

用旋钮的配置键（`gc-mode`）。查询面比 `--set` 宽松，也接受环境变量名
（`Z42_GC_MODE`）。少数旋钮本身只有环境变量形态（`Z42_CONFIG` / `Z42_APP_CONFIG` /
`Z42_STRICT_CONFIG`），`Names()` 里就以该拼写出现。

### `Source` 的取值

```
"cli" | "env" | "user-config" | "app-config" | "default" | "unknown"
```

依次对应：`--set` / `--mode` 命令行、`Z42_*` 环境变量、用户配置文件、应用侧车、内置
默认、以及 key 根本不存在。前四层压后一层，命令行最高。

### `Get` 与 `IsAvailable` 是两回事

`Get` 回答「当前生效值是什么」，`IsAvailable` 回答「这个旋钮在**这个 build / 这个平台**
存不存在」。例如 `jit-profile` 需要 `jit` feature，interp-only 的 z42vm 上
`IsAvailable("jit-profile") == false`——在那种 build 上设它不会生效。写自适应逻辑
（「能开 profiling 就开」）时该问 `IsAvailable`，不是 `Get`。

### `Dump` 的切分约定

每条形如 `"gc-mode=concurrent|env"`；取默认时 value 段为空（`"gc-mode=|default"`）。
按**第一个** `=` 和**最后一个** `|` 切分——value 本身可能含 `=`（如 `log` 的
`z42::jit=debug,z42=warn`）。

返回扁平 `string[]` 而不是 map，沿用 `Environment.GetEnvironmentVariables()` 已确立的
约定。

## 用法

```z42
using Std.IO;
using Std.Runtime;

void Main() {
    // 自适应：这个 build 支持才开
    if (RuntimeConfig.IsAvailable("jit-profile")) { /* ... */ }

    // 诊断输出里带上「设置从哪来」，排查配置问题时省一轮追问
    Console.WriteLine("gc-mode=" + RuntimeConfig.GetOrDefault("gc-mode", "generational-mark-sweep")
                    + " (from " + RuntimeConfig.Source("gc-mode") + ")");

    foreach (string entry in RuntimeConfig.Dump()) {
        Console.WriteLine(entry);           // gc-mode=|default
    }
}
```

## 命令行侧的对应物

| 命令 | 给出什么 |
|---|---|
| `z42vm --list-knobs` | **有哪些旋钮**：类型 / 可设置层 / 本 build 可用性 / 默认值。默认只列 public 旋钮，`--all` 连内部与本 build 不可用的一起列 |
| `z42vm --show-config` | 每个旋钮的**生效值 + 来源**（同样默认只列 public 一档）|
| `z42vm --info` | 构建信息 + 完整旋钮快照 |
| `z42vm --set KEY=VALUE` | 为本次运行设置一个旋钮，可重复；优先级最高 |
| `z42vm --strict-config` | 把来自环境变量 / 配置文件的配置问题从警告升级为致命错误 |

> `Names()` / `Dump()` 返回的是**全部**旋钮（含内部与本 build 不可用的），比
> `--show-config` 默认那一档多。要区分，逐个问 `IsAvailable`。

## 关联页面

- [`Std.Runtime.AppProperties`](app-properties.md) —— 应用自己的配置（不是 VM 旋钮）

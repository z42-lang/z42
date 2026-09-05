# `Std.Runtime.RuntimeConfig` —— 只读地查询运行时设置

> 对齐：2026-09-05。代码：`src/libraries/z42.core/src/Runtime/RuntimeConfig.z42`
> （builtin 在 `src/runtime/src/corelib/config.rs`）。机制全貌见
> [运行时设置](../runtime/runtime-settings.md)。

## 为什么不是直接读环境变量

z42vm 把每个旋钮按固定优先级链解析一次，boot 后冻结：

```
cli (--set / --mode)  >  env (Z42_*)  >  用户配置 (Z42_CONFIG)
    >  应用侧车 (<app>.runtimeconfig.toml)  >  内置默认
```

直接 `Environment.GetEnvironmentVariable("Z42_GC_MODE")` 只能拿到 **env 那一层的原始
字符串**：看不到 `--set` 的覆盖、看不到配置文件、也分不清"根本没设"与"设了但被更高层
压过"。这个类给的是**解析后的生效值和它的来源**。

命令行侧的对应物是 `z42vm --show-config`（生效值 + 来源 + 为什么某层没生效）与
`z42vm --list-knobs`（schema）。

## API

```z42
using Std.Runtime;

string? v   = RuntimeConfig.Get("gc-mode");            // 生效值；取默认时 null
string  src = RuntimeConfig.Source("gc-mode");         // 来源层标签
bool    ok  = RuntimeConfig.IsAvailable("jit-profile");// 本 build / 本平台是否存在
string[] ns = RuntimeConfig.Names();                   // 全部旋钮 key
string[] d  = RuntimeConfig.Dump();                    // "key=value|source" 扁平条目
string? doc = RuntimeConfig.Describe("gc-mode");       // 一行说明
string  x   = RuntimeConfig.GetOrDefault("gc-mode", "stw");
```

| 方法 | 未知 key 时 |
|---|---|
| `Get` / `Describe` | `null` |
| `Source` | `"unknown"` |
| `IsAvailable` | `false` |

`Source` 的取值：`"cli"` / `"env"` / `"user-config"` / `"app-config"` / `"default"`
（未知 key 为 `"unknown"`）。

key 用旋钮的配置键（`gc-mode`）；查询面比 `--set` 宽松，也接受环境变量名
（`Z42_GC_MODE`）——这里没有 typo 要抓，而 `--set` 那边有。

### `IsAvailable` 与 `Get` 是两回事

`Get` 回答"当前生效值是什么"，`IsAvailable` 回答"这个旋钮在**这个 build / 这个平台**
存不存在"。例如 `jit-profile` 需要 `jit` feature，interp-only 的 z42vm 上
`IsAvailable("jit-profile") == false`——在那种 build 上设它不会生效，VM 会在启动时打印
一行说明。写自适应逻辑（"能开 profiling 就开"）时该问 `IsAvailable`，不是 `Get`。

### `Dump` 的切分约定

每条形如 `"gc-mode=concurrent|env"`；取默认时 value 段为空（`"gc-mode=|default"`）。
按**第一个** `=` 和**最后一个** `|` 切分——value 本身可能含 `=`（如 `log` 的
`z42::jit=debug,z42=warn`）。

返回扁平 `string[]` 而不是 Map，沿用 `Environment.GetEnvironmentVariables()` 已确立的
约定（z42 当前没有稳定的 `Map<string,string>` marshal 通路）。

## 为什么没有 setter

1. 配置在 VM 侧是 `OnceLock`，boot 后**物理不可变**。加 setter 要换成 `RwLock`，给每次
   safepoint 都读的 `safepoint-throttle` 加锁开销——为一个边缘能力惩罚主路径。
2. 多数旋钮**只在启动期被消费一次**（`libs` 定位、`sample-hz` 决定要不要起采样线程、
   `gc-mode` 决定建哪种堆）。运行中改它们要么无效、要么需要重建子系统——
   **"能设但不生效"比"不能设"更坏**，这正是 .NET `AppContext.SetSwitch` 的坑。
3. 真正需要运行期可调的能力（触发回收、调堆预算）有专门 API（`Std.GC`），语义明确、
   可实现、可测。

## 典型用法

```z42
using Std.IO;
using Std.Runtime;

void Main() {
    // 自适应：这个 build 支持才开
    if (RuntimeConfig.IsAvailable("jit-profile")) { /* ... */ }

    // 诊断输出里带上"设置从哪来"，排查配置问题时省一轮追问
    Console.WriteLine("gc-mode=" + RuntimeConfig.GetOrDefault("gc-mode", "stw")
                    + " (from " + RuntimeConfig.Source("gc-mode") + ")");
}
```

## 关联

- [运行时设置](../runtime/runtime-settings.md)——五层链、旋钮登记表、可用性与诊断
- change `complete-runtime-settings`（引入）/ `launcher-forwards-set`（本页补齐）

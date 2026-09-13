# Design: 补完运行时设置系统（complete-runtime-settings）

> 状态：🔴 DRAFT（待 User 审批）| 更新：2026-09-05（User 裁决 U1–U4）
> 提案 [proposal.md](proposal.md) | spec [specs/runtime-settings/spec.md](specs/runtime-settings/spec.md)
> 参考实现：CoreCLR `src/coreclr/inc/{clrconfig.h, clrconfigvalues.h, configuration.h}`；侧车**生成时机**对齐 dotnet SDK 的 `<app>.runtimeconfig.json`（格式仍是 z42 自己的 TOML）

---

## Architecture

```
              ┌───────────────────────── 输入层（高 → 低）──────────────────────────┐
  L1 CLI      │  --set k=v（可重复） / --mode                                       │  新增
  L2 env      │  Z42_*                                                             │  已有
  L3 用户配置  │  Z42_CONFIG        → [runtime] TOML                                │  已有，不再被侧车抢占
  L4 应用配置  │  Z42_APP_CONFIG    → <app>.runtimeconfig.toml                      │  新增通道 + 新增生成器
              │                       ▲ z42c build 从 manifest [profile.*] 烤入     │
  L5 默认      │  KNOWN_KNOBS[i].default                                            │  已有
              └───────────────────────────────┬───────────────────────────────────┘
                                              ▼
                         ┌────────────────────────────────────┐
  schema SoT ───────────►│  resolve(layers) → ResolvedKnob[]  │◄─── availability(build/feature/platform)
  KNOWN_KNOBS            │   { name, raw, source: Layer,      │◄─── sources: LayerMask（可接受层）
  (类型/可用性/层/tier)   │     ignored: [(layer,val,reason)] } │
                         └───────────┬────────────┬───────────┘
                                     │            │
                    typed 字段 ◄─────┘            └─────► render.rs（唯一渲染器）
                    RuntimeConfig{..}                      ├─ --info（旋钮块）
                           │                               ├─ --list-knobs [--all] [--json]
                           │                               └─ --show-config [--json]
                           ▼
             runtime_config()（OnceLock，boot 后冻结）
                  ├─ Rust 子系统（gc / jit / native / main）
                  └─ __cfg_* builtins ──► Std.Runtime.RuntimeConfig（只读）
```

**不变式**：`KNOWN_KNOBS` 是 schema 的唯一 SoT。新增一个旋钮 = 表里加一行 + 在 `consumed_by` 处读它；CLI / env / 两个文件层 / 查询 / 脚本表面**全部自动跟随**，无第二处需要改。

**L3/L4 是同一种东西的两个实例**：格式相同（TOML `[runtime]` 表）、解析器相同（`load_config_file`），区别只在**谁写的**——L3 用户手写，L4 由 build 生成。所以它们逐 key 叠加而非互斥。

---

## Decisions

### Decision 1：CLI 用通用 `--set k=v`；key 只认完整定义，短写法靠显式 alias

**选定**（User 裁决 U2）：`--set <key>=<value>`（可重复），`key` **只接受旋钮的 `toml_key`**（kebab-case）或它在 `KnobSpec.aliases` 里**显式声明**的短名。**不**自动接受 `Z42_*` env 名。

**理由**：
- 21 个旋钮逐个加 clap flag 会让 `--help` 从 8 行涨到 30+，且每加一个旋钮要同时改 `Cli` struct、`KNOWN_KNOBS`、消费处三处——违反"表格是唯一 SoT"。通用 `--set` 让新增旋钮的成本保持在一行表格编辑。
- 自动接受 env 名等于建立一条**隐式**的双写法约定：将来若某个旋钮的 env 名与 kebab key 不再是机械映射（例如为兼容而改名），"自动等价"就会失效或产生歧义。改成 `aliases` 显式声明后，每个别名都是表里可见、可枚举、可被 `--list-knobs` 打印的事实。
- 首切 `aliases` 全部为空——先不发明短名。真有高频需求时按需加一行。

**对照**：CoreCLR 干脆**没有** CLI 层（只有 env + host 传入的 configProperties），`dotnet` CLI 也不暴露旋钮。z42 加 CLI 层是因为 `z42vm` 是**用户直接敲的命令**（不像 `dotnet` 后面还有一层 apphost 吃掉命令行），命令行是最自然的一次性覆盖入口。

**保留专用 flag 的例外**：`--mode` 已存在且高频，保留；与 `--set mode=` **同层**，同时给出 → 报错（不定义谁赢——两种写法没有"更具体"之分，猜一个只会制造记忆负担）。

**语法边界**：值含 `=` 时按**第一个** `=` 切分（`--set path=/a=b:/c` → key=`path`, value=`/a=b:/c`）。空值（`--set gc-mode=`）视为**显式清空** → 回落下一层，与 env 空串语义一致（现有 `.filter(|s| !s.trim().is_empty())`）。

---

### Decision 2：`KnobSpec` 扩为完整 schema

```rust
pub struct KnobSpec {
    // 现有
    pub name: &'static str,          // Z42_GC_MODE
    pub toml_key: &'static str,      // gc-mode（也是 --set 的 key）
    pub description: &'static str,
    pub default_hint: &'static str,
    pub consumed_by: &'static str,
    // 新增
    pub aliases:   &'static [&'static str],  // --set 的额外短名（首切全空，见 Decision 1）
    pub value:     ValueKind,                // 类型（校验 + json schema）
    pub sources:   LayerMask,                // 可接受的输入层（见 Decision 9）
    pub build:     BuildAvail,               // Always | DebugOnly
    pub requires:  &'static [&'static str],  // 需要的 cargo feature（全满足才可用）
    pub platforms: PlatformAvail,            // All | Only(..) | Except(..)
    pub tier:      Tier,                     // Public | Unsupported | Internal
}

pub enum ValueKind { Bool, Int{min:i64,max:i64}, Float{min:f64,max:f64},
                     Str, Path, PathList, Enum(&'static [&'static str]) }
pub enum BuildAvail    { Always, DebugOnly }
pub enum PlatformAvail { All, Only(&'static [&'static str]), Except(&'static [&'static str]) }
pub enum Tier          { Public, Unsupported, Internal }
pub struct LayerMask(u8);  // 位集：Cli | Env | UserConfig | AppConfig
```

**为什么是声明式字段而不是一个 `available_when: fn() -> bool`**：闭包/函数指针不可渲染——`--list-knobs --json` 要把"为什么不可用"输出给工具消费，只有**声明式**约束才能被打印、被 diff、被文档生成。CoreCLR 的宏前缀是同一思路（编译期声明，非运行期判定）。

**平台轴为什么需要 `Except`**：多数平台约束的自然表达是"除了 wasm"（采样 profiler 要后台线程、native 扩展要 dlopen）；列举正面清单会在每加一个目标三元组时漏掉。

**可用性求值**（`availability.rs`）——四项全通过才算生效：
1. `sources` 允许该层（Decision 9）
2. `build`：`DebugOnly` 时要求 `cfg!(debug_assertions)`
3. `requires`：逐个查 `feature_enabled(name)`，基于 `cfg!(feature=..)` 的静态 match（feature 名是编译期字面量、无法反射，这张小映射表必须手写；单测防腐见 Implementation Notes）
4. `platforms`：与 `std::env::consts::OS` 比对

### 初始 schema 表（21 旋钮 + 3 元旋钮）

`sources` 列：`C`=CLI, `E`=env, `U`=用户配置, `A`=应用配置。`aliases` 首切全空，略。

| 旋钮 | value | sources | build | requires | platforms | tier |
|---|---|---|---|---|---|---|
| `Z42_CONFIG` | Path | **C E** | Always | — | All | Internal |
| `Z42_APP_CONFIG` 🆕 | Path | **C E** | Always | — | All | Internal |
| `Z42_STRICT_CONFIG` 🆕 | Bool | **C E** | Always | — | All | Internal |
| `Z42_CRASH_DIR` | Path | C E U A | Always | — | All | Public |
| `Z42_GC_MINOR_THRESHOLD` | Float[0,1] | C E U A | Always | — | All | Unsupported |
| `Z42_GC_MODE` | Enum(stw/concurrent/generational + 别名) | C E U A | Always | — | All | Public |
| `Z42_GC_NEAR_LIMIT_RATIO` | Float[0,1] | C E U A | Always | — | All | Unsupported |
| `Z42_GC_PAUSE_WINDOW` | Int[1,65536] | C E U A | Always | — | All | Unsupported |
| `Z42_GC_PRESSURE_RATIO` | Float[0,1] | C E U A | Always | — | All | Unsupported |
| `Z42_GC_SOFT_THRESHOLD` | Float[0,1] | C E U A | Always | — | All | Unsupported |
| `Z42_GC_THROTTLE_RATIO` | Float[0,1] | C E U A | Always | — | All | Unsupported |
| `Z42_JIT_PROFILE` | Bool | C E U A | Always | `jit` | All | Public |
| `Z42_LIBS` | Path | C E U A | Always | — | All | Public |
| `Z42_LOG` | Str | C E U A | Always | — | All | Public |
| `Z42_MODE` | Enum(interp/jit/aot) | C E U A | Always | — | All | Public |
| `Z42_NATIVE_PATH` | PathList | C E U A | Always | `native-interop` | Except(wasm) | Public |
| `Z42_PATH` | PathList | C E U A | Always | — | All | Public |
| `Z42_SAFEPOINT_THROTTLE` | Int[1,u32::MAX] | C E U A | Always | — | All | Unsupported |
| `Z42_SAMPLE_HZ` | Int[1,∞) | C E U A | Always | — | Except(wasm) | Public |
| `Z42_SAMPLE_OUT` | Path | C E U A | Always | — | Except(wasm) | Public |
| `Z42_STRESS_ITERS` | Int[1,∞) | **E** | **DebugOnly** | — | All | **Internal** |
| `Z42_TARGET` | Str | C E U A | Always | — | All | **Internal**（reserved）|
| `Z42_TRACE_OUT` | Path | C E U A | Always | — | Except(wasm) | Public |

> **`Z42_MODE` 的有意例外**：它是三值 Enum，但 jit/aot 的 feature 门控是**逐值**的（interp 在任何 build 都可用），不是整个旋钮的。故旋钮本身 `requires: []`，逐值门控继续由 `main.rs` 现有的 `resolve_config_mode` 负责（已会 warn + 落 build 默认）。可用性层**不重复实现**；spec 显式写明这条例外，避免后人"修正"成 `requires:["jit"]` 而让 interp-only build 上 `Z42_MODE=interp` 也被判不可用。

---

### Decision 9：每个旋钮声明可接受的输入层（`sources: LayerMask`）

**选定**（User 裁决 U3）：不是所有旋钮都该从所有层设置。

- **元旋钮只接受 CLI/env**（`Z42_CONFIG` / `Z42_APP_CONFIG` / `Z42_STRICT_CONFIG`）：它们决定**读哪个文件**和**诊断有多严格**。允许写在配置文件里会自指——一个文件指定要读哪个文件（无限递归 / 加载顺序悖论），或一个文件把自身的错误从 error 降成 warn。CoreCLR 同样把"host 传入哪些 configProperties"放在配置系统之外（由 host 决定，不由 runtimeconfig 自己决定）。
- **`Z42_STRESS_ITERS` 只接受 env**：它是 GC 压力测试脚手架，进 CLI 表面等于向用户暗示这是个正经旋钮；进配置文件等于让它随产物分发。
- **其余旋钮四层全收**。

从不被接受的层设置 → 复用 Decision 3 的同一套诊断，reason 为 `NotAcceptedFrom(layer)`，消息形如：
```
z42: 旋钮 `stress-iters`（Z42_STRESS_ITERS）不能从 [cli] 设置（仅接受：env）。
```

**为什么放进 schema 而不是硬编码判断**：与 Decision 2 同理——`--list-knobs` 要能打印"这个旋钮能从哪儿设"，这是用户最先想知道的事之一（"我能不能在命令行改它？"）。硬编码的 `if name == "Z42_CONFIG"` 无法被枚举。

---

### Decision 3：不可用 / 不接受的严重度按来源分层（CLI 致命 / 其余 warn）

| 来源 | 未知 key | 已知但不可用 / 不接受该层 | 类型·范围非法 |
|---|---|---|---|
| **L1 CLI** | **error, exit 2** + 最近邻建议 | **error, exit 2** | **error, exit 2** |
| L2 env | warn（仅对 `Z42_` 前缀且不在表内的名字）| warn + 忽略 | warn + 落默认（现状行为）|
| L3 用户配置 | warn（`[runtime]` 下的未知 key）| warn + 忽略 | warn + 落默认 |
| L4 应用配置 | warn | warn + 忽略 | warn + 落默认 |

**理由**：CLI 是"此刻、此机、手敲"的意图——静默忽略一个 typo 会让用户以为设置生效了，是最坏结果。env / 两个文件层则**跨机器与跨 build 传播**（CI 全局 export、容器镜像 ENV、随产物分发的侧车）；若它们也致命，一个 export 了 `Z42_JIT_PROFILE=1` 的 CI 环境会让所有 interp-only 二进制**起不来**——用可用性检查换来一场可用性事故。

**CoreCLR 对照**：CoreCLR 对 release build 里的 debug-only 旋钮是**完全静默忽略**（宏在 `#ifdef _DEBUG` 外根本不生成符号，名字都不存在）。z42 选择"忽略但明说"，成本是一行 stderr，收益是用户不必读源码就知道旋钮为什么不生效。

**逃生门**：`--strict-config`（等价 `Z42_STRICT_CONFIG=1`）把所有 warn 升级为 error，CI 用它把「配置漂移」变成硬失败。它本身是元旋钮（Decision 9），不能从配置文件设。

**诊断格式**（一行标题 + 缩进详情，grep 友好）：
```
z42: 旋钮 `jit-profile`（Z42_JIT_PROFILE，来源 [env]）在本 build 不可用：
     需要 feature `jit`；本 z42vm 编译时启用的是：interp, native-interop。
     → 已忽略该值，使用默认（JIT profiling off）。
     用 `z42vm --list-knobs --all` 查看全部旋钮的可用性。
```
诊断发生在 tracing subscriber 装好**之前**（config 决定 `Z42_LOG`），故走 `eprintln!`——与现有 `parse_*` 的 warn 一致。

---

### Decision 4：provenance 随解析产出，不事后重算

`RuntimeConfig` 增一个并行的 `resolved: Vec<ResolvedKnob>`（boot 期一次）：

```rust
pub struct ResolvedKnob {
    pub name: &'static str,          // 指向 KNOWN_KNOBS 的静态名
    pub raw: Option<String>,         // 生效的原始字符串（None = 用默认）
    pub source: Layer,               // Cli | Env | UserConfig | AppConfig | Default
    pub ignored: Vec<(Layer, String, IgnoreReason)>,
}
pub enum Layer { Cli, Env, UserConfig, AppConfig, Default }
pub enum IgnoreReason {
    Overridden,                  // 被更高层覆盖
    Unavailable(String),         // build / feature / platform 不满足
    NotAcceptedFrom(Layer),      // sources 掩码不含该层
    Invalid(String),             // 类型 / 范围非法
}
```

**为什么不让 `--show-config` 自己重新查一遍 env**：`main.rs` 现在的 `print_build_info` 正是这么做的（[main.rs:330-346](../../../../src/runtime/src/main.rs) 再 `std::env::var(knob.name)` 判一次），结果是**渲染逻辑与解析逻辑两份实现**，任何优先级改动都要改两处且可能漂移。让解析器产出 provenance、渲染器纯读——一份真相。这也是 `__cfg_source` builtin 能诚实回答的前提。

`ignored` 让 `--show-config` 能回答最有价值的那个问题——**"我明明设了，为什么没生效"**：
```
gc-mode     = concurrent           [env: Z42_GC_MODE]
  ↳ 忽略 [app-config] "stw"        （被更高层覆盖）
jit-profile = (默认: off)          [default]
  ↳ 忽略 [env] "1"                 （不可用：需要 feature `jit`）
```

---

### Decision 5：脚本表面**只读**，无 setter

`Std.Runtime.RuntimeConfig` 只有 `Get / Source / Names / Dump / Describe / IsAvailable`，**没有 `Set`**。

1. 配置在 `OnceLock` 里，boot 后物理不可变——加 setter 就要换成 `RwLock`，给每个热路径读（`safepoint_throttle` 在每次 safepoint 都读）加锁开销，为边缘能力惩罚主路径。
2. 语义上多数旋钮**只在 boot 期被消费一次**（`Z42_LIBS` 定位、`Z42_SAMPLE_HZ` 决定是否起采样线程、`Z42_GC_MODE` 决定建哪种堆）——运行中改它们要么无效、要么要重建子系统；"能设但不生效"比"不能设"更坏。
3. 真正需要运行期可调的能力（触发 GC、调堆上限）已有或应有**专用 API**（`Std.GC`），语义明确、可实现、可测。

**对照**：.NET 有 `AppContext.SetSwitch`，但 CoreCLR 内部对已缓存的 CLRConfig 值同样不重读——"运行期改了 switch 但组件早读过了"是 .NET 生态的常见坑。z42 选择不制造这个坑。

**扁平 `string[]` 返回形态**：`Names()` / `Dump()` 返回 `string[]` 而非 Map，沿用 `Environment.GetEnvironmentVariables()` 已确立的约定（z42 当前无稳定的 `Map<string,string>` marshal 通路）。`Dump()` 条目形如 `"gc-mode=concurrent|env"`，按**第一个** `=` 与**最后一个** `|` 切分。

---

### Decision 6：工程 profile 走 build 生成的侧车 TOML，不走 env 注入（User 裁决 U1）

**问题**（proposal「现存缺陷 1/2」）：
1. launcher 把 `[profile.debug].mode` 注入成 `Z42_MODE`（[launcher.z42:437-440](../../../../src/toolchain/launcher/core/launcher.z42)），profile 落在 env 层、压过配置文件，且只能带 mode 一个旋钮。
2. launcher 用 `Z42_CONFIG` 一个通道同时表达"用户配置"与"应用侧车"（[launcher.z42:321-326](../../../../src/toolchain/launcher/core/launcher.z42)），用户一设 `Z42_CONFIG`，侧车整份被丢弃。

**选定**：

| 步骤 | 做法 |
|---|---|
| **生成** | `z42c build` 产 `dist/<name>.zpkg` 时同产 `dist/<name>.runtimeconfig.toml`，把 manifest 里生效 profile 的旋钮烤成 `[runtime]` 表 |
| **传递** | launcher 用**独立** env `Z42_APP_CONFIG` 传侧车路径；`Z42_CONFIG` 只属于用户 |
| **叠加** | VM 端 L3（用户）**逐 key 高于** L4（应用）；两层都用同一 `load_config_file` |
| **停用** | launcher 不再注入 `Z42_MODE` |

**为什么生成放在 z42c driver 而不是 launcher**：
- driver 已经知道 manifest（含 `[profile.*]`）、知道 `dist/` 路径、并且已经在那儿写 `dist/<name>.zpkg`（多 exe 时循环写多份）——侧车必须与产物 1:1，放在写产物的地方才不会漏。
- 放 launcher 意味着只有 `z42 run <dir>` 路径产侧车，`z42 build` 单独跑、以及 `z42 publish` 打包时都没有——产物就不自洽了。
- **对照 dotnet**：`<app>.runtimeconfig.json` 由 SDK 在**构建时**生成并与 dll 一起进 `bin/`，不是运行时拼的。同样的位置。
- **代价**：碰自举编译器 → 必须过 gen1==gen2 不动点验证。故排在 P5 最后，前五阶段不依赖它。

**探查确认（2026-09-05）：今天全仓没有任何东西生成侧车。** `runtimeconfig` 在 `src/` 下只出现在 launcher 的 6 处**读取**引用（`_appRuntimeConfig` 拼 `<name>.runtimeconfig.toml` 并探测存在性），`find . -name "*.runtimeconfig.*"` 结果为空——没有生成器、没有实体样例。unify-run-modes P1 的 tasks 也明确记着「`z42 publish` 侧车产出改 TOML —— **空操作**：侧车是 .NET 风格手写文件，全仓无生成器/无实体样例（探查确认）」。所以侧车现在是一个**只读不写**的机制：用户手写才有。本 change 的 5.4 是它的第一个生产者。

**两个必须先解决的模型缺口**（探查确认，直接决定 P5 工作量）：

1. **`Profile` 是固定字段而非键值袋。** [Profile.z42](../../../../src/libraries/z42.project/src/Profile.z42) 只有 `Name/Pack/Strip/Mode/Optimize/Debug` 六个字段，`ManifestLoader._parseProfiles` 只认这 5 个 key、其余**直接丢弃**。要让 profile 携带任意旋钮，必须给 `Profile` 加一个 `Knobs`（键值对）字段并让解析器收集未知 key。这不是"若现状不足"，是确定要做的一项。
2. **`mode` 的默认值是 `"interp"` 而非空。** `_parseProfiles` 里 `string mode = "interp";` —— 于是「用户没写 mode」与「用户写了 mode=interp」在模型层**不可区分**。若照现状烤侧车，一个只写了 `[profile.debug] optimize = 2` 的工程会被塞进 `mode = "interp"`，**意外压过 build 默认的 jit**，静默改变执行模式。必须先让 `Profile` 记录"是否显式给了 mode"（加 `HasMode`，或把默认改成空串由消费方兜底），否则侧车生成本身就是一个行为回归。

**范围提醒**：apphost 直跑路径**不读侧车**（`simplify-apphost-direct-run` 的已知代价，见 [launcher.md:272](../../../../docs/design/runtime/launcher.md)）。本 change 让 `z42 run` 路径的 profile 生效；apphost 也吃到侧车是那条既有的 deferred 项，不在本 change 内。

**手写侧车保护**：生成器若发现目标路径已存在且**不含生成标记头**（`# generated by z42c build — do not edit`），报错而非覆盖，并提示改用预留的 `runtimeconfig.template.toml`（本 change 不实现合并）。

**非破坏**：`ExeCount==0` 的单产物工程、以及没有 `[profile.*]` 段的工程，生成器产出**空 `[runtime]` 表或不产文件**（选后者：不产文件 → 无侧车 → 与今天字节一致）。

---

### Decision 7：运行配置只有 TOML 一种格式（User 裁决 2026-09-05）

**选定**：`Z42_CONFIG`（L3）与 `Z42_APP_CONFIG`（L4）都只读 **TOML 的 `[runtime]` 表**。不引入 JSON，也不为 JSON 预留抽象。

```rust
/// L3 与 L4 共用的唯一加载器（现有 `load_runtime_toml` 泛化为按路径读）。
/// 返回 toml_key → 原始字符串 的扁平映射。
pub fn load_config_file(path: &Path) -> Result<Option<BTreeMap<String, String>>, String>;
```

**为什么不留 `ConfigSource` trait**：unify-run-modes 已经做过这个裁决——把 .NET 风格的 JSON 侧车**收编成 TOML**（proposal D5）。z42 全仓的配置格式已统一为 TOML（`z42.toml` manifest、`~/.z42/config.toml`、`.runtimeconfig.toml`），再留一个"将来可能加 JSON"的抽象层，是为一个已被否决的方向付抽象税。真要加时再引入 trait 也不迟——那时才知道第二种格式的实际形状。

**唯一的 JSON 关照**：路径以 `.json` 结尾时给一行**迁移提示**错误，而不是让用户对着一个被忽略的文件 debug：

```
z42: Z42_CONFIG=app.runtimeconfig.json — z42 的运行配置格式是 TOML。
     请改用 app.runtimeconfig.toml（[runtime] 表）；见 docs/book/src/runtime/runtime-settings.md。
```

这与 Decision 3 的"文件层 warn 不致命"不冲突：那条讲**文件内的单个旋钮**不可用；这里是**整个文件读不了**，等价于既有的"坏 TOML 显式报错"（unify-run-modes P0 已定）。

---

### Decision 8：`--list-knobs` 默认只列 `Public`

默认列 `Public`（本表 12 条），`--all` 才列全部 24 条。GC 的六个比例旋钮是**给调优者的**，普通用户看到只增噪音与误调风险；三个元旋钮是机制内部件；`Z42_STRESS_ITERS` 是测试脚手架；`Z42_TARGET` 是占位。CoreCLR 的 `INTERNAL_/UNSUPPORTED_` 前缀正是这个作用。`--info` 保持列全部（它是 bug report 用途，需要完整快照）。

---

## Implementation Notes

- **文件拆分遵循 `.claude/rules/code-organization.md`**：`config.rs` 现 304 行，加 provenance + 渲染 + 可用性会破限。按 Scope 表拆成 `config/{knobs,parse,availability,resolve,source,render}.rs`，hub `config.rs` 只留 `RuntimeConfig` 本体 + 全局单例 + `pub use`（延续 `refactor-split-config`（2026-09-03）已建立的形状）。
- **BuiltinId 稳定性**：6 个 `__cfg_*` **追加**到 `corelib/mod.rs` 表尾（现有约定，见表内多处 "appended to preserve existing BuiltinIds" 注释）。
- **`--set` 在 clap 里的形状**：`#[arg(long = "set", value_name = "KEY=VALUE")] set: Vec<String>`，自己按第一个 `=` 切分而非用 clap `value_parser`——需要自定义错误信息与最近邻建议。
- **解析顺序**：`--set` 必须在 `init_tracing` 之前被消费（它能设 `log`）；clap 解析已在 `main()` 首行，顺序天然满足。
- **三个查询 flag 与 `<FILE>`**：`--list-knobs` / `--show-config` 与 `--info` 同类，此时 `file` 可省。现有 `main.rs` 已有"`--info` 时 file 可选"的手写检查，扩成三 flag 的集合。
- **`feature_enabled` 映射表防腐**：单测断言表内 feature 名集合 ⊇ `KNOWN_KNOBS` 所有 `requires` 引用的名字；另加一条断言表覆盖 `Cargo.toml [features]`（硬编码期望列表，Cargo.toml 加 feature 时测试失败提醒同步）。
- **P5 的自举纪律**：侧车生成改 `z42c.driver`，按 `.claude/rules/bootstrap-seed.md` 走——先 support 后 use，且 CI gen1==gen2 不动点为权威（cold worktree 本地不可验）。

---

## Testing Strategy

| 层 | 测试 |
|---|---|
| **注册表不变式**（扩展现有 47 测试）| 字母序 / 无重复 / 每个 `RuntimeConfig` 字段有表项 **+ 新增**：`requires` 名字都在 `feature_enabled` 表内；`toml_key==""` ⇒ `tier==Internal` 且 `sources ⊆ {Cli,Env}`；`aliases` 全局无重复且不与任何 `toml_key` 冲突 |
| **可用性求值** | `build`/`platforms`/`requires`/`sources` 四项的组合矩阵（注入假的 (debug, features, os, layer) 元组，**不依赖真实构建配置**，否则测试在不同 preset 下结果漂移）|
| **优先级链** | L1>L2>L3>L4>L5 两两全覆盖；`--set` 空值 = 清空回落；`--mode` 与 `--set mode=` 冲突报错 |
| **双文件层叠加** | 用户配置与应用配置各设不同 key → 合并生效；同 key → 用户赢；**用户设了 `Z42_CONFIG` 时应用侧车仍生效**（修缺陷 1 的回归门）|
| **provenance** | `source` 正确；`ignored` 四种原因各一例 |
| **诊断严重度** | CLI 不可用/不接受 → exit 2 + 消息含原因与本 build feature 列表；env 同类 → warn + 继续 + 用默认；`--strict-config` 升级为 error |
| **未知 key** | CLI 未知 → error + 最近邻建议命中（`--set gc-mod=x` 建议 `gc-mode`）；env/文件未知 → warn |
| **配置文件加载** | `.toml` 正常；`.json` 显式 Err 且消息为迁移提示（含 `.toml` 建议）；无扩展名按 toml 试；坏 TOML 仍显式 Err（回归）|
| **查询表面** | `--list-knobs` 默认 12 条 / `--all` 24 条；`--json` 经 `serde_json` 往返 + schema 断言；`--show-config` 含 `ignored` 解释行 |
| **脚本表面** | z42 端 e2e：六个 API 各一例；`Z42_GC_MODE=concurrent` 下 `Source()=="env"`；不可用旋钮 `IsAvailable()==false` |
| **侧车生成（P5）** | 有 `[profile.*]` → 产侧车且内容正确；无 profile → **不产文件**（字节不变）；已存在手写侧车 → 报错不覆盖；多 exe → 每 exe 一份；e2e：manifest `[profile.debug].mode=interp` + 用户 `Z42_CONFIG` 写 `mode=jit` → 生效 `jit`（链方向正确） |
| **非破坏** | `resolve(env, None, None, no_cli)` 逐字段 == 今天的 `from_env()`；`xtask test` golden 逐字节不变；`--info` 现有行不减少；P5 自举 gen1==gen2 |

---

## Out of Scope

- JSON 配置文件（已裁决不做；只留一行迁移提示错误）。
- `runtimeconfig.template.toml` 的手写模板合并（只留报错入口）。
- 运行期可变旋钮 / 配置热重载。
- 新增功能性旋钮（只补现有 21 条元数据 + 登记 2 个新元旋钮）。
- 从 `KNOWN_KNOBS` 生成文档页的 docgen（本 change 手写 book 页）。
- `Z42_MODE` 逐值 feature 门控迁进可用性层（保留在 `resolve_config_mode`，见 Decision 2 例外）。

# Design: 统一 test / bench 的执行画像矩阵

## Architecture

```
     ┌───────────────────────────────────────┐
     │ Std.Platform.Capabilities()/ExecModes()│ ← caps 的地面真值（stdlib 运行期函数）
     │  [Native("__platform_caps")]           │   探针程序在目标 VM 二进制下调用
     │  → ["jit","native-interop","threads"]  │   corelib/platform.rs 读 cfg flag
     └──────────────────┬────────────────────┘
                    probe emit JSON, harness 捕获
                    ▼
                    ┌──────────────────────────────────────┐
                    │  scripts/common/xtask_exec_profile.z42 │  ← 词汇 + 策略覆盖层 SoT
                    │  ─────────────────────────────────────│
                    │  ExecProfile { mode, platform, caps }  │
                    │  SUPPORT_MATRIX（策略覆盖）: 覆盖 VM 实况│
                    │    的例外——aot 编入仍 stub → skipped   │
                    │  cellStatus(profile, vmCaps) →         │
                    │      runnable | skipped-not-yet | never│
                    │  enumerateCells()                      │
                    └───────────────┬───────────────┬────────┘
                          import     │               │   import
                    ┌───────────────▼──┐        ┌────▼─────────────────┐
                    │  TEST 侧           │        │  BENCH 侧             │
                    │  xtask_test_vm     │        │  xtask_bench (e2e)    │
                    │  xtask_test_lib    │        │  xtask_test_lib(bench)│
                    │  xtask_test_platform│       │  xtask_test_platform  │
                    │  → 跑/跳格子        │        │  → 跑/跳格子 + 打 profile│
                    └────────────────────┘        └──────────┬───────────┘
                                                              │ emit
                                                    ┌─────────▼──────────┐
                                                    │ baseline-schema v2  │
                                                    │ 每项带 profile 标   │
                                                    └────────────────────┘
```

**核心思想**：模式 / 平台 / 能力不再散落在 cargo preset、CI yaml、脚本 if 分支里，而是收敛成
**一个共享描述符 + 一张支持矩阵**。test 和 bench 是这张矩阵的**两个消费者**（答复①：共享词汇 +
schema，双入口保留，不合并命令面）。

## 三根轴的真实现状（已核对源码，作为矩阵取值依据）

| 轴 | 取值 | 现状 |
|----|------|------|
| **mode**（执行组合，非标量，见 Decision 8） | `{tiers, aot_pkgs}` | mode 是**组合**：`tiers` = 活跃后端（interp 恒在），`aot_pkgs` = 预编到 AOT 的 zpkg 子集 |
| | `{[interp],[]}` | ✅ 真实（纯解释器） |
| | `{[interp,jit],[]}` | ✅ 真实（cranelift，`jit` feature，desktop x64/arm64；含单向 jit→interp fallback） |
| | `{…, aot_pkgs≠[]}`（部分/全 AOT、hybrid） | ❌ AOT 执行是 stub（`aot.rs` bail!；M9）；`aot_pkgs` 非空的**任何**组合 → 矩阵 `skipped-not-yet`。**本 change 只建模，不实现执行/配置** |
| **platform** | `linux/macos/windows × x64/arm64` | ✅ desktop（interp+jit 可跑；aot 组合 skipped） |
| | `wasm` | ✅ interp only（无 cranelift、无 dlopen、无 std::thread） |
| | `ios` / `android` | ✅ interp（preset 含 aot 占位但 stub）；有 native-interop + threads |
| **caps** | `jit` / `native-interop` / `threads` / `bundled-compression` | 均真实（threads = `corelib/threading.rs`，desktop+mobile，非 wasm） |
| | 由来 | **标准库运行时函数返回**（User 定调）：`Std.Platform.Capabilities()`（`[Native("__platform_caps")]` → `corelib/platform.rs`），探针在**被测 VM 二进制**下调用取真实能力，非静态推断。`threads` 由 `cfg!(not(wasm32))` 补（非 cargo feature） |

> **caps ⟂ mode 正交**：`caps`/`ExecModes()` 回答「这个二进制**能**做哪些后端」（stdlib 查询，
> 地面真值）；`mode` 组合回答「这次 run **用了**什么后端 + 哪些 zpkg 走 AOT」（运行配置，记进 profile）。
> 两者不混：一个 desktop 二进制 caps 恒含 jit，但某次 run 的 mode 可以是纯 interp。

## 权威支持矩阵（cellStatus 的初始表）

`runnable` = 今天能跑并记结果；`skipped` = 框架已列格子但实现未落地（报告显示 skipped + 原因）；
`never` = 物理不可能（永久 N/A，报告不列为待办）。矩阵按**平台 × mode 组合**取值：

| platform | `{[interp],[]}` | `{[interp,jit],[]}` | `aot_pkgs≠[]`（部分/全 AOT、hybrid） | threads(cap) |
|----------|:---:|:---:|:---:|:---:|
| desktop (linux/macos/windows × x64/arm64) | runnable | runnable | **skipped(M9)** | runnable |
| wasm | runnable | **never**（沙箱无 JIT） | **skipped(M9)**（wasm 用户码 AOT 延后，aot.md D4） | **never**（无 std::thread） |
| ios / android | runnable | never（preset interp-only） | **skipped(M9)**（iOS 是 AOT 首要驱动，aot.md D3） | runnable |

> **`aot_pkgs≠[]` 是一整列**，不是单格：任何非空 AOT 包集（部分 AOT `[z42.core]`、全 AOT
> `[z42.core,app]`、AOT+JIT hybrid）都归此列，今天一律 `skipped-not-yet`。M9 落地后按平台细化
> （desktop = AOT+JIT+interp；ios/wasm = AOT+interp，见 aot.md D2），届时 `cellStatus` 用
> `aot_pkgs` 是否 ⊆「该平台可 AOT 的静态 zpkg 集」判 runnable/never——**这套细化逻辑是 M9 的，
> 本 change 只把整列标 skipped 占位**。

## Decisions

### Decision 1: 统一深度 —— 共享词汇 + schema，命令面双入口（答复①）
**问题**：test 和 bench 该合并成一个入口，还是各自保留？
**选项**：A 只共享 schema/词汇，双子命令保留；B 抽共享 profile 引擎、双入口；C 合并成
`xtask run --profile`。
**决定**：**A/B 之间取「共享 SoT 模块 + 双入口」**（User 已选）。`test` 和 `bench` 仍是独立
xtask 子命令，但都 `import xtask_exec_profile.z42` 查同一矩阵。理由：C 会与已高度成熟的 CI 拓扑
（interp/jit 分腿、平台 job、6 阶段流水线）大冲突，收益不抵迁移成本；A+共享模块已消除「词汇/矩阵
各写各」的根本病，风险最低。

### Decision 2: 未来格子在矩阵里占位（答复②）
**问题**：aot / hybrid 未实现，框架里体现还是等落地再加？
**决定**：**矩阵里列出、状态标 `skipped-not-yet`**（User 已选）。符合 philosophy「设计完整、
不打补丁」——支持矩阵一次画全，`cellStatus` 返回 skipped，报告显式打印「aot: skipped (roadmap
M9)」。M9/AOT、L3/hybrid 落地时只需把该格子从 skipped 翻 runnable + 去掉 `--mode` 的
feature-gate 拒绝，schema/脚本/CI 结构不动。

### Decision 3: schema v2，不兼容 v1（倾向，待 Open Question 裁决）
**问题**：加 `profile` 字段会破坏 `additionalProperties:false` 的 v1。
**决定倾向**：按 philosophy「pre-1.0 不为旧版本兼容」→ **bump 到 `schema_version:2`，弃读 v1**，
旧 baseline 用 `xtask bench` 重生。不留 v1 fallback 分支。（列入 proposal Open Questions，
User 最终裁决。）

### Decision 4: profile.platform 结构化 {os, arch}（倾向）
单串 `linux-x64` 简单但难按轴分组；结构化 `{os:"linux", arch:"x64"}` 利于 diff 时「同 os 跨 arch」
分组与将来的 CPU/core 数扩展。倾向结构化。

### Decision 5: 平台 bench 为 informational，不进 gate（答复③的边界）
User 选「bench 也全平台矩阵」，但移动端/wasm 基准在共享 CI runner 上 ns 级噪声无意义。故：
**平台 bench 走 opt-in / nightly，只上传 artifact 供人看，不做回归门禁**。desktop 的 interp/jit
e2e diff 仍是主对比路径。这既满足「全平台矩阵覆盖」，又不制造假回归。

### Decision 6: 加速比作派生指标，不占新 tier
「jit 相对 interp 加速比」= 同 name 同 platform 下 `interp.value / jit.value`，diff 报告里
派生展示，不新增 schema tier（避免 tier 语义膨胀）。

### Decision 7: caps 是标准库运行时函数返回的属性（User 定调）
**问题**：`profile.caps` 从哪来？从平台 cargo preset 静态推断，还是运行期问运行时？以什么 surface？
**决定**：**由标准库运行时函数返回**——`Std.Platform.Capabilities()` / `ExecModes()`，沿用既有
`Std.Platform.OS()/Arch()`（`[Native("__platform_os")]` → `corelib/platform.rs`）的模式，新增
builtin `__platform_caps` / `__platform_exec_modes`。bench/test 用一个 z42 **能力探针**程序在
**被测 VM 二进制**下调用该 stdlib 函数、拿真实 caps。理由：
- **z42 原生 + 遵循先例**：xtask / bench harness / test-runner 全是 z42，直接调 stdlib 函数即可，
  无需起子进程解析 CLI 文本。`Std.Platform` 已是「运行时/平台身份」的既定家，caps 是它的自然扩展。
- **权威性**：能力是「这个二进制到底编入了什么」，只有运行时自己知道；builtin 读 `cfg!(feature=…)`
  就是编译期真相。静态从 preset 推断会在自定义 feature 组合 / 交叉构建时错。
- **含 threads**：threads 非 cargo feature（恒编入非 wasm），builtin 用 `cfg!(not(wasm32))` 补，
  于是「多线程能力」也成为 stdlib 可查询的一等属性。
- **矩阵职责收窄**：`SUPPORT_MATRIX` 不枚举「哪平台有哪 cap」（交给运行时实况），只保留**策略覆盖层**
  ——`aot` 即便 feature 编入、`ExecModes()` 会列它，但 `aot.rs` 仍 bail → 覆盖为 `skipped-not-yet`。
  `cellStatus(profile, vmCaps)` 先看实况、再叠策略覆盖。
- **探针为何必要**：harness 自身 VM 未必等于被测 VM（平台 bench 下被测是 wasm/mobile 构建）。
  故 caps 必须由**跑在被测 VM 下的探针**调 stdlib 函数取得，而非 harness 进程自报。desktop 下
  两者同一二进制、探针仍走同一路径（统一、无特例）。
**代价**：本变更为 **stdlib + runtime + toolchain** 跨子系统（锁状态见下「调度约束」）。

### Decision 8: mode 是「执行组合」而非标量，统一 partial-AOT（User 定调）
**问题**：AOT 可以是**部分的、以 zpkg 为单位**的（aot.md D2/D8：AOT 化随包静态 zpkg，动态加载的
走 interp/jit）。标量 `interp|jit|aot` 无法表达「z42.core 走 AOT、其余走 JIT」这类混合。怎么统一？
**决定**：`profile.mode` 建成**执行组合** `{ tiers: [...], aot_pkgs: [...] }`：
- `tiers` = 本次 run 活跃的后端集（interp 恒在，可选 +jit）。
- `aot_pkgs` = 被预编到 AOT 的 zpkg 逻辑名子集（今天恒 `[]`）。
- **统一性**：interp / jit / 部分AOT / 全AOT / 「混合执行」全是同一形状的不同取值，无特例分支：
  - 纯 interp = `{[interp], []}`；纯 jit = `{[interp,jit], []}`
  - 部分 AOT（desktop）= `{[interp,jit], [z42.core]}`；iOS 全随包 AOT = `{[interp], [z42.core,app]}`
- **「混合执行」就此消解**：hybrid 不是第 4 个枚举，而是 `aot_pkgs≠[]` 或 `|tiers|>1` 的一般情形
  （对标 .NET R2R+TieredJIT / Android ART，aot.md D2）。之前「hybrid 是独立 mode」的说法**作废**。
- **规范化标签**（可读 + diff 键）：`interp` / `jit` / （未来）`jit+aot[z42.core,z42.math]`。
- **本 change 边界（User 明确）**：**只建模，不实现**。今天 harness 恒发 `aot_pkgs:[]`；`aot_pkgs`
  非空的组合一律 `skipped-not-yet`。**AOT 的实际执行 + 配置面（z42.toml per-package / CLI
  `--aot-pkg`：哪些 zpkg 走 AOT）都归 M9**，不在本 change。框架只需数据模型能**表示**组合，
  不产出 AOT run、不解析 AOT 配置。M9 落地时把 skipped 列翻 runnable + 加配置解析即可，schema/
  harness 结构不动。
**为何不推给 M9 再设计 mode 形状**：若现在把 mode 建成标量，M9 要加 partial-AOT 时得改 schema
（v2→v3）+ 所有 diff 分组逻辑。现在一次建成组合，是「设计完整、不打补丁」（philosophy）——今天多
写一个 `aot_pkgs:[]` 字段的成本，远小于将来 schema 破坏性升级。

## bench baseline schema v2（草案）

```jsonc
{
  "schema_version": 2,
  "commit": "9dde4ec", "branch": "main", "timestamp": "…",
  "z42vm_version": "0.30.1",          // 替代 dotnet_version
  "rustc_version": "1.8x",
  "benchmarks": [{
    "name": "01_fibonacci",
    "tier": "z42-e2e",               // enum: rust-micro | z42-e2e | z42-micro  (删 csharp-throughput)
    "metric": "time",
    "value": 32.4, "unit": "ms",
    "ci_lower": 31.8, "ci_upper": 33.1, "samples": 10,
    "profile": {                     // ← 新增，一等维度
      "mode": {                      // 执行组合（非标量，Decision 8）
        "tiers": ["interp", "jit"],  // 活跃后端（interp 恒在）
        "aot_pkgs": []               // 预编 AOT 的 zpkg 子集；今天恒 []，非空 → skipped(M9)
      },
      "mode_label": "jit",           // 规范化标签（可读 + diff 键）：interp|jit|jit+aot[…]
      "platform": { "os": "linux", "arch": "x64" },
      "caps": ["jit", "native-interop", "threads"]   // 运行期查询：Std.Platform.Capabilities()
    }
  }]
}
```

diff（`--diff`）改为**同 profile 内**对比：`(name, metric, mode_label, platform)` 四元组匹配
（`mode_label` 由 `{tiers,aot_pkgs}` 规范化派生，含 AOT 包集 → AOT-baseline 不会与纯 JIT 误比），
避免跨模式/跨平台误比。baseline 文件命名 `main-<os>-<arch>-<mode_label>.json`（或单文件多 profile 项）。

## Implementation Notes

- **能力 builtin**：`corelib/platform.rs` 加 `__platform_caps`（返回 `string[]`）+
  `__platform_exec_modes`，用 `cfg!(feature=…)` 收集 jit/native-interop/bundled-compression +
  `cfg!(not(target_arch="wasm32"))` 加 threads；`corelib/mod.rs` 注册。`Std.Platform` 加对应
  `[Native]` extern + `HasJit()/HasThreads()/…` 谓词（沿用 `IsLinux()` 风格）。
- **能力探针**：`bench/probe/capabilities.z42` 调 `Std.Platform.Capabilities()`/`ExecModes()`/
  `OS()`/`Arch()` 打印一行 JSON；harness 编它为 `.zbc`、在**目标 VM 二进制**下跑一次、解析、
  缓存进 profile（每个被测 VM 只探一次）。
- `xtask_exec_profile.z42` 的支持矩阵（策略覆盖层）用**显式 sort 后的稳定结构**（common-pitfalls
  §1：任何枚举/注册循环先按稳定键排序，禁止依赖 FS/Dict 顺序）。`enumerateCells()` 输出有序。
  `cellStatus(profile, vmCaps)`：先据 vmCaps 判「已编入」，再叠策略覆盖（aot→skipped）。
- e2e bench 传模式：把现有 `hyperfine "$vm $zbc Main"`（`xtask_bench.z42:61`，无 mode）改为
  `hyperfine "$vm $zbc Main --mode <m>"`，对 `both` 生成两条 hyperfine 命令、两组结果项。
- `--mode` 的 feature-gate 拒绝复用 VM 现状（clap 只 advertise 编入的模式）；bench 脚本在
  `cellStatus == never/skipped` 时**主动跳过并 log**（common-pitfalls：不静默截断——skip 要打印）。
- 平台 bench 复用 `xtask_test_platform.z42` 的 build→assets→run 三段，run 段增基准采集分支；
  wasm/mobile 的 caps 同样由探针在该平台 VM 下调 `Std.Platform.Capabilities()` 取得（不从 preset 推断）。
- micro（`bench stdlib`）已有 `--mode`，只需在 `MicroBenchAgg`（`xtask_bench.z42:221`）产出项
  里补 `profile` 字段。

## Testing Strategy

- **单元**：`xtask_exec_profile.z42` 的 `cellStatus` 各一用例：`{[interp,jit],[]}`@wasm=never、
  `{…,aot_pkgs≠[]}`@desktop=skipped、`{[interp,jit],[]}`@desktop=runnable、threads@wasm=never
  （据探针 vmCaps）、`mode_label` 规范化（`{[interp,jit],[z42.core]}`→`jit+aot[z42.core]`）（放
  `scripts` 侧单测或 z42c 单测目录）。
- **schema 校验**：新 v2 结果文档用 `bench/baseline-schema.json` 校验通过；v1 残留文档应被拒
  （证明 additionalProperties 收紧到 v2）。
- **e2e 冒烟**：`xtask bench --quick --mode both` 产出含 interp+jit 两组 profile 项、diff 同
  profile 对比、加速比正确。
- **线程场景**：`06_thread_scaling.z42` 在 1/2/4 线程下产出可复现聚合值（Assert 自验证 +
  bench 采样）。
- **GREEN gate**：纯 toolchain 变更 → `xtask test`（含 `test compiler` 自举不动点，确认 bench
  脚本改动不破坏 xtask.zpkg 自建）+ `xtask bench --quick` 冒烟。平台 bench 本地按需
  （`xtask test platform <p> bench`）、CI informational。

## Deferred / Future Work

### exec-profile-matrix-future-aot-composition-cells
- **来源**：本 proposal 决策 2 + 决策 8
- **触发原因**：`aot_pkgs≠[]` 的执行组合（部分/全 AOT、AOT+JIT hybrid）VM 侧未实现——AOT 执行是
  `aot.rs` stub（M9），且「哪些 zpkg 走 AOT」的**配置面**（z42.toml per-package / CLI `--aot-pkg`）
  也归 M9。
- **前置依赖**：`aot.rs` 落地（M9，cranelift-AOT，aot.md）；per-zpkg AOT 配置解析（M9）
- **触发条件**：M9 完成后，把 `aot_pkgs≠[]` 列从 skipped 翻 runnable（按平台细化 aot.md D2 组合）+
  加配置解析 + 探针在 AOT 组合下捕获 mode。schema `{tiers,aot_pkgs}` 结构**已就位、无需改**。
- **当前 workaround**：mode 组合已能**表示** AOT（`aot_pkgs` 字段），但 harness 恒发 `[]`；非空组合
  `cellStatus` 返 `skipped-not-yet`，报告显式打印跳过原因。

### exec-profile-matrix-future-platform-bench-gating
- **来源**：本 proposal 决策 5
- **触发原因**：移动端/wasm 基准在共享 CI runner 噪声大，暂不做回归门禁
- **触发条件**：有稳定专用基准硬件（self-hosted runner）后，可把平台 bench 纳入 nightly 宽阈值门禁
- **当前 workaround**：平台 bench informational / opt-in，只上传 artifact

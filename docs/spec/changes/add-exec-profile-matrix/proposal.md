# Proposal: 统一 test / bench 的执行画像矩阵（exec-profile matrix）

## Why

z42 VM 有多个执行模式（interp / jit，未来 aot）、多个目标平台（linux/macos/windows ×
x64/arm64、wasm、ios、android）、多种运行时能力（jit / native-interop / 多线程 …，以
cargo feature 表达）。**测试侧**已经把「模式」做成一等维度（`test e2e --mode interp|jit`、
CI 分 interp/jit 腿、`test platform`），但**基准侧几乎没有模式维度**：

- e2e bench（`scripts/xtask_bench.z42`）跑 `hyperfine "$vm $zbc Main"` **不带 `--mode`**
  → 永远只测 VM 默认模式（jit）。**有 JIT 却从不测「JIT 比 interp 快多少」**——基准最核心
  的价值缺失；更糟的是同一 baseline 文件可能悄悄混入不同模式（一台默认 jit、一台默认 interp）
  而无从分辨（diff 只按 `name`+`metric` 匹配）。
- `bench/baseline-schema.json` 只有粗粒度 `os` 字符串，**无 `mode`、无 `platform/arch`、
  无 `capabilities`** 字段，且 `additionalProperties:false` → 不改 schema 加不进去。
- schema 与 README 残留**已删除的 C# 时代**：`tier` enum 仍有 `csharp-throughput`、字段仍有
  `dotnet_version`、README 仍宣传 `just bench-compiler-all`（且**仓库根本没有 justfile**，
  所有 `just bench-*` 命令是死的）。Rust criterion 微基准存在于 `src/runtime/benches/` 但
  **未接入 xtask**。
- test 与 bench 的「模式/平台/能力」词汇各写各的，没有共享的支持矩阵 SoT——哪些
  `{平台 × 模式 × 能力}` 格子该跑、哪些该跳、哪些永远不可能（wasm 无法 JIT），散落在
  cargo preset、CI yaml、脚本 if 分支里，无单一真相来源。

不做的后果：JIT/未来 AOT 的性能回归无人守护；跨平台/跨能力的基准互相撞车（baseline 按
`main-<os>.json` 命名，跨 arch/能力冲突）；未来 AOT（M9）、线程可扩展性基准落地时又要各改一遍。

## What Changes

1. **caps = 标准库运行时函数（`Std.Platform.Capabilities()`）**：`caps` 是运行期属性，由
   **标准库运行时函数返回**（User 定调），沿用既有 `Std.Platform.OS()/Arch()` 模式
   （`[Native("__platform_os")]` → `corelib/platform.rs`）。给 `Std.Platform` 增
   `Capabilities()` / `ExecModes()` / `HasJit()` / `HasThreads()` / `HasNativeInterop()` /
   `HasAot()`，由新 VM builtin `__platform_caps` / `__platform_exec_modes` 背书，读同一套
   `cfg!(feature=…)` + `cfg!(not(target_arch="wasm32"))`（threads 恒编入非 wasm，故不是 feature）。
   bench/test 用一个 z42 **能力探针**程序在**目标 VM 二进制**下调用该 stdlib 函数、捕获真实 caps
   ——纯 z42 原生路径，不解析任何 CLI。
2. **共享「执行画像」词汇 + 支持矩阵 SoT**：新增一个共享脚本模块，定义
   `profile = { mode, platform, caps }`，其中 **`mode` 是执行组合 `{tiers, aot_pkgs}` 而非标量**
   （见第 3 点）；配一张**权威支持矩阵**——每个 `{platform × mode组合 × cap}` 格子标注
   `runnable` / `skipped-not-yet`（`aot_pkgs≠[]` 的组合）/ `never`（wasm+jit）。**运行期返回的
   exec_modes/caps 是「已编入」的实况地面真值**；静态矩阵只作**策略覆盖层**（如 AOT 执行仍 stub →
   `aot_pkgs≠[]` 恒 `skipped`）。test 与 bench **都查这张表 + VM 实况**决定跑哪些格子、跳哪些。
3. **`mode` = 执行组合，统一 partial-AOT（`{tiers, aot_pkgs}`）**：AOT 是**按 zpkg 为单位、可部分**
   的（aot.md D2/D8），标量 `interp|jit|aot` 表达不了「z42.core 走 AOT、其余走 JIT」。故 `mode` 建成
   组合：`tiers`=活跃后端（interp 恒在，可 +jit）、`aot_pkgs`=预编 AOT 的 zpkg 子集。interp/jit/
   部分AOT/全AOT/「混合执行」皆同一形状不同取值。**本 change 只建模**：今天恒发 `aot_pkgs:[]`，非空
   组合一律 skipped；**AOT 执行 + 配置面归 M9**。
4. **bench 结果 schema v2**：per-benchmark 项加结构化 `profile`（`mode`=`{tiers,aot_pkgs}` +
   `mode_label` + `platform`(os+arch) + `caps`，caps 取自运行期 stdlib 查询）；顶层加
   `z42vm_version`；**删除** stale 的 `csharp-throughput` tier 与 `dotnet_version` 字段。
   `main-<os>.json` 命名升级为按 profile 键。
5. **bench e2e 补模式扫描**：e2e runner 接受 `--mode interp|jit`（可 `both`），把
   `--mode` 传给 hyperfine 的被测进程，结果按 `mode_label` 打标；新增「jit 相对 interp 加速比」
   作为一等派生指标。
6. **bench 多线程可扩展性场景**（多线程今天真实可用）：加至少一个 `Std.Threading`
   spawn/join 的可扩展性场景，`caps` 标 `threads`（由 stdlib 运行期函数返回）。
7. **bench 平台矩阵接入**（答复③：bench 也全平台）：复用 `test platform` 的 fixture 编排，
   让 wasm/ios/android 也能跑一组 e2e 基准，profile 的 `platform` 打对应标签。**平台 bench
   为 informational / opt-in，不进 gate**（共享 runner 上移动端 ns 级噪声无意义）。
8. **未来格子占位**（答复②）：矩阵里列出 `aot_pkgs≠[]` 的组合列，状态标 `skipped-not-yet`，
   CI/本地报告显示为「skipped（M9 未落地）」。M9 落地时把该列翻 `runnable` + 加配置解析即可，
   schema `{tiers,aot_pkgs}` 结构不动。
9. **文档/死引用清理**：`bench/README.md` 去掉 justfile / C# / 已失效命令；把「模式×平台×能力」
   矩阵与运行方式写清；把 Rust criterion tier 现状（未接入 xtask）如实标注或接入。

## Scope（允许改动的文件）

> **占用子系统（跨子系统）**：`stdlib`（`Std.Platform` 能力函数）+ `runtime`（`__platform_caps`
> builtin）+ `toolchain`（scripts/）。**锁状态见文末「调度约束」**。

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/Platform.z42` | MODIFY | 加 `Capabilities()` / `ExecModes()` / `HasJit()` / `HasThreads()` / `HasNativeInterop()` / `HasAot()`（`[Native]` extern，沿用 `__platform_os` 模式） |
| `src/runtime/src/corelib/platform.rs` | MODIFY | 加 `__platform_caps` / `__platform_exec_modes` builtin，读 `cfg!(feature=…)` + threads via `cfg!(not(target_arch="wasm32"))` |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 注册新 builtin |
| `bench/probe/capabilities.z42` | NEW | 能力探针：调 `Std.Platform.Capabilities()`/`ExecModes()` 打印 JSON，harness 在目标 VM 下运行捕获 |
| `scripts/common/xtask_exec_profile.z42` | NEW | 执行画像（`mode`=`{tiers,aot_pkgs}` 组合 + platform + caps）+ 支持矩阵 SoT（策略覆盖层）+ 跑探针取 caps + `mode_label` 规范化 + skip 判定（共享词汇，test/bench 都 import） |
| `bench/baseline-schema.json` | MODIFY | schema v2：加 `profile`(mode=`{tiers,aot_pkgs}`+mode_label/platform/caps)、`z42vm_version`；删 `csharp-throughput`/`dotnet_version` |
| `scripts/xtask_bench.z42` | MODIFY | e2e：`--mode` 传被测进程 + 模式扫描 + 结果打 profile 标 + platform(os+arch)；接入支持矩阵跳格；加速比派生指标 |
| `scripts/xtask_cli.z42` | MODIFY | `bench`(e2e) parser 加 `--mode` / `--caps` / e2e `--json`；`bench` 帮助文案 |
| `scripts/test/xtask_test_lib.z42` | MODIFY | `bench stdlib` 结果并入 profile 词汇（micro 已有 `--mode`，补 profile 打标） |
| `scripts/test/xtask_test_platform.z42` | MODIFY | 平台 bench 编排入口（复用 build/assets/run 三段，加 `bench` 动作或子步） |
| `bench/scenarios/06_thread_scaling.z42` | NEW | 多线程 spawn/join 可扩展性场景（caps=threads） |
| `bench/README.md` | MODIFY | 去死引用（justfile/C#）；写模式×平台×能力矩阵 + 运行方式 |
| `docs/design/testing/exec-profile-matrix.md` | NEW | 设计原理：三元组、支持矩阵、skip 语义、schema v2、加速比指标（知识上浮） |
| `docs/book/src/dev/test-gate.md` | MODIFY | 若 gate 组成受影响（模式腿说明对齐），否则只读引用 |
| `.github/workflows/bench-pr.yml` | MODIFY | e2e bench PR 腿：跑 interp+jit 双模式（informational） |
| `.github/workflows/bench-update.yml` | MODIFY | main baseline 持久化按 profile 键存 |

**只读引用**（理解上下文，不修改）：

- `src/runtime/src/metadata/types.rs`（`ExecMode` 枚举）、`src/runtime/src/main.rs`（`--mode`
  解析 + `--info` capability tag）、`src/runtime/Cargo.toml`（feature preset = 能力/平台 SoT 来源）
- `src/runtime/src/corelib/threading.rs` / `sync.rs`（多线程 API，写线程场景参考）
- `scripts/test/xtask_test_vm.z42` / `xtask_test_lib_units.z42`（test 侧 `--mode` 传递现状，作对齐参照）
- `docs/roadmap.md`（M9 AOT / L3 线程 / 混合执行的 milestone 归属）

## Out of Scope

- **不实现 AOT 执行**：`mode` 组合能**表示** `aot_pkgs≠[]`，但仅矩阵占位标 skipped；AOT 执行仍是
  `aot.rs` 的 stub（M9）。
- **不设计/实现 partial-AOT 配置面**：「哪些 zpkg 走 AOT」的配置（z42.toml per-package /
  CLI `--aot-pkg`）**归 M9**——本 change 只让 profile 数据模型能表示组合，不解析任何 AOT 配置。
- **不做函数级 per-function 模式**：`Function.exec_mode` 元数据虽被解码但 VM 从不据其派发
  （只用 module-level `default_mode`）；本变更不碰这条死路径。
- **不把 micro（`[Benchmark]` / criterion）纳入 CI 硬门禁**：沿用现状（ns 级共享 runner 噪声大），
  仅本地/nightly；本变更只统一其**结果打标**，不改其 gate 策略。
- **不改 VM 执行语义**：runtime 侧改动仅限**新增能力查询 builtin**（`__platform_caps`，读 cfg
  flag、返回字符串数组）——纯只读，沿用 `__platform_os` 既有模式，不碰 interp/jit 派发、GC、指令集。
  stdlib 侧只加 `Std.Platform` 几个 extern 函数。其余为 toolchain（脚本）+ schema + 文档变更。

## Open Questions

- [ ] schema v2 是 bump `schema_version:2` 并弃读 v1，还是保留 v1 读取？（按 philosophy「不为旧版本
      兼容」倾向直接 v2、旧 baseline 重生）——待 User 裁决。
- [ ] `profile.platform` 用 `{os, arch}` 两字段，还是单串 `linux-x64`？（结构化利于 diff 分组，
      单串简单）——设计倾向结构化。
- [x] ~~caps 记全量 tag 还是子集？~~ **已定（User）：caps 是标准库运行时函数返回的属性**——
      `Std.Platform.Capabilities()`，探针在目标 VM 下调用、取真实全量 caps（含 threads），不静态推断。
- [x] ~~VM 能力查询用 CLI flag？~~ **已定（User）：走 stdlib 运行时函数**（`Std.Platform`），
      背书 builtin `__platform_caps`，非 CLI。放弃 `--info --json` 方案。
- [ ] 能力函数返回形态：`Capabilities()` 返回 `string[]`（["jit","threads",…]）为主，是否同时提供
      `HasJit()`/`HasThreads()` 布尔谓词（.NET 风格，与现有 `Platform.IsLinux()` 一致）？
      ——设计倾向两者都给（数组供探针 emit，谓词供 z42 代码可读判断）。

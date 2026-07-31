# Spec: Exec-Profile Matrix（执行画像矩阵）

## ADDED Requirements

### Requirement: 标准库运行时能力查询（caps 的权威来源）

标准库提供运行时能力查询函数 `Std.Platform.Capabilities()` / `ExecModes()`（`[Native]`
builtin 背书），反映**该 VM 二进制真实编入的能力**，含 `threads`（非 wasm）。test/bench 用一个
在**被测 VM 二进制**下运行的 z42 探针调用该函数、以其返回值填充 `profile`，不静态推断。

#### Scenario: desktop 运行时报告 jit + threads
- **WHEN** 在 desktop release z42vm 下调用 `Std.Platform.Capabilities()` / `ExecModes()`
- **THEN** `ExecModes()` 含 `jit`，`Capabilities()` 含 `jit`/`native-interop`/`threads`

#### Scenario: wasm 运行时报告无 jit、无 threads
- **WHEN** 在 wasm(interp-only) z42vm 下调用同函数
- **THEN** `ExecModes()` 仅 `interp`，`Capabilities()` 不含 `jit`、不含 `threads`

#### Scenario: HasX 谓词与数组一致
- **WHEN** `Capabilities()` 含 `threads`
- **THEN** `HasThreads()` 返回 true（谓词与数组同源，不漂移）

#### Scenario: caps 取自被测二进制而非静态推断
- **WHEN** bench/test 记录一个结果项的 `profile.caps`
- **THEN** 其值等于探针在对应 VM 二进制下 `Capabilities()` 的返回（逐项一致）

### Requirement: 共享执行画像描述符与支持矩阵 SoT

系统提供单一模块，定义 `ExecProfile { mode, platform, caps }` 与一张权威支持矩阵，
test 与 bench 均以它判定哪些 `{platform × mode组合 × cap}` 格子该运行 / 跳过 / 永不适用。
`mode` 是执行组合 `{tiers, aot_pkgs}`（非标量）。

#### Scenario: desktop 上 jit 组合是可运行格子
- **WHEN** 查询 `cellStatus({mode:{tiers:[interp,jit], aot_pkgs:[]}, platform:{os:linux,arch:x64}})`
- **THEN** 返回 `runnable`

#### Scenario: wasm 上 jit 组合永不适用
- **WHEN** 查询 `cellStatus({mode:{tiers:[interp,jit], aot_pkgs:[]}, platform:{os:wasm}})`
- **THEN** 返回 `never`（报告中标记为 N/A，不列为待办）

#### Scenario: 任何非空 AOT 包集为未落地占位
- **WHEN** 查询 `cellStatus({mode:{tiers:[interp,jit], aot_pkgs:[z42.core]}, platform:<任意>})`
  （部分 AOT），或 `aot_pkgs:[z42.core,app]`（全随包 AOT）
- **THEN** 均返回 `skipped-not-yet`，附原因指向 roadmap M9（`aot_pkgs≠[]` 整列占位）

#### Scenario: mode_label 规范化
- **WHEN** 对 `{tiers:[interp], aot_pkgs:[]}` / `{tiers:[interp,jit], aot_pkgs:[]}` /
  `{tiers:[interp,jit], aot_pkgs:[z42.core]}` 派生 `mode_label`
- **THEN** 分别得 `interp` / `jit` / `jit+aot[z42.core]`（稳定、可作 diff 键）

#### Scenario: threads 格子状态由运行时实况决定
- **WHEN** 以 wasm VM 的能力查询结果调用 `cellStatus({cap:threads}, vmCaps)`
- **THEN** 返回 `never`（vmCaps.caps 不含 threads）；对 desktop VM 则返回 `runnable`

#### Scenario: 枚举顺序稳定
- **WHEN** 两次调用 `enumerateCells()`
- **THEN** 返回顺序完全一致（按稳定键排序，不依赖 FS/Dict 迭代顺序）

### Requirement: bench 结果 schema v2 携带执行画像

benchmark 结果每项带结构化 `profile`（mode + platform + caps），顶层带 `z42vm_version`；
移除已删除 C# 时代的 `csharp-throughput` tier 与 `dotnet_version` 字段。

#### Scenario: v2 结果含 profile 且校验通过
- **WHEN** `xtask bench` 产出结果并按 `baseline-schema.json`（v2）校验
- **THEN** 每个 benchmark 项含 `profile.mode.{tiers,aot_pkgs}` / `profile.mode_label` /
  `profile.platform.{os,arch}` / `profile.caps`，校验通过；今天 `aot_pkgs` 恒为 `[]`

#### Scenario: 拒绝 stale csharp tier
- **WHEN** 一个 benchmark 项的 `tier` 为 `csharp-throughput`
- **THEN** schema 校验失败（enum 已删该值）

#### Scenario: 无 profile 的旧 v1 文档被拒
- **WHEN** 用 v2 schema 校验一个缺 `profile`、`schema_version:1` 的旧文档
- **THEN** 校验失败（additionalProperties/必填收紧到 v2）

### Requirement: bench e2e 支持模式扫描

e2e 基准接受 `--mode interp|jit|both`，把模式传给**被测进程**（而非仅编译步），结果按模式打标。

#### Scenario: both 模式产出两组打标结果
- **WHEN** 运行 `xtask bench --quick --mode both`
- **THEN** 每个场景产出两个 benchmark 项，`profile.mode_label` 分别为 `interp` 与 `jit`
  （`mode.tiers` 分别 `[interp]` / `[interp,jit]`，`aot_pkgs` 均 `[]`）

#### Scenario: 被测进程按指定模式运行
- **WHEN** 运行 `xtask bench --mode interp`
- **THEN** hyperfine 计时的 z42vm 进程带 `--mode interp`（不再是 VM 默认 jit）

#### Scenario: 请求 never 格子被显式跳过
- **WHEN** 在 wasm 平台请求 `--mode jit`
- **THEN** 跳过并打印跳过原因（不静默、不假成功）

#### Scenario: diff 同 profile 对比
- **WHEN** 对含 interp 与 jit 项的两份结果做 `xtask bench --diff`
- **THEN** 仅在 `(name, metric, mode_label, platform)` 相同的项之间比较，不跨模式误比

### Requirement: 多线程可扩展性基准场景

提供至少一个使用 `Std.Threading` spawn/join 的 e2e 场景，profile 的 caps 标 `threads`。

#### Scenario: 线程场景可复现
- **WHEN** 运行线程可扩展性场景于固定线程数
- **THEN** 产出确定的聚合结果（Assert 自验证），benchmark 项 `profile.caps` 含 `threads`

### Requirement: bench 平台维度接入（informational）

wasm/ios/android 可通过复用平台 fixture 编排跑一组 e2e 基准，profile.platform 打对应标签；
平台 bench 为 informational，不进回归门禁。

#### Scenario: 平台 bench 打对应平台标
- **WHEN** 运行 `xtask test platform wasm bench`（或等价入口）
- **THEN** 结果项 `profile.platform.os` 为 `wasm`，`profile.caps` 反映平台预设（无 jit/threads）

## MODIFIED Requirements

### Requirement: bench baseline 文档格式

**Before:** `schema_version:1`，per-项仅 `name/tier/metric/value/unit/ci_*/samples`，环境仅顶层
`os` 自由串；`tier` 含 `csharp-throughput`；顶层含 `dotnet_version`。

**After:** `schema_version:2`，per-项增 `profile{mode:{tiers,aot_pkgs}, mode_label,
platform{os,arch}, caps}`；顶层 `os` 由 `profile.platform` 取代、增 `z42vm_version`；`tier` 删
`csharp-throughput`；删 `dotnet_version`。

## Out of Scope（本 spec 不覆盖）

- **AOT 的实际执行**：`mode.aot_pkgs` 能表示，但仅矩阵占位 skipped（`aot.rs` stub，M9）
- **partial-AOT 配置面**（z42.toml per-package / CLI `--aot-pkg`：哪些 zpkg 走 AOT）——归 M9
- 函数级 per-function 模式派发（VM 未消费该元数据）
- micro/criterion 纳入 CI 硬门禁（沿用现状，仅统一打标）

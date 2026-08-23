# Spec: Runtime Counters —— 补零 + 分配/堆分代 surface

> **注**：`native_calls`/`exceptions_thrown`/`exceptions_caught` 三个计数**已于 2026-05-26 埋点生效**
> 并已在 P0 快照/JSON 中输出（本 change 核查确认，非本 change 新增）。下方"既有行为回归确认"仅作
> 回归护栏，不代表本 change 的新增功能。本 change 的新增功能是 GC 维度（HeapStats 分代 + 合并快照）。

## 既有行为回归确认（护栏，非新增）

### Requirement: native / exceptions 计数持续有效

`native_calls`（`exec_native::call_native` 入口）、`exceptions_thrown`（`fire_exception_thrown`）、
`exceptions_caught`（`fire_exception_caught`）保持既有埋点，输出到 profile 快照。

#### Scenario: throw + catch + native
- **WHEN** 脚本 `throw` 并 `try/catch` 捕获、并调用若干 native FFI
- **THEN** profile 快照中 `exceptions_thrown >= 1`、`exceptions_caught >= 1`、`native_calls >= 1`（非恒 0）

## ADDED Requirements

### Requirement: HeapStats 分代拆分

`HeapStats` 新增 `minor_collections` / `major_collections` / `reclaimed_bytes`，与既有合计
`gc_cycles` 并存（`gc_cycles` 语义不变 = 全部收集调用次数）。

#### Scenario: 分代收集发生
- **WHEN** generational 收集器执行了 m 次 minor + n 次 major 收集
- **THEN** `minor_collections == m` 且 `major_collections == n`
- **AND** `reclaimed_bytes` 单调不减，等于各周期回收字节之和

#### Scenario: 新字段默认零
- **WHEN** 堆刚创建、未发生任何收集
- **THEN** `minor_collections == 0 && major_collections == 0 && reclaimed_bytes == 0`

### Requirement: 合并 profile 快照（allocations + 堆分代进 JSON）

`--print-stats-on-exit` 的输出（text 与 `--stats-format=json` 两路）在既有 counter 字段之外，
追加 `allocations` + `minor_collections` / `major_collections` / `reclaimed_bytes`（取自
`ctx.heap().stats()`），合成**一行** JSON，沿用 `z42vm_counters` sentinel。

#### Scenario: JSON 快照含新字段
- **WHEN** 以 `--print-stats-on-exit --stats-format=json` 运行任意脚本
- **THEN** stderr 输出单行 JSON，含 `"z42vm_counters":1` 且含键 `allocations`、`minor_collections`、
  `major_collections`、`reclaimed_bytes`、`native_calls`、`exceptions_thrown`、`exceptions_caught`
- **AND** 该行可被 `xtask profile` 的 scraper 解析（后向兼容：仅新增键）

#### Scenario: xtask profile 报告展示
- **WHEN** 运行 `xtask profile <script> --heap`（或 `--all`）
- **THEN** 报告展示分配次数、GC minor/major 收集次数、reclaimed 字节
- **WHEN** `--cpu`（或 `--all`）
- **THEN** 报告展示 native 调用数、异常 thrown/caught 数（非恒 0）

## MODIFIED Requirements

### Requirement: RuntimeCounters 快照输出

**Before:** `--print-stats-on-exit` 只输出 `RuntimeCounters::Snapshot`（7 个 counter 字段，其中
`native_calls`/`exceptions_*` 恒 0），JSON 用 `z42vm_counters` sentinel。

**After:** 输出 `ProfileSnapshot`（counter 字段填实 + 追加堆派生字段 `allocations`/`minor_collections`/
`major_collections`/`reclaimed_bytes`），JSON 仍单行、仍 `z42vm_counters` sentinel（键为超集）。

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更（纯 Rust 运行期计数）。

## Pipeline Steps

受影响阶段：
- [ ] Lexer / Parser / TypeChecker / IR Codegen —— 不涉及
- [x] VM interp —— native/exception 埋点 + GC 收集器分代计数 + 快照输出合并
- [x] 工具链（xtask profile 报告）

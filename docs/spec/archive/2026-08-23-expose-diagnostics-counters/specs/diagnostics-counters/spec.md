# Spec: diagnostics-counters（运行时计数暴露给 z42）

## ADDED Requirements

### Requirement: z42 脚本可读运行时计数快照

#### Scenario: 脚本调 RuntimeStats.Counters() 拿到快照
- **WHEN** z42 脚本执行 `var c = Std.Diagnostics.RuntimeStats.Counters();`
- **THEN** 返回一个 `Std.Diagnostics.RuntimeCounters` 对象，可读以下只读属性：
  `BuiltinCalls` / `NativeCalls` / `JitMethodsCompiled` / `JitCompileUsTotal` /
  `JitNativeFromInterp` / `ExceptionsThrown` / `ExceptionsCaught` /
  `Allocations` / `MinorCollections` / `MajorCollections` / `ReclaimedBytes`（共 11 字段）

#### Scenario: 快照值与 --print-stats-on-exit JSON 一致
- **WHEN** 同一程序既调 `RuntimeStats.Counters()`（脚本内、退出前）又用 `--print-stats-on-exit --stats-format json`
- **THEN** 脚本内读到的字段值与 JSON sentinel（`z42vm_counters`）对应键在**同一时刻**语义一致
  （counter 来自 `RuntimeCounters::snapshot`，堆派生字段来自 `HeapStats`，与 app.rs 的 ProfileSnapshot 同源）

#### Scenario: 分配会反映到 Allocations 字段
- **WHEN** 脚本先 `RuntimeStats.Counters()` 记 `a0.Allocations`，分配若干对象后再 `RuntimeStats.Counters()` 记 `a1.Allocations`
- **THEN** `a1.Allocations > a0.Allocations`（单调递增，来自 `HeapStats.allocations` SoT，不重复计数）

### Requirement: 新 builtin 遵守 BuiltinId append-only 纪律

#### Scenario: __diag_counters append 在 BUILTINS 末尾
- **WHEN** 在 `corelib/mod.rs` 注册 `__diag_counters`
- **THEN** 它 append 在 `BUILTINS` 数组**末尾**（带日期注释），既有 builtin 的 `BuiltinId` 不移位
  （已编译 zbc / 跨 nightly 不破坏）

#### Scenario: 未知 builtin 名不静默返回错值
- **WHEN** VM dispatch `__diag_counters`
- **THEN** 命中 `builtin_diag_counters`，返回 `Std.Diagnostics.RuntimeCounters` 对象；
  投影字段数 == 11，缺一即单测失败（防止投影漏字段）

### Requirement: allocations 分配回归 gate（informational）

#### Scenario: CI 打印 allocations 但不 fail
- **WHEN** CI 跑固定 gate 脚本取 `allocations` 并与 baseline 比对
- **THEN** CI **打印**当前值 vs baseline 差异（`informational: allocations X (baseline Y, Δ Z)`），
  **无论差异多大都不 fail 该 job**（观察阶段，阈值待后续 change 定）

#### Scenario: gate 脚本确定性
- **WHEN** 同一 gate 脚本在同一 GC-mode 下跑两次
- **THEN** `allocations` 完全相同（确定性，非 wall-time 噪声）；跨 OS / GC-mode 差异记入观察日志

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更（`[Native]` 是运行时解析的字符串 attribute，
新 builtin 仅追加 `BUILTINS` 表项，不改二进制格式）。

## Pipeline Steps

- [ ] Lexer —— 无
- [ ] Parser / AST —— 无
- [ ] TypeChecker —— 无（`extern` 方法 + 自定义返回类型走既有路径）
- [ ] IR Codegen —— 无
- [x] VM interp —— 新 builtin `builtin_diag_counters` + BUILTINS 注册
- [x] stdlib —— 新 `Std.Diagnostics.RuntimeStats` / `RuntimeCounters`
- [x] CI —— informational allocations gate
